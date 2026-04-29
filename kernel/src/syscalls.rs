use crate::process::{CURRENT_PROCESS, PROCESS_TABLE, ProcessState};
use crate::vfs;

pub enum Syscall {
    Read(u32, *mut u8, usize),
    Write(u32, *const u8, usize),
    Exit(i32),
}

pub fn dispatch_syscall(call: Syscall) -> i32 {
    let pid = unsafe { CURRENT_PROCESS.unwrap_or(0) };

    if pid == 0 {
        return -1;
    }

    match call {
        Syscall::Write(fd, ptr, len) => vfs::vfs_write(pid, fd, ptr, len),
        Syscall::Read(fd, ptr, len) => vfs::vfs_read(pid, fd, ptr, len),
        Syscall::Exit(_status) => {
            let mut table = PROCESS_TABLE.lock();
            if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                p.state = ProcessState::Terminated;
            }
            0
        }
    }
}

pub fn validate_memory(pid: u32, ptr: usize, len: usize) -> bool {
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
