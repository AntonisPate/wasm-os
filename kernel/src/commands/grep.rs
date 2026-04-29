use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;

// Static state so grep can resume across yields (kernel re-calls entry point)
static mut GREP_READ_FD: i32 = -1;
static mut GREP_OPENED_FILE: bool = false;
static mut GREP_LINE_BUF: [u8; 512] = [0u8; 512];
static mut GREP_LINE_LEN: usize = 0;
const GREP_LINE_BUF_CAP: usize = 512;

pub fn grep_main(argc: usize, argv: *const *const u8) {
    let args = unsafe { slice::from_raw_parts(argv, argc) };

    if argc < 2 {
        let msg = b"Usage: grep <pattern> [file]\r\n";
        dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        dispatch_syscall(Syscall::Exit(1));
        return;
    }

    // Parse pattern from argv[1] (null-terminated)
    let pattern_ptr = args[1];
    let mut pattern_len = 0usize;
    unsafe { while *pattern_ptr.add(pattern_len) != 0 { pattern_len += 1; } }
    let pattern = unsafe { slice::from_raw_parts(pattern_ptr, pattern_len) };

    // On first call: open file or use stdin
    if unsafe { GREP_READ_FD } == -1 {
        if argc >= 3 {
            let fname_ptr = args[2];
            let mut fname_len = 0usize;
            unsafe { while *fname_ptr.add(fname_len) != 0 { fname_len += 1; } }
            let fd = dispatch_syscall(Syscall::Open(fname_ptr, fname_len, 0));
            if fd < 0 {
                let msg = b"grep: No such file\r\n";
                dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
                dispatch_syscall(Syscall::Exit(1));
                return;
            }
            unsafe { GREP_READ_FD = fd; GREP_OPENED_FILE = true; }
        } else {
            unsafe { GREP_READ_FD = vfs::STDIN as i32; GREP_OPENED_FILE = false; }
        }
        unsafe { GREP_LINE_LEN = 0; }
    }

    let read_fd = unsafe { GREP_READ_FD } as u32;
    let mut chunk = [0u8; 64];

    loop {
        let n = dispatch_syscall(Syscall::Read(read_fd, chunk.as_mut_ptr(), chunk.len()));
        if n == -3 {
            // Blocked — yield to kernel; state preserved in statics
            return;
        }
        if n <= 0 {
            // EOF — flush any remaining partial line
            let line_len = unsafe { GREP_LINE_LEN };
            if line_len > 0 {
                let line = unsafe { &GREP_LINE_BUF[..line_len] };
                if line_contains(line, pattern) {
                    write_bytes(line);
                    write_bytes(b"\r\n");
                }
            }
            break;
        }

        let data = &chunk[..n as usize];
        for &b in data {
            if b == b'\n' || b == b'\r' {
                let line_len = unsafe { GREP_LINE_LEN };
                if line_len > 0 {
                    let line = unsafe { &GREP_LINE_BUF[..line_len] };
                    if line_contains(line, pattern) {
                        write_bytes(line);
                        write_bytes(b"\r\n");
                    }
                    unsafe { GREP_LINE_LEN = 0; }
                }
            } else {
                let line_len = unsafe { GREP_LINE_LEN };
                if line_len < GREP_LINE_BUF_CAP - 1 {
                    unsafe {
                        GREP_LINE_BUF[line_len] = b;
                        GREP_LINE_LEN = line_len + 1;
                    }
                }
            }
        }
    }

    // Cleanup
    if unsafe { GREP_OPENED_FILE } {
        dispatch_syscall(Syscall::Close(read_fd));
    }
    unsafe {
        GREP_READ_FD = -1;
        GREP_OPENED_FILE = false;
        GREP_LINE_LEN = 0;
    }
    dispatch_syscall(Syscall::Exit(0));
}

/// Returns true if `pattern` is a substring of `line`.
fn line_contains(line: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() { return true; }
    if pattern.len() > line.len() { return false; }
    for i in 0..=(line.len() - pattern.len()) {
        if &line[i..i + pattern.len()] == pattern {
            return true;
        }
    }
    false
}

/// Write all bytes to stdout, handling partial writes.
fn write_bytes(data: &[u8]) {
    let mut written = 0;
    while written < data.len() {
        let res = dispatch_syscall(Syscall::Write(
            vfs::STDOUT,
            unsafe { data.as_ptr().add(written) },
            data.len() - written,
        ));
        if res == -3 || res <= 0 { break; }
        written += res as usize;
    }
}
