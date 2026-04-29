use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

pub fn echo_main(argc: usize, argv: *const *const u8) {
    if argc > 1 {
        let args = unsafe { slice::from_raw_parts(argv, argc) };
        
        // Skip argv[0] which is the command name
        for i in 1..argc {
            let arg_ptr = args[i];
            
            // We need to find the length of the null-terminated string
            let mut len = 0;
            unsafe {
                while *arg_ptr.add(len) != 0 {
                    len += 1;
                }
            }
            
            dispatch_syscall(Syscall::Write(vfs::STDOUT, arg_ptr, len));
            
            if i < argc - 1 {
                dispatch_syscall(Syscall::Write(vfs::STDOUT, b" ".as_ptr(), 1));
            }
        }
    }
    
    dispatch_syscall(Syscall::Write(vfs::STDOUT, b"\r\n".as_ptr(), 2));
    dispatch_syscall(Syscall::Exit(0));
}
