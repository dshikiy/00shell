# 0-shell

A minimalist Unix shell written in Rust using only the standard library (plus
five libc calls it declares itself). Every command is implemented from scratch;
the shell never spawns an external program.

## Build and run

```sh
cargo build --release
./target/release/0-shell
```

```text
$ cd /tmp
$ pwd
/tmp
$ echo "Hello   There"
Hello   There
$ something
Command 'something' not found
$ exit
```

`Ctrl+D` on an empty prompt exits, `Ctrl+C` abandons the current line and
returns to a fresh prompt.

## Commands

| Command | Behaviour |
|---------|-----------|
| `echo [-n] [ARG...]` | Prints arguments separated by spaces; quotes are honoured by the parser. |
| `cd [DIR]` | No argument → `$HOME`; `cd -` → `$OLDPWD`; leading `~` → `$HOME`. Tracks the logical path through symlinks. |
| `pwd` | Prints the working directory. |
| `ls [-l] [-a] [-F] [FILE...]` | Flags can be combined (`-laF`). Output matches GNU `ls`: columns on a terminal, one per line otherwise; `-l` shows `total`, aligned columns, symlink targets and `major, minor` for devices. |
| `cat [FILE...]` | Prints files; reads stdin when given no file or `-`. |
| `cp [-r] SRC... DEST` | Copies files (keeping permissions) and, with `-r`, directories and symlinks. Refuses to copy a directory into itself. |
| `mv SRC... DEST` | Renames or moves; falls back to copy + delete across file systems. |
| `rm [-r] [-f] FILE...` | Removes files; `-r` for directories. Refuses `.`, `..` and `/` with `-r`. |
| `mkdir [-p] DIR...` | Creates directories. |
| `exit [N]` | Leaves the shell (default status: that of the last command). |

Error messages and exit statuses follow GNU coreutils (`cp: cannot stat 'x': No
such file or directory`, …) and go to standard error. This includes
`Command '<name>' not found`; to send it to standard output instead, change
`report_not_found` in `src/shell.rs`.

## Parsing

Whitespace separates words. `"..."` groups words (with `\"`, `\\`, `\$` and
``\` `` escapes), `'...'` is literal, and a backslash escapes the next character
outside quotes. If a quote is still open at the end of the line the shell asks
for more input with a `> ` prompt.

## Layout

```text
src/
  main.rs        entry point
  shell.rs       prompt / read / execute loop, Ctrl+C and Ctrl+D handling
  parser.rs      quote-aware tokenizer
  cwd.rs         logical working directory ($PWD / $OLDPWD)
  sys.rs         the only `unsafe` code: SIGINT, isatty, terminal width, localtime
  util.rs        error formatting and option parsing shared by commands
  commands/      one file per command (+ identity.rs: uid/gid → names for ls -l)
tests/shell.rs   end-to-end tests that drive the real binary
```

## Tests

```sh
cargo test
cargo clippy --all-targets
```

Unit tests cover the parser, option handling and `ls` formatting; the
integration tests start the binary, feed it a script and check stdout, stderr,
exit status and the resulting files.

## Compatibility notes

* No crates are used, so it builds offline.
* `Cargo.toml` declares `rust-version = "1.63"` and the code avoids newer std
  APIs (no `let … else`, no `IsTerminal`); it was compiled and tested with
  Rust 1.97 only.
* `ls` sorts by byte order in the `C`/`POSIX` locale and approximates
  `en_US.UTF-8` collation (case- and punctuation-insensitive) otherwise, the
  same switch GNU `ls` makes from `LC_ALL` / `LC_COLLATE` / `LANG`.
* Not implemented (not required): pipes, redirection, globbing, `;` chaining,
  variable expansion, history, completion.
