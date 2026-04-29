use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

pub fn rm_main(argc: usize, argv: *const *const u8) {
    if argc < 2 {
        let msg = "Usage: rm [-rf] <path>\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
        return;
    }

    let args = unsafe { slice::from_raw_parts(argv, argc) };
    let mut recursive = false;
    let mut path_idx = 1;

    // Very simple arg parsing
    let first_arg_ptr = args[1];
    let mut first_arg_len = 0;
    unsafe {
        while *first_arg_ptr.add(first_arg_len) != 0 {
            first_arg_len += 1;
        }
        let first_arg = core::str::from_utf8(slice::from_raw_parts(first_arg_ptr, first_arg_len)).unwrap_or("");
        if first_arg == "-rf" {
            recursive = true;
            if argc < 3 {
                let msg = "Usage: rm [-rf] <path>\r\n";
                dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
                dispatch_syscall(Syscall::Exit(1));
                return;
            }
            path_idx = 2;
        }
    }

    let path_ptr = args[path_idx];
    let mut path_len = 0;
    unsafe {
        while *path_ptr.add(path_len) != 0 {
            path_len += 1;
        }
    }

    let res = dispatch_syscall(Syscall::Unlink(path_ptr, path_len, recursive));
    if res < 0 {
        let msg = "rm: Could not remove path\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
    } else {
        dispatch_syscall(Syscall::Exit(0));
    }
}
