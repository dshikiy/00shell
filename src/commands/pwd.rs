//! `pwd`: print the current working directory.

use crate::{cwd, util};

pub fn run(_args: &[String]) -> i32 {
    match cwd::logical_cwd() {
        Ok(path) => {
            util::write_stdout(&format!("{}\n", path.display()));
            0
        }
        Err(err) => {
            let message = format!(
                "error retrieving current directory: {}",
                util::describe(&err)
            );
            util::error("pwd", &message);
            util::STATUS_FAILURE
        }
    }
}
