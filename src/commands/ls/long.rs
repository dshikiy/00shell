//! The `ls -l` format: permission string, owner, size or device numbers, and time.

use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::time::UNIX_EPOCH;

use super::{kind, label, mark, Entry};
use crate::commands::identity::Names;
use crate::sys;
use crate::util::ParsedArgs;

/// Files older than this (or in the future) show their year instead of the time.
const SIX_MONTHS_SECS: i64 = 15_778_476;
const MONTHS: &str = "JanFebMarAprMayJunJulAugSepOctNovDec";

/// `ls -l`: every row is seven cells (mode, links, owner, group, size, time,
/// name), each padded to the widest cell of its column.
pub(super) fn render_long(entries: &[Entry], opts: &ParsedArgs, with_total: bool) -> String {
    let names = Names::load();
    let now = UNIX_EPOCH.elapsed().map_or(0, |d| d.as_secs() as i64);
    let devs: Vec<_> = entries.iter().map(|e| device_numbers(&e.meta)).collect();
    let dev_width = |pick: fn(&(u64, u64)) -> u64| {
        devs.iter()
            .flatten()
            .map(|d| pick(d).to_string().len())
            .max()
            .unwrap_or(0)
    };
    let (major_w, minor_w) = (dev_width(|d| d.0), dev_width(|d| d.1));

    let rows: Vec<[String; 7]> = entries
        .iter()
        .zip(&devs)
        .map(|(entry, dev)| {
            let meta = &entry.meta;
            let size = match dev {
                Some((major, minor)) => format!("{major:>major_w$}, {minor:>minor_w$}"),
                None => meta.len().to_string(),
            };
            [
                mode_string(kind(meta).0, meta.mode()),
                meta.nlink().to_string(),
                names.user(meta.uid()),
                names.group(meta.gid()),
                size,
                format_time(meta.mtime(), now),
                long_name(entry, opts),
            ]
        })
        .collect();
    let width = |cell: usize| {
        rows.iter()
            .map(|r| r[cell].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (links_w, owner_w, group_w, size_w) = (width(1), width(2), width(3), width(4));

    // `st_blocks` counts 512-byte units; `ls` prints 1024-byte blocks.
    let blocks: u64 = entries.iter().map(|e| e.meta.blocks()).sum();
    let mut out = String::new();
    if with_total {
        out += &format!("total {}\n", (blocks + 1) / 2);
    }
    for [mode, links, owner, group, size, time, name] in &rows {
        let line = format!("{mode} {links:>links_w$} {owner:<owner_w$} {group:<group_w$} ");
        out += &format!("{line}{size:>size_w$} {time} {name}\n");
    }
    out
}

/// The name cell of `ls -l`. A symlink shows its target; with `-F` the mark
/// goes on the target (a dangling link gets none), as in GNU `ls -lF`.
fn long_name(entry: &Entry, opts: &ParsedArgs) -> String {
    match fs::read_link(&entry.path) {
        Err(_) => label(entry, opts),
        Ok(target) => {
            let mark = mark(fs::metadata(&entry.path).ok().as_ref(), opts);
            format!("{} -> {}{mark}", entry.name, target.to_string_lossy())
        }
    }
}

/// Renders the ten-character permission string, e.g. `drwxr-xr-x`.
fn mode_string(kind: char, mode: u32) -> String {
    let mut text = String::from(kind);
    for (i, symbol) in "rwxrwxrwx".chars().enumerate() {
        let on = mode & (0o400 >> i) != 0;
        // The execute slots double as setuid (`s`), setgid (`s`) and sticky (`t`).
        let special = i % 3 == 2 && mode & (0o4000 >> (i / 3)) != 0;
        let letter = if i == 8 { 't' } else { 's' };
        text.push(match (special, on) {
            (false, _) => {
                if on {
                    symbol
                } else {
                    '-'
                }
            }
            (true, true) => letter,
            (true, false) => letter.to_ascii_uppercase(),
        });
    }
    text
}

/// `(major, minor)` for device files; each kernel packs the number differently.
fn device_numbers(meta: &Metadata) -> Option<(u64, u64)> {
    let rdev = meta.rdev();
    let numbers = if cfg!(target_os = "macos") {
        ((rdev >> 24) & 0xff, rdev & 0x00ff_ffff)
    } else {
        (
            ((rdev >> 8) & 0xfff) | ((rdev >> 32) & !0xfff),
            (rdev & 0xff) | ((rdev >> 12) & !0xff),
        )
    };
    matches!(kind(meta).0, 'c' | 'b').then_some(numbers)
}

/// `Mon DD HH:MM` for recent files, `Mon DD  YYYY` for old or future ones.
fn format_time(mtime: i64, now: i64) -> String {
    let t = match sys::local_time(mtime) {
        Some(t) => t,
        None => return "?".to_string(),
    };
    let month = MONTHS.get(3 * t.month..3 * t.month + 3).unwrap_or("???");
    if mtime <= now && now - mtime < SIX_MONTHS_SECS {
        format!("{month} {:>2} {:02}:{:02}", t.day, t.hour, t.minute)
    } else {
        format!("{month} {:>2}  {}", t.day, t.year)
    }
}

#[cfg(test)]
mod tests {
    use super::mode_string;

    #[test]
    fn permission_strings() {
        assert_eq!(mode_string('d', 0o755), "drwxr-xr-x");
        assert_eq!(mode_string('-', 0o4644), "-rwSr--r--");
        assert_eq!(mode_string('-', 0o2755), "-rwxr-sr-x");
        assert_eq!(mode_string('d', 0o1777), "drwxrwxrwt");
    }
}