use crate::process::{PROCESS_TABLE, FileType, ProcessState};
use crate::tty::TTY;
use crate::syscalls::validate_memory;
use crate::shared_memory;
use core::slice;

pub const STDIN: u32 = 0;
pub const STDOUT: u32 = 1;

pub fn vfs_read(pid: u32, fd: u32, ptr: *mut u8, len: usize) -> i32 {
    if fd >= 8 {
        return -9; // EBADF
    }

    let file_type = {
        let table = PROCESS_TABLE.lock();
        if let Some(proc) = table.iter().find(|p| p.id == pid) {
            proc.file_descriptors[fd as usize]
        } else {
            return -9; // EBADF
        }
    }; // PROCESS_TABLE lock dropped here

    match file_type {
        Some(FileType::Tty) => {
            // Validate memory WITHOUT holding TTY lock
            if !validate_memory(pid, ptr as usize, len) {
                return -2; // Memory violation
            }

            let mut tty_guard = TTY.lock();
            if tty_guard.is_line_ready() {
                let line = tty_guard.get_line();
                let copy_len = line.len().min(len);
                unsafe {
                    core::ptr::copy_nonoverlapping(line.as_ptr(), ptr, copy_len);
                }
                tty_guard.clear_line();
                drop(tty_guard); // Drop TTY lock before returning
                copy_len as i32
            } else {
                drop(tty_guard); // Drop TTY lock before acquiring PROCESS_TABLE lock
                
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    p.state = ProcessState::Blocked;
                }
                -3 // EAGAIN / Blocked
            }
        }
        None => -9,
    }
}

pub fn vfs_write(pid: u32, fd: u32, ptr: *const u8, len: usize) -> i32 {
    if fd >= 8 {
        return -9; // EBADF
    }

    let file_type = {
        let table = PROCESS_TABLE.lock();
        if let Some(proc) = table.iter().find(|p| p.id == pid) {
            proc.file_descriptors[fd as usize]
        } else {
            return -9; // EBADF
        }
    }; // PROCESS_TABLE lock dropped here

    match file_type {
        Some(FileType::Tty) => {
            // Validate memory WITHOUT holding any other locks
            if !validate_memory(pid, ptr as usize, len) {
                return -2; // Memory violation
            }

            let data = unsafe { slice::from_raw_parts(ptr, len) };
            shared_memory::write_to_shared_memory(data);
            0
        }
        None => -9,
    }
}
