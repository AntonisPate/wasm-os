use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;

pub fn help_main(_argc: usize, _argv: *const *const u8) {
    let msg = "Available commands:\r\n\
               echo <text> [> file]  - Print text to stdout or file\r\n\
               cat <file>            - Print file contents\r\n\
               ls [-t | -ta] [path]  - List directory contents (-t: tree, -ta: tree from root)\r\n\
               cd <path>             - Change directory\r\n\
               mkdir <path>          - Create directory\r\n\
               rm [-rf] <path>       - Remove file or directory\r\n\
               help                  - Show this message\r\n";
    dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
    dispatch_syscall(Syscall::Exit(0));
}
