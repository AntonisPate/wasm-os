use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

pub fn cat_main(argc: usize, argv: *const *const u8) {
    if argc < 2 {
        let msg = "Usage: cat <filename>\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
        return;
    }

    let args = unsafe { slice::from_raw_parts(argv, argc) };
    let filename_ptr = args[1];
    
    // Find length of null-terminated filename
    let mut filename_len = 0;
    unsafe {
        while *filename_ptr.add(filename_len) != 0 {
            filename_len += 1;
        }
    }

    let fd = dispatch_syscall(Syscall::Open(filename_ptr, filename_len, 0));
    if fd < 0 {
        let msg = "cat: Could not open file\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
        return;
    }

    let mut buffer = [0u8; 64];
    loop {
        let bytes_read = dispatch_syscall(Syscall::Read(fd as u32, buffer.as_mut_ptr(), buffer.len()));
        if bytes_read <= 0 {
            break;
        }
        dispatch_syscall(Syscall::Write(vfs::STDOUT, buffer.as_ptr(), bytes_read as usize));
    }

    dispatch_syscall(Syscall::Close(fd as u32));
    dispatch_syscall(Syscall::Exit(0));
}
