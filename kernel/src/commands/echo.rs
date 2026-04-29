use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

pub fn echo_main(argc: usize, argv: *const *const u8) {
    if argc > 1 {
        let args = unsafe { slice::from_raw_parts(argv, argc) };
        for i in 1..argc {
            let arg_ptr = args[i];
            let mut len = 0;
            unsafe {
                while *arg_ptr.add(len) != 0 { len += 1; }
            }
            
            let mut written = 0;
            while written < len {
                let res = dispatch_syscall(Syscall::Write(vfs::STDOUT, unsafe { arg_ptr.add(written) }, len - written));
                if res == -3 { return; }
                if res > 0 { written += res as usize; }
                else { break; }
            }
            
            if i < argc - 1 {
                let res = dispatch_syscall(Syscall::Write(vfs::STDOUT, b" ".as_ptr(), 1));
                if res == -3 { return; }
            }
        }
    }
    
    let res = dispatch_syscall(Syscall::Write(vfs::STDOUT, b"\r\n".as_ptr(), 2));
    if res == -3 { return; }
    dispatch_syscall(Syscall::Exit(0));
}
