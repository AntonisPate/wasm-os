use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;

pub fn shell_main(_argc: usize, _argv: *const *const u8) {
    let mut buffer = [0u8; 128];
    
    // Attempt to read from TTY
    let bytes_read = dispatch_syscall(Syscall::Read(vfs::STDIN, buffer.as_mut_ptr(), buffer.len()));
    
    if bytes_read > 0 {
        let input = &mut buffer[..bytes_read as usize];
        
        // Find the end of the command name
        let mut cmd_end = input.len();
        for i in 0..input.len() {
            if input[i] == b' ' || input[i] == b'\r' || input[i] == b'\n' {
                cmd_end = i;
                break;
            }
        }
        
        let (cmd, rest) = input.split_at_mut(cmd_end);
        
        // Arguments start after the command (skip the delimiter if present)
        let args_part = if !rest.is_empty() {
            &mut rest[1..]
        } else {
            &mut []
        };
        
        // Serialize arguments: replace spaces and newlines with null bytes
        for i in 0..args_part.len() {
            if args_part[i] == b' ' || args_part[i] == b'\r' || args_part[i] == b'\n' {
                args_part[i] = 0;
            }
        }
        
        if !cmd.is_empty() {
            let res = dispatch_syscall(Syscall::Spawn(
                cmd.as_ptr(), 
                cmd.len(), 
                args_part.as_ptr(), 
                args_part.len()
            ));

            if res == -1 {
                let msg = "Unknown command\r\n";
                dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
            }
        }
    } else if bytes_read == -3 {
        // Blocked, just return to scheduler
        return;
    }
}
