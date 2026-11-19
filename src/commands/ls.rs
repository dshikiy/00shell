//! `ls [-l] [-a] [-F] [FILE...]`, GNU-style: columns on a terminal, one name per
//! line otherwise; `-l` adds `total`, aligned columns and `major, minor` for devices.

use std::cmp::max;
use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::{env, io};

use crate::sys;
use crate::util::{self, ParsedArgs, STATUS_FAILURE as MINOR, STATUS_USAGE as SERIOUS};

mod long;

/// A name plus the entry's own metadata (symlinks are not followed).
struct Entry {
    name: String,
    path: PathBuf,
    meta: Metadata,
}

fn report(what: &str, path: &Path, err: &io::Error) {
    let message = format!("{what} '{}': {}", path.display(), util::describe(err));
    util::error("ls", &message);
}

fn load(name: String, path: PathBuf) -> Option<Entry> {
    let meta = fs::symlink_metadata(&path).map_err(|err| report("cannot access", &path, &err));
    meta.ok().map(|meta| Entry { name, path, meta })
}

pub fn run(args: &[String]) -> i32 {
    let opts = match util::parse_flags("ls", args, "laF") {
        Ok(opts) => opts,
        Err(_) => return SERIOUS,
    };
    let mut operands = opts.operands.clone();
    if operands.is_empty() {
        operands.push(".".to_string());
    }
    // A symlink to a directory is followed only without `-l`/`-F`, like GNU `ls`.
    let follow = !opts.has('l') && !opts.has('F');
    let (mut status, mut files, mut dirs) = (0, Vec::new(), Vec::new());
    for operand in &operands {
        match load(operand.clone(), PathBuf::from(operand)) {
            Some(e) if e.meta.is_dir() || (follow && e.path.is_dir()) => dirs.push(e),
            Some(e) => files.push(e),
            None => status = SERIOUS,
        }
    }
    sort(&mut dirs);
    if !files.is_empty() {
        util::write_stdout(&render(&mut files, &opts, false));
    }
    // With several operands each directory gets a `name:` title after a blank line.
    let mut gap = if files.is_empty() { "" } else { "\n" };
    for dir in &dirs {
        if operands.len() > 1 {
            util::write_stdout(&format!("{gap}{}:\n", dir.name));
        }
        gap = "\n";
        status = max(status, list_dir(&dir.path, &opts));
    }
    status
}

/// Prints one directory's listing and returns the exit status.
fn list_dir(dir: &Path, opts: &ParsedArgs) -> i32 {
    let reader = match fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(err) => {
            report("cannot open directory", dir, &err);
            return SERIOUS;
        }
    };
    let mut status = 0;
    let mut wanted: Vec<(String, PathBuf)> = Vec::new();
    if opts.has('a') {
        wanted.extend([".", ".."].map(|dot| (dot.to_string(), dir.join(dot))));
    }
    for item in reader {
        match item {
            Ok(i) => wanted.push((i.file_name().to_string_lossy().into_owned(), i.path())),
            Err(err) => {
                report("cannot access", dir, &err);
                status = MINOR;
            }
        }
    }
    wanted.retain(|(name, _)| opts.has('a') || !name.starts_with('.'));
    let count = wanted.len();
    let mut entries: Vec<Entry> = wanted.into_iter().filter_map(|(n, p)| load(n, p)).collect();
    util::write_stdout(&render(&mut entries, opts, true));
    if entries.len() < count {
        status = MINOR;
    }
    status
}

/// Sorts by raw bytes, or in a UTF-8 locale (anything but unset, `C`, `POSIX`,
/// `C.*`) by letters and digits only, ignoring case, as `en_US.UTF-8` does.
/// Ties put lowercase first, as glibc does, then use the raw name.
fn sort(entries: &mut [Entry]) {
    let vars = ["LC_ALL", "LC_COLLATE", "LANG"].map(|key| env::var(key).unwrap_or_default());
    let locale = vars.into_iter().find(|value| !value.is_empty());
    let locale = locale.map_or(false, |v| v != "C" && v != "POSIX" && !v.starts_with("C."));
    entries.sort_by_cached_key(|entry| sort_key(&entry.name, locale));
}

fn sort_key(name: &str, locale: bool) -> (String, Vec<bool>, String) {
    if !locale {
        return (name.to_string(), Vec::new(), String::new());
    }
    let letters = name.chars().filter(|c| c.is_alphanumeric());
    let letters = letters.flat_map(char::to_lowercase).collect();
    let cases = name.chars().map(char::is_uppercase).collect();
    (letters, cases, name.to_string())
}

fn render(entries: &mut [Entry], opts: &ParsedArgs, with_total: bool) -> String {
    sort(entries);
    if opts.has('l') {
        return long::render_long(entries, opts, with_total);
    }
    let labels: Vec<String> = entries.iter().map(|e| label(e, opts)).collect();
    match (labels.is_empty(), sys::stdout_is_tty()) {
        (true, _) => String::new(),
        (false, true) => format_columns(&labels, sys::terminal_width().unwrap_or(80)),
        (false, false) => labels.join("\n") + "\n",
    }
}

/// The entry's name, followed by its `-F` mark when `-F` is on.
fn label(entry: &Entry, opts: &ParsedArgs) -> String {
    format!("{}{}", entry.name, mark(Some(&entry.meta), opts))
}

/// Lays names out in columns (two spaces apart), filled top to bottom, using as
/// many columns as fit.
fn format_columns(labels: &[String], width: usize) -> String {
    let count = labels.len();
    let len: Vec<usize> = labels.iter().map(|l| l.chars().count()).collect();
    for columns in (1..=max(1, width / 3).min(count)).rev() {
        let rows = (count + columns - 1) / columns;
        let widths: Vec<usize> = len.chunks(rows).map(|c| *c.iter().max().unwrap()).collect();
        if columns > 1 && widths.iter().sum::<usize>() + 2 * (widths.len() - 1) > width {
            continue;
        }
        return (0..rows)
            .map(|r| {
                let cells: Vec<usize> = (r..count).step_by(rows).collect();
                let pad = |(c, &i): (usize, &usize)| match c + 1 < cells.len() {
                    true => format!("{:<w$}", labels[i], w = widths[c] + 2),
                    false => labels[i].clone() + "\n",
                };
                cells.iter().enumerate().map(pad).collect::<String>()
            })
            .collect();
    }
    labels.join("\n") + "\n"
}

/// The `-F` suffix for `meta` (empty unless `-F` is on and the type has one).
fn mark(meta: Option<&Metadata>, opts: &ParsedArgs) -> &'static str {
    meta.filter(|_| opts.has('F')).map_or("", |m| kind(m).1)
}

/// The file type letter shown by `-l` and the suffix shown by `-F`.
fn kind(meta: &Metadata) -> (char, &'static str) {
    let mode = meta.mode();
    match mode & 0o170000 {
        0o040000 => ('d', "/"),
        0o120000 => ('l', "@"),
        0o020000 => ('c', ""),
        0o060000 => ('b', ""),
        0o010000 => ('p', "|"),
        0o140000 => ('s', "="),
        0o100000 if mode & 0o111 != 0 => ('-', "*"),
        _ => ('-', ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorting_and_columns() {
        let sorted = |locale| {
            let mut names = ["b", "B", ".a", "A", "c"].map(String::from);
            names.sort_by_cached_key(|name| sort_key(name, locale));
            names.join(" ")
        };
        assert_eq!(sorted(true), ".a A b B c");
        assert_eq!(sorted(false), ".a A B b c");
        let five = ["a", "b", "c", "d", "e"].map(String::from);
        assert_eq!(format_columns(&five, 6), "a  d\nb  e\nc\n");
        assert_eq!(format_columns(&five[..2], 1), "a\nb\n");
    }
}