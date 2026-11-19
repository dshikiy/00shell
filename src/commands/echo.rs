//! `echo [-n] [ARG...]`: print the arguments separated by spaces.

use crate::util;

pub fn run(args: &[String]) -> i32 {
    let (newline, words) = match args.split_first() {
        Some((first, rest)) if first == "-n" => (false, rest),
        _ => (true, args),
    };

    let mut text = words.join(" ");
    if newline {
        text.push('\n');
    }
    util::write_stdout(&text);
    0
}
