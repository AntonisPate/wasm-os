use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

pub fn cat_main(argc: usize, argv: *const *const u8) {
    // Determine which fd to read from:
    // - If a filename argument is given, open it and read from it.
    // - If no argument given (e.g. right side of a pipe), read from stdin (fd 0).
    let read_fd: u32;
    let opened_file: bool;

    if argc >= 2 {
        let args = unsafe { slice::from_raw_parts(argv, argc) };
        let filename_ptr = args[1];
        let mut filename_len = 0;
        unsafe {
            while *filename_ptr.add(filename_len) != 0 { filename_len += 1; }
        }

        let fd = dispatch_syscall(Syscall::Open(filename_ptr, filename_len, 0));
        if fd < 0 {
            let msg = "cat: Could not open file\r\n";
            dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
            dispatch_syscall(Syscall::Exit(1));
            return;
        }
        read_fd = fd as u32;
        opened_file = true;
    } else {
        // No filename: read from stdin (pipe or tty)
        read_fd = vfs::STDIN;
        opened_file = false;
    }

    let mut buffer = [0u8; 64];
    loop {
        let bytes_read = dispatch_syscall(Syscall::Read(read_fd, buffer.as_mut_ptr(), buffer.len()));
        if bytes_read == -3 {
            // Blocked (pipe empty or tty not ready) — yield to kernel
            return;
        }
        if bytes_read <= 0 {
            // EOF or error
            break;
        }

        let mut written = 0;
        while written < bytes_read as usize {
            let res = dispatch_syscall(Syscall::Write(vfs::STDOUT, unsafe { buffer.as_ptr().add(written) }, bytes_read as usize - written));
            if res == -3 {
                // Write blocked (downstream pipe full) — yield
                return;
            }
            if res > 0 {
                written += res as usize;
            } else {
                break;
            }
        }
    }

    if opened_file {
        dispatch_syscall(Syscall::Close(read_fd));
    }
    dispatch_syscall(Syscall::Exit(0));
}
