use crate::process::{PROCESS_TABLE, FileType, ProcessState};
use crate::tty::TTY;
use crate::syscalls::validate_memory;
use crate::shared_memory;
use crate::fs::{RAM_FS, traverse_path, FsNode};
use core::slice;
use alloc::vec::Vec;
use alloc::string::{String, ToString};

pub const STDIN: u32 = 0;
pub const STDOUT: u32 = 1;

pub fn resolve_path(pid: u32, path: &str) -> String {
    let mut full_path = if path.starts_with('/') {
        String::from(path)
    } else {
        let table = PROCESS_TABLE.lock();
        if let Some(proc) = table.iter().find(|p| p.id == pid) {
            let mut base = proc.cwd.clone();
            if !base.ends_with('/') {
                base.push('/');
            }
            base.push_str(path);
            base
        } else {
            String::from(path)
        }
    };

    let mut components = Vec::new();
    for segment in full_path.split('/') {
        match segment {
            "" | "." => {}
            ".." => { components.pop(); }
            _ => { components.push(segment); }
        }
    }

    let mut result = String::from("/");
    for (i, comp) in components.iter().enumerate() {
        if i > 0 { result.push('/'); }
        result.push_str(comp);
    }
    result
}

pub fn vfs_read(pid: u32, fd: u32, ptr: *mut u8, len: usize) -> i32 {
    if fd >= 8 {
        return -9; // EBADF
    }

    // 1. Lock Sandwich: Get file info
    let file_info = {
        let table = PROCESS_TABLE.lock();
        if let Some(proc) = table.iter().find(|p| p.id == pid) {
            match proc.file_descriptors[fd as usize] {
                Some(FileType::File { ref path, offset }) => Some((path.clone(), offset)),
                Some(FileType::Tty) => None,
                None => return -9,
            }
        } else {
            return -9;
        }
    }; // PROCESS_TABLE lock dropped

    if let Some((path, offset)) = file_info {
        // 2. Interact with RAM_FS
        let bytes_read = {
            let mut fs_root = RAM_FS.lock();
            if let Some(FsNode::File(data)) = traverse_path(&mut *fs_root, &path, false) {
                let available = data.len().saturating_sub(offset);
                let copy_len = available.min(len);
                if copy_len > 0 {
                    if !validate_memory(pid, ptr as usize, copy_len) {
                        return -2;
                    }
                    unsafe {
                        core::ptr::copy_nonoverlapping(data[offset..].as_ptr(), ptr, copy_len);
                    }
                }
                copy_len
            } else {
                0
            }
        }; // RAM_FS lock dropped

        // 3. Update offset
        if bytes_read > 0 {
            let mut table = PROCESS_TABLE.lock();
            if let Some(proc) = table.iter_mut().find(|p| p.id == pid) {
                if let Some(FileType::File { ref mut offset, .. }) = proc.file_descriptors[fd as usize] {
                    *offset += bytes_read;
                }
            }
        }
        return bytes_read as i32;
    }

    // Handle Tty
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

pub fn vfs_write(pid: u32, fd: u32, ptr: *const u8, len: usize) -> i32 {
    if fd >= 8 {
        return -9; // EBADF
    }

    // 1. Lock Sandwich: Get file info
    let file_info = {
        let table = PROCESS_TABLE.lock();
        if let Some(proc) = table.iter().find(|p| p.id == pid) {
            match proc.file_descriptors[fd as usize] {
                Some(FileType::File { ref path, offset }) => Some((path.clone(), offset)),
                Some(FileType::Tty) => None,
                None => return -9,
            }
        } else {
            return -9;
        }
    }; // PROCESS_TABLE lock dropped

    if let Some((path, offset)) = file_info {
        // 2. Interact with RAM_FS
        if !validate_memory(pid, ptr as usize, len) {
            return -2;
        }
        let data_to_write = unsafe { core::slice::from_raw_parts(ptr, len) };

        let bytes_written = {
            let mut fs_root = RAM_FS.lock();
            // Split path into parent and name
            let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(filename) = parts.pop() {
                let parent_path = parts.join("/");
                if let Some(FsNode::Directory(entries)) = traverse_path(&mut *fs_root, &parent_path, true) {
                    let data = match entries.entry(filename.to_string()).or_insert(FsNode::File(Vec::new())) {
                        FsNode::File(d) => d,
                        _ => return -1, // Not a file
                    };
                    
                    let end_pos = offset + len;
                    if end_pos > data.len() {
                        data.resize(end_pos, 0);
                    }
                    data[offset..end_pos].copy_from_slice(data_to_write);
                    len
                } else { 0 }
            } else { 0 }
        }; // RAM_FS lock dropped

        // 3. Update offset
        if bytes_written > 0 {
            let mut table = PROCESS_TABLE.lock();
            if let Some(proc) = table.iter_mut().find(|p| p.id == pid) {
                if let Some(FileType::File { ref mut offset, .. }) = proc.file_descriptors[fd as usize] {
                    *offset += bytes_written;
                }
            }
        }
        return bytes_written as i32;
    }

    // Handle Tty
    // Validate memory WITHOUT holding any other locks
    if !validate_memory(pid, ptr as usize, len) {
        return -2; // Memory violation
    }

    let data = unsafe { slice::from_raw_parts(ptr, len) };
    shared_memory::write_to_shared_memory(data);
    0
}
