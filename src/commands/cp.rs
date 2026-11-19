//! `cp [-r] SOURCE... DEST`: copy files, and directories with `-r`.
//! `copy_path` is shared with `mv` for moves across file systems.

use std::fs::{self, File, Metadata, OpenOptions, Permissions};
use std::io::{self, ErrorKind};
use std::os::unix::fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::util;

pub fn run(args: &[String]) -> i32 {
    let parsed = match util::parse_flags("cp", args, "rR") {
        Ok(parsed) => parsed,
        Err(_) => return util::STATUS_USAGE,
    };
    let recursive = parsed.has('r') || parsed.has('R');
    let (sources, dest, dest_is_dir) = match util::split_destination("cp", &parsed.operands) {
        Some(parts) => parts,
        None => return util::STATUS_FAILURE,
    };

    // Every source is attempted even after a failure.
    let ok = sources.iter().fold(true, |ok, source| {
        let src = Path::new(source);
        let target = target_path(src, dest, dest_is_dir);
        ok & target.map_or(false, |target| copy_path("cp", src, &target, recursive))
    });
    if ok {
        0
    } else {
        util::STATUS_FAILURE
    }
}

/// Where `src` ends up: `dest` itself, or inside `dest` if that is a directory.
/// `.` and `..` are resolved so `cp -r .. dir` still has a name to use.
/// Prints an error and returns `None` if `src` has no usable name.
fn target_path(src: &Path, dest: &Path, dest_is_dir: bool) -> Option<PathBuf> {
    if !dest_is_dir {
        return Some(dest.to_path_buf());
    }
    let resolved = fs::canonicalize(src).ok();
    let name = src.file_name().or_else(|| resolved.as_ref()?.file_name());
    if name.is_none() {
        util::error(
            "cp",
            &format!("cannot determine the name of '{}'", src.display()),
        );
    }
    Some(dest.join(name?))
}

/// Copies `src` to the exact path `dst`, printing problems as `cmd: ...`.
///
/// Without `recursive`, symlinks are followed and directories are refused.
/// With it, symlinks are recreated as symlinks, as `cp -r` does.
/// Returns true if everything was copied.
pub fn copy_path(cmd: &str, src: &Path, dst: &Path, recursive: bool) -> bool {
    let meta = if recursive {
        fs::symlink_metadata(src)
    } else {
        fs::metadata(src)
    };
    let mut errors = Vec::new();
    match meta {
        Err(err) => errors.push(at("cannot stat", src)(err)),
        Ok(meta) if recursive && meta.is_dir() && copies_into_itself(src, dst) => {
            let (from, to) = (src.display(), dst.display());
            errors.push(format!(
                "cannot copy a directory, '{from}', into itself, '{to}'"
            ));
        }
        Ok(meta) => copy_node(src, dst, &meta, recursive, &mut errors),
    }
    errors.iter().for_each(|message| util::error(cmd, message));
    errors.is_empty()
}

/// True if `dst` is `src` itself or lies inside it.
pub fn copies_into_itself(src: &Path, dst: &Path) -> bool {
    // `dst` may not exist yet: resolve its parent and append the final name.
    let resolved_dst = fs::canonicalize(dst).ok().or_else(|| {
        let parent = dst.parent().filter(|p| !p.as_os_str().is_empty());
        let parent = fs::canonicalize(parent.unwrap_or_else(|| Path::new("."))).ok()?;
        Some(parent.join(dst.file_name()?))
    });
    let resolved_src = fs::canonicalize(src).ok();
    resolved_src
        .zip(resolved_dst)
        .map_or(false, |(src, dst)| dst.starts_with(src))
}

/// Builds the error text `what 'path': reason` from an `io::Error`.
fn at<'a>(what: &'a str, path: &'a Path) -> impl FnOnce(io::Error) -> String + 'a {
    move |err| format!("{what} '{}': {}", path.display(), util::describe(&err))
}

/// Copies one file, symlink or directory tree. Every failure is pushed to
/// `errors`, and the copy carries on with the remaining siblings.
fn copy_node(src: &Path, dst: &Path, meta: &Metadata, recursive: bool, errors: &mut Vec<String>) {
    let kind = meta.file_type();
    let result = if kind.is_dir() {
        if recursive {
            return copy_dir(src, dst, meta, errors);
        }
        Err(format!(
            "-r not specified; omitting directory '{}'",
            src.display()
        ))
    } else if kind.is_symlink() {
        copy_symlink(src, dst)
    } else if kind.is_file() {
        copy_file(src, dst, meta)
    } else {
        Err(format!(
            "cannot copy '{}': unsupported file type",
            src.display()
        ))
    };
    errors.extend(result.err());
}

fn copy_dir(src: &Path, dst: &Path, meta: &Metadata, errors: &mut Vec<String>) {
    let created = match fs::symlink_metadata(dst) {
        Ok(existing) if existing.is_dir() => false,
        Ok(_) => {
            let (from, to) = (src.display(), dst.display());
            return errors.push(format!(
                "cannot overwrite non-directory '{to}' with directory '{from}'"
            ));
        }
        Err(err) if err.kind() == ErrorKind::NotFound => match fs::create_dir(dst) {
            Ok(()) => true,
            Err(err) => return errors.push(at("cannot create directory", dst)(err)),
        },
        Err(err) => return errors.push(at("cannot stat", dst)(err)),
    };
    let entries = match fs::read_dir(src) {
        Ok(entries) => entries,
        Err(err) => return errors.push(at("cannot open directory", src)(err)),
    };

    for entry in entries {
        match entry.map(|entry| (entry.path(), entry.file_name())) {
            Err(err) => errors.push(at("cannot stat", src)(err)),
            Ok((path, name)) => match fs::symlink_metadata(&path) {
                Ok(child) => copy_node(&path, &dst.join(name), &child, true, errors),
                Err(err) => errors.push(at("cannot stat", &path)(err)),
            },
        }
    }

    // Permissions go on last so a read-only source directory can still be
    // filled in. An existing destination directory keeps its own.
    if created {
        let failure = fs::set_permissions(dst, Permissions::from_mode(meta.mode() & 0o7777)).err();
        errors.extend(failure.map(at("failed to set permissions for", dst)));
    }
}

fn copy_file(src: &Path, dst: &Path, meta: &Metadata) -> Result<(), String> {
    let same_file = fs::metadata(dst).map_or(false, |existing| {
        existing.dev() == meta.dev() && existing.ino() == meta.ino()
    });
    if same_file {
        return Err(format!(
            "'{}' and '{}' are the same file",
            src.display(),
            dst.display()
        ));
    }
    let mut input = File::open(src).map_err(|err| {
        format!(
            "cannot open '{}' for reading: {}",
            src.display(),
            util::describe(&err)
        )
    })?;
    // A new file gets the source's permissions (minus the umask); an existing
    // destination keeps its own, matching `cp`.
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(meta.mode() & 0o7777)
        .open(dst)
        .map_err(at("cannot create regular file", dst))?;
    io::copy(&mut input, &mut output)
        .map(drop)
        .map_err(at("error writing", dst))
}

fn copy_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    let target = fs::read_link(src).map_err(at("cannot read link", src))?;
    if fs::symlink_metadata(dst).map_or(false, |existing| !existing.is_dir()) {
        fs::remove_file(dst).map_err(at("cannot remove", dst))?;
    }
    symlink(target, dst).map_err(at("cannot create symbolic link", dst))
}