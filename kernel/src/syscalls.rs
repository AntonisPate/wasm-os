use crate::process::{CURRENT_PROCESS, PROCESS_TABLE, ProcessState};
use crate::shared_memory;
use crate::tty::TTY;
use core::slice;

pub enum Syscall {
    Read(*mut u8, usize),
    Write(*const u8, usize),
    Exit(i32),
}

pub fn dispatch_syscall(call: Syscall) -> i32 {
    let pid = unsafe { CURRENT_PROCESS.unwrap_or(0) };

    if pid == 0 {
        return -1;
    }

    match call {
        Syscall::Write(ptr, len) => {
            if validate_memory(pid, ptr as usize, len) {
                let data = unsafe { slice::from_raw_parts(ptr, len) };
                shared_memory::write_to_shared_memory(data);
                0
            } else {
                -2 // Memory violation
            }
        }
        Syscall::Read(ptr, len) => {
            let mut tty_guard = TTY.lock();
            if tty_guard.is_line_ready() {
                if validate_memory(pid, ptr as usize, len) {
                    let line = tty_guard.get_line();
                    let copy_len = line.len().min(len);
                    unsafe {
                        core::ptr::copy_nonoverlapping(line.as_ptr(), ptr, copy_len);
                    }
                    tty_guard.clear_line();
                    copy_len as i32
                } else {
                    -2 // Memory violation
                }
            } else {
                // Block the process
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    p.state = ProcessState::Blocked;
                }
                -3 // EAGAIN / Blocked
            }
        }
        Syscall::Exit(_status) => {
            let mut table = PROCESS_TABLE.lock();
            if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                p.state = ProcessState::Terminated;
            }
            0
        }
    }
}

fn validate_memory(pid: u32, ptr: usize, len: usize) -> bool {
    // Το εσωτερικό Shell (PID 1) είναι trusted και έχει πρόσβαση παντού
    if pid == 1 {
        return true;
    }

    let table = PROCESS_TABLE.lock();
    if let Some(proc) = table.iter().find(|p| p.id == pid) {
        let in_bounds = ptr >= proc.memory_start && (ptr + len) <= (proc.memory_start + proc.size);
        return in_bounds;
    }
    false
}
