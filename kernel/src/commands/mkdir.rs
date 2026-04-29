use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

pub fn mkdir_main(argc: usize, argv: *const *const u8) {
    if argc < 2 {
        let msg = "Usage: mkdir <path>\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
        return;
    }

    let args = unsafe { slice::from_raw_parts(argv, argc) };
    let path_ptr = args[1];
    let mut path_len = 0;
    unsafe {
        while *path_ptr.add(path_len) != 0 {
            path_len += 1;
        }
    }

    let res = dispatch_syscall(Syscall::Mkdir(path_ptr, path_len));
    if res < 0 {
        let msg = "mkdir: Could not create directory\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
    } else {
        dispatch_syscall(Syscall::Exit(0));
    }
}
