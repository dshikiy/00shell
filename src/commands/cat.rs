//! `cat [FILE...]`: print files to standard output (standard input for no file or `-`).

use std::fs::{self, File};
use std::io::{self, Read, Write};

use crate::{sys, util};

const BUFFER_SIZE: usize = 64 * 1024;

pub fn run(args: &[String]) -> i32 {
    let parsed = match util::parse_flags("cat", args, "") {
        Ok(parsed) => parsed,
        Err(_) => return util::STATUS_USAGE,
    };

    if parsed.operands.is_empty() {
        return status(copy_stdin("-"));
    }

    let mut ok = true;
    for operand in &parsed.operands {
        let done = if operand == "-" {
            copy_stdin(operand)
        } else {
            copy_file(operand)
        };
        ok &= done;
    }
    status(ok)
}

fn status(ok: bool) -> i32 {
    if ok {
        0
    } else {
        util::STATUS_FAILURE
    }
}

fn copy_file(path: &str) -> bool {
    if fs::metadata(path)
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
    {
        util::error("cat", &format!("{path}: Is a directory"));
        return false;
    }
    match File::open(path) {
        Ok(file) => copy_stream(file, path),
        Err(err) => {
            util::error("cat", &format!("{path}: {}", util::describe(&err)));
            false
        }
    }
}

fn copy_stdin(label: &str) -> bool {
    copy_stream(io::stdin().lock(), label)
}

/// Copies `reader` to standard output. Stops quietly if Ctrl+C is pressed or if
/// standard output is closed.
fn copy_stream(mut reader: impl Read, label: &str) -> bool {
    let mut out = io::stdout().lock();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return true,
            Ok(count) => count,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                sys::take_interrupt();
                // Move past the "^C" the terminal echoed, like other tools do.
                let _ = out.write_all(b"\n");
                let _ = out.flush();
                return false;
            }
            Err(err) => {
                util::error("cat", &format!("{label}: {}", util::describe(&err)));
                return false;
            }
        };
        if out.write_all(&buffer[..count]).is_err() {
            return false;
        }
        // Flush so interactive input (cat with no file) is echoed back line by line.
        if out.flush().is_err() {
            return false;
        }
    }
}
