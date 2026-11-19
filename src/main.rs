//! 0-shell: a minimalist Unix shell whose commands are implemented from scratch.

mod commands;
mod cwd;
mod parser;
mod shell;
mod sys;
mod util;

fn main() {
    sys::install_sigint_handler();
    std::process::exit(shell::run());
}
