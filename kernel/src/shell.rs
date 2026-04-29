use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;

static mut PROMPT_PRINTED: bool = false;
static mut WAITING_FOR_PID: u32 = 0;

pub fn shell_main(_argc: usize, _argv: *const *const u8) {
    unsafe {
        // If we were waiting for a process, check if it's done
        if WAITING_FOR_PID > 0 {
            let res = dispatch_syscall(Syscall::Wait(WAITING_FOR_PID));
            if res == -3 {
                return; // Still waiting
            }
            WAITING_FOR_PID = 0;
            PROMPT_PRINTED = false;
            // When a command finishes, we want a fresh prompt on the next call
            return;
        }

        if !PROMPT_PRINTED {
            let mut cwd_buf = [0u8; 128];
            let cwd_len = dispatch_syscall(Syscall::GetCwd(cwd_buf.as_mut_ptr(), cwd_buf.len()));
            let cwd = if cwd_len > 0 {
                core::str::from_utf8(&cwd_buf[..cwd_len as usize]).unwrap_or("/")
            } else {
                "/"
            };
            
            let prompt = alloc::format!("[ {} ] > ", cwd);
            dispatch_syscall(Syscall::Write(vfs::STDOUT, prompt.as_ptr(), prompt.len()));
            PROMPT_PRINTED = true;
        }
    }

    let mut buffer = [0u8; 128];
    let bytes_read = dispatch_syscall(Syscall::Read(vfs::STDIN, buffer.as_mut_ptr(), buffer.len()));
    
    if bytes_read > 0 {
        let input = &mut buffer[..bytes_read as usize];
        // ... parse command ...
        
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
        
        // Redirection support
        let mut stdout_fd = vfs::STDOUT;
        let mut final_args_len = args_part.len();
        
        let mut redirect_idx = None;
        for i in 0..args_part.len() {
            if args_part[i] == b'>' {
                redirect_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = redirect_idx {
            let filename_start = idx + 1;
            let actual_args_len = idx;
            
            // Find start of filename (skip spaces)
            let mut name_start = filename_start;
            while name_start < args_part.len() && args_part[name_start] == b' ' {
                name_start += 1;
            }
            
            // Find end of filename
            let mut name_end = args_part.len();
            for i in name_start..args_part.len() {
                let c = args_part[i];
                if c == b' ' || c == b'\r' || c == b'\n' || c == 0 {
                    name_end = i;
                    break;
                }
            }
            
            if name_start < name_end {
                let filename = &args_part[name_start..name_end];
                let fd = dispatch_syscall(Syscall::Open(filename.as_ptr(), filename.len(), 0));
                if fd >= 0 {
                    stdout_fd = fd as u32;
                }
            }
            final_args_len = actual_args_len;
        }

        let final_args = &mut args_part[..final_args_len];
        
        // Serialize arguments: replace spaces and newlines with null bytes
        for i in 0..final_args.len() {
            if final_args[i] == b' ' || final_args[i] == b'\r' || final_args[i] == b'\n' {
                final_args[i] = 0;
            }
        }
        
        if !cmd.is_empty() {
            if cmd == b"cd" {
                // Find length of first argument in final_args (already null-separated)
                let mut arg_len = 0;
                while arg_len < final_args.len() && final_args[arg_len] != 0 {
                    arg_len += 1;
                }
                
                let res = dispatch_syscall(Syscall::Chdir(final_args.as_ptr(), arg_len));
                if res < 0 {
                    let msg = "cd: No such directory\r\n";
                    dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
                }
            } else {
                let res = dispatch_syscall(Syscall::Spawn(
                    cmd.as_ptr(), 
                    cmd.len(), 
                    final_args.as_ptr(), 
                    final_args.len(),
                    stdout_fd
                ));

                if stdout_fd != vfs::STDOUT {
                    dispatch_syscall(Syscall::Close(stdout_fd));
                }

                if res > 0 {
                    unsafe { WAITING_FOR_PID = res as u32; }
                    let wait_res = dispatch_syscall(Syscall::Wait(res as u32));
                    if wait_res == -3 {
                        return;
                    }
                    unsafe { WAITING_FOR_PID = 0; }
                }

                if res == -1 {
                    let msg = "Unknown command\r\n";
                    dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
                }
            }
        }
        // Add a newline before the next prompt for better readability
        dispatch_syscall(Syscall::Write(vfs::STDOUT, b"\r\n".as_ptr(), 2));
        unsafe { PROMPT_PRINTED = false; }
    } else if bytes_read == -3 {
        // Blocked, just return to scheduler
        return;
    }
}
