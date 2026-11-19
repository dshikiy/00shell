//! `mkdir [-p] DIR...`: create directories.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::util;

/// `errno` values, identical on Linux and macOS.
const EEXIST: i32 = 17;
const ENOTDIR: i32 = 20;

pub fn run(args: &[String]) -> i32 {
    let parsed = match util::parse_flags("mkdir", args, "p") {
        Ok(parsed) => parsed,
        Err(_) => return util::STATUS_USAGE,
    };
    if parsed.operands.is_empty() {
        util::error("mkdir", "missing operand");
        return util::STATUS_FAILURE;
    }

    let parents = parsed.has('p');
    let mut ok = true;
    for operand in &parsed.operands {
        let result = if parents {
            create_with_parents(Path::new(operand))
        } else {
            fs::create_dir(operand).map_err(|err| (PathBuf::from(operand), err))
        };
        if let Err((failed_path, err)) = result {
            let message = format!(
                "cannot create directory '{}': {}",
                failed_path.display(),
                util::describe(&err)
            );
            util::error("mkdir", &message);
            ok = false;
        }
    }

    if ok {
        0
    } else {
        util::STATUS_FAILURE
    }
}

/// `mkdir -p`: creates each missing component in turn. On failure, returns the
/// component that could not be created so the message can name it.
fn create_with_parents(path: &Path) -> Result<(), (PathBuf, io::Error)> {
    let mut prefix = PathBuf::new();
    let component_count = path.components().count();

    for (index, component) in path.components().enumerate() {
        prefix.push(component.as_os_str());
        let is_last = index + 1 == component_count;

        match fs::metadata(&prefix) {
            Ok(meta) if meta.is_dir() => continue,
            Ok(_) => {
                // Something that is not a directory is in the way.
                let errno = if is_last { EEXIST } else { ENOTDIR };
                return Err((prefix, io::Error::from_raw_os_error(errno)));
            }
            Err(_) => {}
        }
        if let Err(err) = fs::create_dir(&prefix) {
            return Err((prefix, err));
        }
    }
    Ok(())
}
