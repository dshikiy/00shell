//! Thin wrappers around the few libc calls the standard library does not expose.
//!
//! This is the only module that contains `unsafe` code. libc is already linked
//! into every Rust program, so no external crate is needed.

use std::env;
use std::io;
use std::mem;
use std::os::raw::{c_char, c_int, c_long, c_ulong, c_ushort};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};

const SIGINT: c_int = 2;
/// `TIOCGWINSZ` is 0x5413 on Linux and 0x40087468 on macOS.
const TIOCGWINSZ: c_ulong = if cfg!(target_os = "macos") {
    0x4008_7468
} else {
    0x5413
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Mirror of the C `struct tm` (glibc / musl / BSD field order).
#[repr(C)]
struct Tm {
    tm_sec: c_int,
    tm_min: c_int,
    tm_hour: c_int,
    tm_mday: c_int,
    tm_mon: c_int,
    tm_year: c_int,
    tm_wday: c_int,
    tm_yday: c_int,
    tm_isdst: c_int,
    tm_gmtoff: c_long,
    tm_zone: *const c_char,
}

/// Mirror of the C `struct winsize`.
#[repr(C)]
#[derive(Default)]
struct WinSize {
    rows: c_ushort,
    cols: c_ushort,
    x_pixels: c_ushort,
    y_pixels: c_ushort,
}

extern "C" {
    fn signal(signum: c_int, handler: usize) -> usize;
    fn siginterrupt(sig: c_int, flag: c_int) -> c_int;
    fn isatty(fd: c_int) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn localtime_r(timep: *const c_long, result: *mut Tm) -> *mut Tm;
}

extern "C" fn handle_sigint(_signum: c_int) {
    // Only async-signal-safe work is allowed here: set a flag and return.
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Makes Ctrl+C interrupt a blocking read instead of killing the shell.
pub fn install_sigint_handler() {
    // SAFETY: `handle_sigint` only touches an atomic, which is signal-safe.
    // `siginterrupt` makes `read` fail with EINTR so the prompt can be redrawn.
    unsafe {
        signal(SIGINT, handle_sigint as *const () as usize);
        siginterrupt(SIGINT, 1);
    }
}

/// Returns true (once) if Ctrl+C was pressed since the last call.
pub fn take_interrupt() -> bool {
    INTERRUPTED.swap(false, Ordering::SeqCst)
}

fn is_tty(stream: &impl AsRawFd) -> bool {
    // SAFETY: `isatty` only inspects a file descriptor number.
    unsafe { isatty(stream.as_raw_fd()) == 1 }
}

pub fn stdin_is_tty() -> bool {
    is_tty(&io::stdin())
}

pub fn stdout_is_tty() -> bool {
    is_tty(&io::stdout())
}

/// Width of the terminal in columns, honouring `$COLUMNS` like GNU `ls`.
pub fn terminal_width() -> Option<usize> {
    let from_env = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&columns| columns > 0);

    from_env.or_else(|| {
        let mut size = WinSize::default();
        // SAFETY: `size` is a valid, writable `winsize` for the duration of the call.
        let status = unsafe {
            ioctl(
                io::stdout().as_raw_fd(),
                TIOCGWINSZ,
                &mut size as *mut WinSize,
            )
        };
        (status == 0 && size.cols > 0).then(|| usize::from(size.cols))
    })
}

/// A broken-down local time.
pub struct LocalTime {
    pub year: i32,
    /// Zero-based month (0 = January).
    pub month: usize,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
}

/// Converts seconds since the Unix epoch to the local time zone.
pub fn local_time(secs: i64) -> Option<LocalTime> {
    let timestamp = c_long::try_from(secs).ok()?;
    // SAFETY: `Tm` is plain old data for which all-zero bytes (including the
    // null `tm_zone` pointer) are a valid value.
    let mut tm: Tm = unsafe { mem::zeroed() };
    // SAFETY: both pointers are valid; `localtime_r` is the re-entrant variant
    // and writes only into `tm`.
    if unsafe { localtime_r(&timestamp, &mut tm) }.is_null() {
        return None;
    }
    Some(LocalTime {
        year: tm.tm_year + 1900,
        month: usize::try_from(tm.tm_mon).ok()?,
        day: tm.tm_mday,
        hour: tm.tm_hour,
        minute: tm.tm_min,
    })
}