//! The built-in commands. Each submodule exposes a `run` function that takes the
//! command's arguments and returns an exit status.

mod cat;
mod cd;
mod cp;
mod echo;
mod exit;
mod identity;
mod ls;
mod mkdir;
mod mv;
mod pwd;
mod rm;

/// What the shell should do after a command finished.
pub enum Flow {
    /// Keep going; carries the command's exit status.
    Continue(i32),
    /// Leave the shell with this exit status.
    Exit(i32),
}

/// Runs the built-in called `name`, or returns `None` if there is no such command.
pub fn execute(name: &str, args: &[String], last_status: i32) -> Option<Flow> {
    let status = match name {
        "echo" => echo::run(args),
        "cd" => cd::run(args),
        "ls" => ls::run(args),
        "pwd" => pwd::run(args),
        "cat" => cat::run(args),
        "cp" => cp::run(args),
        "rm" => rm::run(args),
        "mv" => mv::run(args),
        "mkdir" => mkdir::run(args),
        "exit" => return Some(exit::run(args, last_status)),
        _ => return None,
    };
    Some(Flow::Continue(status))
}
