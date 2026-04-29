use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;

static mut PROMPT_PRINTED: bool = false;
static mut WAITING_FOR_PID: u32 = 0;

pub fn shell_main(_argc: usize, _argv: *const *const u8) {
    unsafe {
        if WAITING_FOR_PID > 0 {
            let res = dispatch_syscall(Syscall::Wait(WAITING_FOR_PID));
            if res == -3 { return; }
            WAITING_FOR_PID = 0;
            PROMPT_PRINTED = false;
            return;
        }

        if !PROMPT_PRINTED {
            let mut cwd_buf = [0u8; 128];
            let cwd_len = dispatch_syscall(Syscall::GetCwd(cwd_buf.as_mut_ptr(), cwd_buf.len()));
            let cwd = if cwd_len > 0 {
                core::str::from_utf8(&cwd_buf[..cwd_len as usize]).unwrap_or("/")
            } else { "/" };
            let prompt = alloc::format!("[ {} ] > ", cwd);
            dispatch_syscall(Syscall::Write(vfs::STDOUT, prompt.as_ptr(), prompt.len()));
            PROMPT_PRINTED = true;
        }
    }

    let mut buffer = [0u8; 128];
    let bytes_read = dispatch_syscall(Syscall::Read(vfs::STDIN, buffer.as_mut_ptr(), buffer.len()));
    
    if bytes_read > 0 {
        let input = &mut buffer[..bytes_read as usize];
        
        // Find pipe position, but only consider '|' that isn't after a '>'
        // e.g. "echo aa > a.txt" has no real pipe; "ls | cat" does.
        let mut pipe_pos = None;
        let mut seen_redirect = false;
        for i in 0..input.len() {
            if input[i] == b'>' {
                seen_redirect = true;
            }
            if input[i] == b'|' && !seen_redirect {
                pipe_pos = Some(i);
                break;
            }
        }

        if let Some(pos) = pipe_pos {
            let (left_part, right_part) = input.split_at_mut(pos);
            let right_raw = &mut right_part[1..]; // Skip '|'

            // Create Pipe
            let mut pipe_read_fd = 0u32;
            let mut pipe_write_fd = 0u32;
            dispatch_syscall(Syscall::Pipe(&mut pipe_read_fd, &mut pipe_write_fd));

            // Spawn left side, handling any '>' redirection within it.
            // If the left side redirects to a file, the pipe write end is unused
            // (closed early) and the right side will get EOF immediately.
            let (l_cmd, l_args, left_stdout_fd) = parse_cmd_with_redirect(left_part, pipe_write_fd);
            dispatch_syscall(Syscall::Spawn(
                l_cmd.as_ptr(), l_cmd.len(),
                l_args.as_ptr(), l_args.len(),
                vfs::STDIN, left_stdout_fd
            ));
            // If left was redirected to a file (not the pipe), close our file fd
            if left_stdout_fd != pipe_write_fd {
                dispatch_syscall(Syscall::Close(left_stdout_fd));
            }

            // Spawn right side, handling any '>' redirection within it.
            let (r_cmd, r_args, right_stdout_fd) = parse_cmd_with_redirect(right_raw, vfs::STDOUT);
            let pid2 = dispatch_syscall(Syscall::Spawn(
                r_cmd.as_ptr(), r_cmd.len(),
                r_args.as_ptr(), r_args.len(),
                pipe_read_fd, right_stdout_fd
            ));
            if right_stdout_fd != vfs::STDOUT {
                dispatch_syscall(Syscall::Close(right_stdout_fd));
            }

            // Close shell's pipe ends
            dispatch_syscall(Syscall::Close(pipe_read_fd));
            dispatch_syscall(Syscall::Close(pipe_write_fd));

            if pid2 > 0 {
                unsafe { WAITING_FOR_PID = pid2 as u32; }
            }
        } else {
            // Regular command (no pipe)
            let (cmd, args_part) = parse_cmd(input);
            if cmd == b"cd" {
                let mut arg_len = 0;
                while arg_len < args_part.len() && args_part[arg_len] != 0 { arg_len += 1; }
                dispatch_syscall(Syscall::Chdir(args_part.as_ptr(), arg_len));
            } else {
                // Parse for '>' redirection. We already called parse_cmd, so work with args_part.
                let mut stdout_fd = vfs::STDOUT;
                let mut args_end = args_part.len();
                for i in 0..args_part.len() {
                    if args_part[i] == b'>' {
                        let after = &args_part[i+1..];
                        let mut ns = 0;
                        while ns < after.len() && (after[ns] == b' ' || after[ns] == 0) { ns += 1; }
                        let mut ne = ns;
                        while ne < after.len() && after[ne] != b' ' && after[ne] != 0 && after[ne] != b'\r' && after[ne] != b'\n' { ne += 1; }
                        if ns < ne {
                            let fd = dispatch_syscall(Syscall::Open(after[ns..].as_ptr(), ne - ns, 0));
                            if fd >= 0 { stdout_fd = fd as u32; }
                        }
                        args_end = i;
                        break;
                    }
                }
                let final_args = &args_part[..args_end];
                let res = dispatch_syscall(Syscall::Spawn(
                    cmd.as_ptr(), cmd.len(),
                    final_args.as_ptr(), final_args.len(),
                    vfs::STDIN, stdout_fd
                ));
                if stdout_fd != vfs::STDOUT { dispatch_syscall(Syscall::Close(stdout_fd)); }
                if res > 0 { unsafe { WAITING_FOR_PID = res as u32; } }
            }
        }
        dispatch_syscall(Syscall::Write(vfs::STDOUT, b"\r\n".as_ptr(), 2));
        unsafe { PROMPT_PRINTED = false; }
    } else if bytes_read == -3 { return; }
}

/// Parse a command slice, detect any `>` output redirection, open the file,
/// and return `(cmd, args_without_redirect, stdout_fd)`.
/// If no redirect is found, `stdout_fd` is set to `default_stdout`.
fn parse_cmd_with_redirect<'a>(input: &'a mut [u8], default_stdout: u32) -> (&'a [u8], &'a mut [u8], u32) {
    let (cmd, args) = parse_cmd(input);
    
    let mut stdout_fd = default_stdout;
    let mut args_end = args.len();

    for i in 0..args.len() {
        if args[i] == b'>' {
            // Find filename after '>'
            let after = &args[i+1..];
            let mut ns = 0;
            while ns < after.len() && (after[ns] == b' ' || after[ns] == 0) { ns += 1; }
            let mut ne = ns;
            while ne < after.len() && after[ne] != b' ' && after[ne] != 0 && after[ne] != b'\r' && after[ne] != b'\n' { ne += 1; }

            if ns < ne {
                let fd = dispatch_syscall(Syscall::Open(after[ns..].as_ptr(), ne - ns, 0));
                if fd >= 0 { stdout_fd = fd as u32; }
            }
            args_end = i; // truncate args before '>'
            break;
        }
    }

    (cmd, &mut args[..args_end], stdout_fd)
}

fn parse_cmd(input: &mut [u8]) -> (&[u8], &mut [u8]) {
    let mut start = 0;
    while start < input.len() && (input[start] == b' ' || input[start] == b'\n' || input[start] == b'\r') { start += 1; }
    let trimmed = &mut input[start..];
    
    let mut cmd_end = trimmed.len();
    for i in 0..trimmed.len() {
        if trimmed[i] == b' ' || trimmed[i] == b'\r' || trimmed[i] == b'\n' {
            cmd_end = i;
            break;
        }
    }
    
    let (cmd, rest) = trimmed.split_at_mut(cmd_end);
    let args = if !rest.is_empty() { &mut rest[1..] } else { &mut [] };
    
    // Normalize args: spaces/newlines become null separators
    for i in 0..args.len() {
        if args[i] == b' ' || args[i] == b'\r' || args[i] == b'\n' {
            args[i] = 0;
        }
    }
    
    (cmd, args)
}
