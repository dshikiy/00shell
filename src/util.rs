//! Small helpers shared by the shell and the built-in commands.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Exit status for a command that failed.
pub const STATUS_FAILURE: i32 = 1;
/// Exit status for a command that was invoked incorrectly.
pub const STATUS_USAGE: i32 = 2;

/// Turns an I/O error into the bare message a Unix tool would print
/// (e.g. `No such file or directory`, without Rust's `(os error 2)` suffix).
pub fn describe(err: &io::Error) -> String {
    let text = err.to_string();
    match text.find(" (os error") {
        Some(index) => text[..index].to_string(),
        None => text,
    }
}

/// Prints `cmd: message` on standard error.
pub fn error(cmd: &str, message: &str) {
    eprintln!("{cmd}: {message}");
}

/// Explains why `dest` cannot take several sources (`cp`/`mv` with many operands):
/// it either does not exist or is not a directory.
fn report_bad_target(cmd: &str, dest: &Path) {
    let reason = match fs::metadata(dest) {
        Err(err) => describe(&err),
        Ok(_) => "Not a directory".to_string(),
    };
    error(cmd, &format!("target '{}': {reason}", dest.display()));
}

/// Splits the operands of `cp` / `mv` into `(sources, destination, destination
/// is a directory)`. Prints the GNU-style complaint and returns `None` when
/// there is no destination, or several sources and a destination that is not a
/// directory.
pub fn split_destination<'a>(
    cmd: &str,
    operands: &'a [String],
) -> Option<(&'a [String], &'a Path, bool)> {
    let (dest, sources) = match operands.split_last() {
        Some((dest, sources)) if !sources.is_empty() => (Path::new(dest), sources),
        Some((only, _)) => {
            error(
                cmd,
                &format!("missing destination file operand after '{only}'"),
            );
            return None;
        }
        None => {
            error(cmd, "missing file operand");
            return None;
        }
    };
    let dest_is_dir = fs::metadata(dest).map_or(false, |meta| meta.is_dir());
    if sources.len() > 1 && !dest_is_dir {
        report_bad_target(cmd, dest);
        return None;
    }
    Some((sources, dest, dest_is_dir))
}

/// Writes text to standard output and flushes it.
///
/// Write failures (for example a closed stdout) are ignored on purpose: a shell
/// must keep running even when the output side goes away.
pub fn write_stdout(text: &str) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// Returned when a command received an option it does not support.
/// The message has already been printed when this value is produced.
#[derive(Debug)]
pub struct UsageError;

/// Options and operands split apart.
pub struct ParsedArgs {
    flags: Vec<char>,
    pub operands: Vec<String>,
}

impl ParsedArgs {
    pub fn has(&self, flag: char) -> bool {
        self.flags.contains(&flag)
    }
}

/// Splits `args` into single-letter flags and operands.
///
/// Flags may be combined (`-la`), `--` ends option parsing and a lone `-` is an
/// operand. Any flag not listed in `allowed` is reported and rejected.
pub fn parse_flags(cmd: &str, args: &[String], allowed: &str) -> Result<ParsedArgs, UsageError> {
    let mut flags = Vec::new();
    let mut operands = Vec::new();
    let mut options_ended = false;

    for arg in args {
        if options_ended || arg == "-" || !arg.starts_with('-') {
            operands.push(arg.clone());
        } else if arg == "--" {
            options_ended = true;
        } else if arg.starts_with("--") {
            error(cmd, &format!("unrecognized option '{arg}'"));
            return Err(UsageError);
        } else {
            for flag in arg.chars().skip(1) {
                if !allowed.contains(flag) {
                    error(cmd, &format!("invalid option -- '{flag}'"));
                    return Err(UsageError);
                }
                flags.push(flag);
            }
        }
    }
    Ok(ParsedArgs { flags, operands })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn combined_flags_are_split() {
        let parsed = parse_flags("ls", &strings(&["-la", "dir"]), "laF").unwrap();
        assert!(parsed.has('l') && parsed.has('a') && !parsed.has('F'));
        assert_eq!(parsed.operands, strings(&["dir"]));
    }

    #[test]
    fn double_dash_ends_options() {
        let parsed = parse_flags("rm", &strings(&["--", "-r"]), "r").unwrap();
        assert!(!parsed.has('r'));
        assert_eq!(parsed.operands, strings(&["-r"]));
    }

    #[test]
    fn unknown_flag_is_rejected() {
        assert!(parse_flags("ls", &strings(&["-z"]), "laF").is_err());
    }

    #[test]
    fn describe_strips_os_error_suffix() {
        let err = io::Error::from_raw_os_error(2);
        assert_eq!(describe(&err), "No such file or directory");
    }
}