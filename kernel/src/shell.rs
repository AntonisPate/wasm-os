use crate::syscalls::{dispatch_syscall, Syscall};

pub fn shell_main() {
    let mut buffer = [0u8; 128];
    
    // Attempt to read from TTY
    let bytes_read = dispatch_syscall(Syscall::Read(buffer.as_mut_ptr(), buffer.len()));
    
    if bytes_read > 0 {
        if let Ok(cmd) = core::str::from_utf8(&buffer[..bytes_read as usize]) {
            let msg = if cmd == "ping" {
                "pong\r\n"
            } else if !cmd.is_empty() {
                "Unknown command\r\n"
            } else {
                ""
            };

            if !msg.is_empty() {
                dispatch_syscall(Syscall::Write(msg.as_ptr(), msg.len()));
            }
        }
    } else if bytes_read == -3 {
        // Blocked, just return to scheduler
        return;
    }
}
