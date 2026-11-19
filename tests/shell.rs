//! End-to-end tests: each one starts the real `0-shell` binary, feeds it a
//! script on standard input and checks what it printed and what it did on disk.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PROMPT: &str = "$ ";

/// A fresh, empty, canonical directory unique to this test.
fn sandbox(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("0shell-it-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::canonicalize(dir).unwrap()
}

fn run_shell(dir: &Path, script: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_0-shell"))
        .current_dir(dir)
        .env("HOME", dir)
        .env("LC_ALL", "C")
        .env_remove("PWD")
        .env_remove("OLDPWD")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start 0-shell");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// Standard output with the prompts removed.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace(PROMPT, "")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
}

#[test]
fn prompt_is_shown_and_eof_exits_cleanly() {
    let dir = sandbox("eof");
    let output = run_shell(&dir, "");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), PROMPT);
}

#[test]
fn echo_prints_arguments_and_keeps_quoted_spacing() {
    let dir = sandbox("echo");
    let output = run_shell(
        &dir,
        "echo Hello   There\necho \"Hello   There\"\necho\necho -n hi\n",
    );
    assert_eq!(stdout(&output), "Hello There\nHello   There\n\nhi");
}

#[test]
fn unknown_command_is_reported_and_shell_continues() {
    let dir = sandbox("unknown");
    let output = run_shell(&dir, "something\necho after\n");
    assert_eq!(stderr(&output), "Command 'something' not found\n");
    assert_eq!(stdout(&output), "after\n");
}

#[test]
fn exit_uses_given_status() {
    let dir = sandbox("exit");
    assert_eq!(run_shell(&dir, "exit 3\n").status.code(), Some(3));
    assert_eq!(run_shell(&dir, "exit\n").status.code(), Some(0));
    assert_eq!(run_shell(&dir, "nope\nexit\n").status.code(), Some(127));
}

#[test]
fn text_after_exit_is_not_executed() {
    let dir = sandbox("exit-stops");
    let output = run_shell(&dir, "exit\necho unreachable\n");
    assert_eq!(stdout(&output), "");
}

#[test]
fn cd_pwd_and_home() {
    let dir = sandbox("cd");
    fs::create_dir_all(dir.join("a/b")).unwrap();
    let output = run_shell(&dir, "cd a/b\npwd\ncd ..\npwd\ncd\npwd\ncd -\npwd\n");
    let expected = format!("{d}/a/b\n{d}/a\n{d}\n{d}/a\n{d}/a\n", d = dir.display());
    assert_eq!(stdout(&output), expected);
}

#[test]
fn cd_errors_do_not_move_the_shell() {
    let dir = sandbox("cd-errors");
    write(&dir, "file", "x");
    let output = run_shell(&dir, "cd missing\ncd file\ncd a b\npwd\n");
    let err = stderr(&output);
    assert!(
        err.contains("cd: missing: No such file or directory"),
        "{err}"
    );
    assert!(err.contains("cd: file: Not a directory"), "{err}");
    assert!(err.contains("cd: too many arguments"), "{err}");
    assert_eq!(stdout(&output), format!("{}\n", dir.display()));
}

#[test]
fn cd_follows_symlinks_logically() {
    let dir = sandbox("cd-link");
    fs::create_dir(dir.join("real")).unwrap();
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    let output = run_shell(&dir, "cd link\npwd\ncd ..\npwd\n");
    assert_eq!(
        stdout(&output),
        format!("{d}/link\n{d}\n", d = dir.display())
    );
}

#[test]
fn cat_prints_files_in_order_and_reports_problems() {
    let dir = sandbox("cat");
    write(&dir, "one", "first\n");
    write(&dir, "two", "second\n");
    fs::create_dir(dir.join("folder")).unwrap();
    let output = run_shell(&dir, "cat one two\ncat missing one\ncat folder\n");
    assert_eq!(stdout(&output), "first\nsecond\nfirst\n");
    let err = stderr(&output);
    assert!(
        err.contains("cat: missing: No such file or directory"),
        "{err}"
    );
    assert!(err.contains("cat: folder: Is a directory"), "{err}");
}

#[test]
fn mkdir_cp_mv_rm_round_trip() {
    let dir = sandbox("files");
    write(&dir, "one.txt", "hello\n");
    let script = "mkdir -p proj/src\n\
                  cp one.txt proj/copy.txt\n\
                  mv one.txt proj/src\n\
                  cat proj/copy.txt proj/src/one.txt\n\
                  rm proj/copy.txt\n\
                  ls proj\n\
                  rm -r proj\n\
                  ls\n";
    let output = run_shell(&dir, script);
    assert_eq!(stdout(&output), "hello\nhello\nsrc\n");
    assert_eq!(stderr(&output), "");
    assert!(!dir.join("proj").exists());
}

#[test]
fn mv_renames_and_moves_several_files_into_a_directory() {
    let dir = sandbox("mv");
    write(&dir, "a", "A");
    write(&dir, "b", "B");
    fs::create_dir(dir.join("box")).unwrap();
    let output = run_shell(&dir, "mv a renamed\nmv renamed b box\n");
    assert_eq!(stderr(&output), "");
    assert_eq!(fs::read_to_string(dir.join("box/renamed")).unwrap(), "A");
    assert_eq!(fs::read_to_string(dir.join("box/b")).unwrap(), "B");
    assert!(!dir.join("a").exists());
}

#[test]
fn cp_needs_r_for_directories_and_then_copies_everything() {
    let dir = sandbox("cp-dir");
    fs::create_dir_all(dir.join("tree/deep")).unwrap();
    write(&dir, "tree/deep/leaf", "leaf");
    let output = run_shell(&dir, "cp tree copy\ncp -r tree copy\n");
    assert!(
        stderr(&output).contains("cp: -r not specified; omitting directory 'tree'"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(dir.join("copy/deep/leaf")).unwrap(),
        "leaf"
    );
}

#[test]
fn cp_preserves_permissions() {
    let dir = sandbox("cp-mode");
    write(&dir, "tool", "#!/bin/true\n");
    fs::set_permissions(dir.join("tool"), fs::Permissions::from_mode(0o750)).unwrap();
    run_shell(&dir, "cp tool tool2\n");
    let mode = fs::metadata(dir.join("tool2"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o750 & !current_umask());
}

fn current_umask() -> u32 {
    // Reads the umask the test process inherited by creating a file with mode 0o777.
    let probe = std::env::temp_dir().join(format!("0shell-umask-{}", std::process::id()));
    let _ = fs::remove_file(&probe);
    fs::File::create(&probe).unwrap();
    let mode = fs::metadata(&probe).unwrap().permissions().mode() & 0o777;
    fs::remove_file(&probe).unwrap();
    // A new file starts from 0o666, so the missing bits are the umask's.
    0o666 & !mode
}

#[test]
fn rm_refuses_directories_without_r_and_dot_entries_with_r() {
    let dir = sandbox("rm");
    fs::create_dir(dir.join("folder")).unwrap();
    let output = run_shell(&dir, "rm folder\nrm -r .\nrm missing\nrm -f missing\n");
    let err = stderr(&output);
    assert!(
        err.contains("rm: cannot remove 'folder': Is a directory"),
        "{err}"
    );
    assert!(
        err.contains("refusing to remove '.' or '..' directory"),
        "{err}"
    );
    assert!(
        err.contains("rm: cannot remove 'missing': No such file or directory"),
        "{err}"
    );
    assert!(dir.join("folder").is_dir());
}

#[test]
fn mkdir_reports_existing_directories() {
    let dir = sandbox("mkdir");
    let output = run_shell(&dir, "mkdir one\nmkdir one\nmkdir -p one\nmkdir -p x/y/z\n");
    assert_eq!(
        stderr(&output),
        "mkdir: cannot create directory 'one': File exists\n"
    );
    assert!(dir.join("x/y/z").is_dir());
}

#[test]
fn ls_lists_sorted_names_and_hides_dotfiles_unless_asked() {
    let dir = sandbox("ls-basic");
    write(&dir, "b", "");
    write(&dir, "a", "");
    write(&dir, ".hidden", "");
    let output = run_shell(&dir, "ls\nls -a\n");
    assert_eq!(stdout(&output), "a\nb\n.\n..\n.hidden\na\nb\n");
}

#[test]
fn ls_classifies_with_f() {
    let dir = sandbox("ls-f");
    fs::create_dir(dir.join("folder")).unwrap();
    write(&dir, "plain", "");
    write(&dir, "program", "");
    fs::set_permissions(dir.join("program"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("plain", dir.join("shortcut")).unwrap();
    let output = run_shell(&dir, "ls -F\n");
    assert_eq!(stdout(&output), "folder/\nplain\nprogram*\nshortcut@\n");
}

#[test]
fn ls_long_shows_total_permissions_size_and_link_target() {
    let dir = sandbox("ls-l");
    write(&dir, "data", "12345");
    fs::set_permissions(dir.join("data"), fs::Permissions::from_mode(0o640)).unwrap();
    std::os::unix::fs::symlink("data", dir.join("ptr")).unwrap();
    let output = run_shell(&dir, "ls -l\n");
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("total "), "{text}");
    assert!(lines[1].starts_with("-rw-r----- 1 "), "{text}");
    assert!(
        lines[1].contains(" 5 ") && lines[1].ends_with(" data"),
        "{text}"
    );
    assert!(lines[2].starts_with("lrwxrwxrwx 1 "), "{text}");
    assert!(lines[2].ends_with(" ptr -> data"), "{text}");
}

#[test]
fn ls_combined_flags_and_missing_paths() {
    let dir = sandbox("ls-combined");
    fs::create_dir(dir.join("sub")).unwrap();
    write(&dir, "sub/inner", "");
    let output = run_shell(&dir, "ls -laF sub\nls nothing\nls -z\n");
    let text = stdout(&output);
    assert!(text.contains(" ./\n"), "{text}");
    assert!(text.contains(" ../\n"), "{text}");
    assert!(text.contains(" inner\n"), "{text}");
    let err = stderr(&output);
    assert!(
        err.contains("ls: cannot access 'nothing': No such file or directory"),
        "{err}"
    );
    assert!(err.contains("ls: invalid option -- 'z'"), "{err}");
}

#[test]
fn ls_with_several_operands_prints_headers() {
    let dir = sandbox("ls-headers");
    fs::create_dir_all(dir.join("d1")).unwrap();
    fs::create_dir_all(dir.join("d2")).unwrap();
    write(&dir, "d1/x", "");
    write(&dir, "d2/y", "");
    write(&dir, "file", "");
    let output = run_shell(&dir, "ls file d1 d2\n");
    assert_eq!(stdout(&output), "file\n\nd1:\nx\n\nd2:\ny\n");
}

#[test]
fn quoted_text_can_span_lines() {
    let dir = sandbox("multiline");
    let output = run_shell(&dir, "echo \"one\ntwo\"\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "$ > one\ntwo\n$ ");
}

#[test]
fn failing_commands_never_stop_the_shell() {
    let dir = sandbox("resilient");
    let script =
        "cat missing\ncp\nmv\nrm\nmkdir\ncd /nope\nls /nope\ncp a b c\nmv a b c\necho survived\n";
    let output = run_shell(&dir, script);
    assert_eq!(stdout(&output), "survived\n");
    assert!(output.status.success());
}

#[test]
fn unterminated_quote_at_eof_is_an_error_not_a_hang() {
    let dir = sandbox("eof-quote");
    let output = run_shell(&dir, "echo \"never closed\n");
    assert!(
        stderr(&output).contains("unexpected EOF"),
        "{}",
        stderr(&output)
    );
}
