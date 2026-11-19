//! `rm [-r] [-f] FILE...`: remove files, and directories with `-r`.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::util;

pub fn run(args: &[String]) -> i32 {
    let parsed = match util::parse_flags("rm", args, "rRf") {
        Ok(parsed) => parsed,
        Err(_) => return util::STATUS_USAGE,
    };
    let recursive = parsed.has('r') || parsed.has('R');
    let force = parsed.has('f');

    if parsed.operands.is_empty() {
        if force {
            return 0;
        }
        util::error("rm", "missing operand");
        return util::STATUS_FAILURE;
    }

    let mut ok = true;
    for operand in &parsed.operands {
        ok &= remove(operand, recursive, force);
    }
    if ok {
        0
    } else {
        util::STATUS_FAILURE
    }
}

fn remove(operand: &str, recursive: bool, force: bool) -> bool {
    let path = Path::new(operand);

    // Without -r a directory (dots included) is just "Is a directory" below.
    if recursive && is_dot_entry(operand) {
        let message = format!("refusing to remove '.' or '..' directory: skipping '{operand}'");
        util::error("rm", &message);
        return false;
    }
    if recursive && is_root(path) {
        util::error("rm", "it is dangerous to operate recursively on '/'");
        return false;
    }

    // `symlink_metadata` so a symlink to a directory is removed as a link,
    // never followed.
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if force && err.kind() == ErrorKind::NotFound => return true,
        Err(err) => return report(operand, &err),
    };

    let result = if meta.is_dir() {
        if !recursive {
            util::error("rm", &format!("cannot remove '{operand}': Is a directory"));
            return false;
        }
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };

    match result {
        Ok(()) => true,
        Err(err) => report(operand, &err),
    }
}

fn report(operand: &str, err: &std::io::Error) -> bool {
    util::error(
        "rm",
        &format!("cannot remove '{operand}': {}", util::describe(err)),
    );
    false
}

/// True for `.`, `..` and paths ending in `/.` or `/..`.
///
/// Done on the text because `Path::components` silently drops a trailing `.`.
fn is_dot_entry(operand: &str) -> bool {
    let trimmed = operand.trim_end_matches('/');
    trimmed == "." || trimmed == ".." || trimmed.ends_with("/.") || trimmed.ends_with("/..")
}

fn is_root(path: &Path) -> bool {
    fs::canonicalize(path)
        .map(|resolved| resolved.parent().is_none())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_entries_are_detected() {
        for dotted in [".", "..", "a/..", "a/.", "../.."] {
            assert!(is_dot_entry(dotted), "{dotted}");
        }
        for plain in ["a", "a/b", ".hidden", "...", "a.."] {
            assert!(!is_dot_entry(plain), "{plain}");
        }
    }
}
