//! The interactive read–parse–execute loop.

use std::io::{self, BufRead, Write};

use crate::commands::{self, Flow};
use crate::parser::{self, ParseError};
use crate::{sys, util};

const PROMPT: &str = "$ ";
const CONTINUATION_PROMPT: &str = "> ";
const STATUS_NOT_FOUND: i32 = 127;

enum Line {
    Text(String),
    /// Ctrl+C was pressed while waiting for input.
    Interrupted,
    /// End of input (Ctrl+D).
    Eof,
}

enum Input {
    Words(Vec<String>),
    Interrupted,
    Eof,
}

/// Runs the shell until `exit` or end of input and returns the exit status.
pub fn run() -> i32 {
    let stdin = io::stdin();
    let mut last_status = 0;

    loop {
        let words = match next_command(&stdin) {
            Input::Words(words) => words,
            Input::Interrupted => continue,
            Input::Eof => {
                if sys::stdin_is_tty() {
                    println!("exit");
                }
                return last_status;
            }
        };

        let (name, args) = match words.split_first() {
            Some(parts) => parts,
            None => continue,
        };

        match commands::execute(name, args, last_status) {
            Some(Flow::Continue(status)) => last_status = status,
            Some(Flow::Exit(status)) => return status,
            None => {
                report_not_found(name);
                last_status = STATUS_NOT_FOUND;
            }
        }
    }
}

fn report_not_found(name: &str) {
    eprintln!("Command '{name}' not found");
}

/// Reads one complete command, asking for more lines while a quote is open.
///
/// Standard input is locked only while reading, never while a command runs:
/// commands such as `cat` read from stdin themselves and would deadlock on a
/// lock held here. Buffered input is kept by `Stdin`, so nothing is lost.
fn next_command(stdin: &io::Stdin) -> Input {
    let mut input = stdin.lock();
    let input = &mut input;
    let mut text = match prompt_for_line(input, PROMPT) {
        Line::Text(text) => text,
        Line::Interrupted => return Input::Interrupted,
        Line::Eof => return Input::Eof,
    };

    loop {
        match parser::tokenize(&text) {
            Ok(words) => return Input::Words(words),
            Err(ParseError::UnterminatedQuote) => match prompt_for_line(input, CONTINUATION_PROMPT)
            {
                Line::Text(more) => {
                    text.push('\n');
                    text.push_str(&more);
                }
                Line::Interrupted => return Input::Interrupted,
                Line::Eof => {
                    util::error("0-shell", "unexpected EOF while looking for matching quote");
                    return Input::Eof;
                }
            },
        }
    }
}

fn prompt_for_line(input: &mut impl BufRead, prompt: &str) -> Line {
    // A Ctrl+C that arrived while a command was running must not cancel this read.
    sys::take_interrupt();
    print!("{prompt}");
    let _ = io::stdout().flush();

    match read_line(input) {
        Ok(Some(text)) => Line::Text(text),
        Ok(None) => Line::Eof,
        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
            println!();
            Line::Interrupted
        }
        Err(err) => {
            util::error("0-shell", &format!("read error: {}", util::describe(&err)));
            Line::Eof
        }
    }
}

/// Reads up to the next newline.
///
/// `BufRead::read_line` silently retries when a signal interrupts the read, which
/// would make Ctrl+C invisible, so the loop is written by hand to surface
/// `ErrorKind::Interrupted`. Returns `None` only at end of input, so Ctrl+D on a
/// partially typed line is ignored, as in other shells.
fn read_line(input: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(if bytes.is_empty() {
                None
            } else {
                Some(decode(bytes))
            });
        }
        match available.iter().position(|&byte| byte == b'\n') {
            Some(index) => {
                bytes.extend_from_slice(&available[..index]);
                input.consume(index + 1);
                return Ok(Some(decode(bytes)));
            }
            None => {
                let length = available.len();
                bytes.extend_from_slice(available);
                input.consume(length);
            }
        }
    }
}

fn decode(mut bytes: Vec<u8>) -> String {
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
