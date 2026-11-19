//! `cd [DIR]`: change the working directory.
//!
//! No argument goes to `$HOME`, `-` goes back to `$OLDPWD`, and a leading `~`
//! stands for `$HOME`.

use std::env;
use std::path::PathBuf;

use crate::{cwd, util};

pub fn run(args: &[String]) -> i32 {
    if args.len() > 1 {
        util::error("cd", "too many arguments");
        return util::STATUS_FAILURE;
    }

    let (target, announce) = match args.first().map(String::as_str) {
        None => match non_empty_var("HOME") {
            Some(home) => (PathBuf::from(home), false),
            None => {
                util::error("cd", "HOME not set");
                return util::STATUS_FAILURE;
            }
        },
        Some("-") => match non_empty_var("OLDPWD") {
            Some(old) => (PathBuf::from(old), true),
            None => {
                util::error("cd", "OLDPWD not set");
                return util::STATUS_FAILURE;
            }
        },
        Some(arg) => (expand_tilde(arg), false),
    };

    match cwd::change_dir(&target) {
        Ok(new_dir) => {
            if announce {
                util::write_stdout(&format!("{}\n", new_dir.display()));
            }
            0
        }
        Err(err) => {
            let message = format!("{}: {}", target.display(), util::describe(&err));
            util::error("cd", &message);
            util::STATUS_FAILURE
        }
    }
}

fn non_empty_var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn expand_tilde(arg: &str) -> PathBuf {
    if arg == "~" || arg.starts_with("~/") {
        if let Some(home) = non_empty_var("HOME") {
            return PathBuf::from(format!("{home}{}", &arg[1..]));
        }
    }
    PathBuf::from(arg)
}
