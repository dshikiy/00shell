//! `mv SOURCE... DEST`: move or rename files and directories.

use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::cp;
use crate::util;

/// `errno` for "invalid cross-device link" on Linux and macOS.
const EXDEV: i32 = 18;

pub fn run(args: &[String]) -> i32 {
    // `-f` is accepted for compatibility; 0-shell never prompts anyway.
    let parsed = match util::parse_flags("mv", args, "f") {
        Ok(parsed) => parsed,
        Err(_) => return util::STATUS_USAGE,
    };

    let (sources, dest_path, dest_is_dir) = match util::split_destination("mv", &parsed.operands) {
        Some(parts) => parts,
        None => return util::STATUS_FAILURE,
    };

    let mut ok = true;
    for source in sources {
        let src = Path::new(source);
        let target = if dest_is_dir {
            match src.file_name() {
                Some(name) => dest_path.join(name),
                None => {
                    util::error(
                        "mv",
                        &format!("cannot move '{source}': invalid source name"),
                    );
                    ok = false;
                    continue;
                }
            }
        } else {
            dest_path.to_path_buf()
        };
        ok &= move_one(src, &target);
    }

    if ok {
        0
    } else {
        util::STATUS_FAILURE
    }
}

fn move_one(src: &Path, target: &Path) -> bool {
    let src_meta = match fs::symlink_metadata(src) {
        Ok(meta) => meta,
        Err(err) => {
            let message = format!("cannot stat '{}': {}", src.display(), util::describe(&err));
            util::error("mv", &message);
            return false;
        }
    };

    if is_same_file(&src_meta, target) {
        let message = format!(
            "'{}' and '{}' are the same file",
            src.display(),
            target.display()
        );
        util::error("mv", &message);
        return false;
    }
    if src_meta.is_dir() && target_is_non_directory(target) {
        let message = format!(
            "cannot overwrite non-directory '{}' with directory '{}'",
            target.display(),
            src.display()
        );
        util::error("mv", &message);
        return false;
    }
    if src_meta.is_dir() && cp::copies_into_itself(src, target) {
        let message = format!(
            "cannot move '{}' to a subdirectory of itself, '{}'",
            src.display(),
            target.display()
        );
        util::error("mv", &message);
        return false;
    }

    match fs::rename(src, target) {
        Ok(()) => true,
        Err(err) if err.raw_os_error() == Some(EXDEV) => {
            move_across_file_systems(src, target, &src_meta)
        }
        Err(err) => {
            let message = format!(
                "cannot move '{}' to '{}': {}",
                src.display(),
                target.display(),
                util::describe(&err)
            );
            util::error("mv", &message);
            false
        }
    }
}

fn target_is_non_directory(target: &Path) -> bool {
    fs::symlink_metadata(target)
        .map(|meta| !meta.is_dir())
        .unwrap_or(false)
}

fn is_same_file(src_meta: &Metadata, target: &Path) -> bool {
    match fs::symlink_metadata(target) {
        Ok(target_meta) => {
            src_meta.dev() == target_meta.dev() && src_meta.ino() == target_meta.ino()
        }
        Err(_) => false,
    }
}

/// `rename` cannot cross file systems, so copy everything and remove the original.
fn move_across_file_systems(src: &Path, target: &Path, src_meta: &Metadata) -> bool {
    if !cp::copy_path("mv", src, target, true) {
        return false;
    }
    let removed = if src_meta.is_dir() {
        fs::remove_dir_all(src)
    } else {
        fs::remove_file(src)
    };
    match removed {
        Ok(()) => true,
        Err(err) => {
            let message = format!(
                "cannot remove '{}': {}",
                src.display(),
                util::describe(&err)
            );
            util::error("mv", &message);
            false
        }
    }
}