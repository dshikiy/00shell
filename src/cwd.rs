//! Tracking of the shell's working directory.
//!
//! Like other shells, 0-shell remembers the *logical* path (the one the user
//! typed, symlinks included) in `$PWD` and the previous one in `$OLDPWD`.

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

fn same_directory(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Removes `.` and `..` components lexically, without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// The current directory, preferring the logical path in `$PWD` when it is
/// still accurate.
pub fn logical_cwd() -> io::Result<PathBuf> {
    let physical = env::current_dir()?;
    if let Some(pwd) = env::var_os("PWD") {
        let pwd = PathBuf::from(pwd);
        if pwd.is_absolute() && same_directory(&pwd, &physical) {
            return Ok(pwd);
        }
    }
    Ok(physical)
}

/// Changes directory and updates `$PWD` / `$OLDPWD`. Returns the new logical path.
pub fn change_dir(target: &Path) -> io::Result<PathBuf> {
    let previous = logical_cwd().ok();
    let destination = match &previous {
        Some(current) => normalize(&current.join(target)),
        None => target.to_path_buf(),
    };

    env::set_current_dir(&destination)?;

    if let Some(previous) = previous {
        env::set_var("OLDPWD", previous);
    }
    env::set_var("PWD", &destination);
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dots() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
    }

    #[test]
    fn normalize_stops_at_root() {
        assert_eq!(normalize(Path::new("/../..")), PathBuf::from("/"));
    }
}
