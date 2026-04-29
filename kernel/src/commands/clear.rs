use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;

pub fn clear_main(_argc: usize, _argv: *const *const u8) {
    // ANSI escape sequence: \x1b[2J (clear screen) \x1b[H (move cursor to home)
    let msg = "\x1b[2J\x1b[H";
    dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
    dispatch_syscall(Syscall::Exit(0));
}
