//! `exit [N]`: leave the shell, with status `N` (default: the last command's status).

use super::Flow;
use crate::util;

pub fn run(args: &[String], last_status: i32) -> Flow {
    match args {
        [] => Flow::Exit(last_status),
        [code] => match code.parse::<i64>() {
            // Exit statuses are a single byte; truncate like the OS does.
            Ok(number) => Flow::Exit((number & 0xff) as i32),
            Err(_) => {
                util::error("exit", &format!("{code}: numeric argument required"));
                Flow::Exit(util::STATUS_USAGE)
            }
        },
        _ => {
            util::error("exit", "too many arguments");
            Flow::Continue(util::STATUS_FAILURE)
        }
    }
}
