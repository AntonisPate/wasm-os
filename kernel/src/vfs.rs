use crate::process::{PROCESS_TABLE, FileType, ProcessState, BlockedReason};
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
        if i > 0 && result != "/" { result.push('/'); }
        result.push_str(comp);
    }
    result
}

pub fn vfs_read(pid: u32, fd: u32, ptr: *mut u8, len: usize) -> i32 {
    if fd >= 8 { return -9; }
    if !validate_memory(pid, ptr as usize, len) { return -2; }

    let fd_type = {
        let table = PROCESS_TABLE.lock();
        if let Some(p) = table.iter().find(|p| p.id == pid) {
            match &p.file_descriptors[fd as usize] {
                Some(ft) => ft.clone(),
                None => return -9,
            }
        } else { return -1; }
    };

    match fd_type {
        FileType::Tty => {
            let mut tty_guard = TTY.lock();
            if tty_guard.is_line_ready() {
                let line = tty_guard.get_line();
                let copy_len = line.len().min(len);
                unsafe { core::ptr::copy_nonoverlapping(line.as_ptr(), ptr, copy_len); }
                tty_guard.clear_line();
                copy_len as i32
            } else {
                drop(tty_guard);
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    p.state = ProcessState::Blocked(BlockedReason::Tty);
                }
                -3
            }
        }
        FileType::PipeRead(buf) => {
            let mut pipe = buf.lock();
            if pipe.data.is_empty() {
                if pipe.writer_count == 0 {
                    drop(pipe);
                    return 0; // EOF
                }
                drop(pipe);
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    p.state = ProcessState::Blocked(BlockedReason::PipeRead(buf.clone()));
                }
                -3
            } else {
                let count = len.min(pipe.data.len());
                for i in 0..count {
                    unsafe { *ptr.add(i) = pipe.data.pop_front().unwrap(); }
                }
                count as i32
            }
        }
        FileType::File { path, offset } => {
            let mut fs_root = RAM_FS.lock();
            if let Some(FsNode::File(data)) = traverse_path(&mut *fs_root, &path, false) {
                if offset >= data.len() { return 0; }
                let copy_len = (data.len() - offset).min(len);
                unsafe { core::ptr::copy_nonoverlapping(data[offset..].as_ptr(), ptr, copy_len); }
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    if let Some(FileType::File { ref mut offset, .. }) = p.file_descriptors[fd as usize] {
                        *offset += copy_len;
                    }
                }
                copy_len as i32
            } else { -1 }
        }
        FileType::PipeWrite(_) => -1,
    }
}

pub fn vfs_write(pid: u32, fd: u32, ptr: *const u8, len: usize) -> i32 {
    if fd >= 8 { return -9; }
    if !validate_memory(pid, ptr as usize, len) { return -2; }

    let fd_type = {
        let table = PROCESS_TABLE.lock();
        if let Some(p) = table.iter().find(|p| p.id == pid) {
            match &p.file_descriptors[fd as usize] {
                Some(ft) => ft.clone(),
                None => return -9,
            }
        } else { return -1; }
    };

    match fd_type {
        FileType::Tty => {
            let data = unsafe { slice::from_raw_parts(ptr, len) };
            shared_memory::write_to_shared_memory(data);
            len as i32
        }
        FileType::PipeWrite(buf) => {
            let mut pipe = buf.lock();
            if pipe.reader_count == 0 {
                drop(pipe);
                return -1; // EPIPE
            }
            if pipe.data.len() >= pipe.capacity {
                drop(pipe);
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    p.state = ProcessState::Blocked(BlockedReason::PipeWrite(buf.clone()));
                }
                return -3;
            }
            let data = unsafe { slice::from_raw_parts(ptr, len) };
            let mut count = 0;
            for &b in data {
                if pipe.data.len() < pipe.capacity {
                    pipe.data.push_back(b);
                    count += 1;
                } else { break; }
            }
            count as i32
        }
        FileType::File { path, offset } => {
            let mut fs_root = RAM_FS.lock();
            let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(filename) = parts.pop() {
                let parent_path = parts.join("/");
                if let Some(FsNode::Directory(entries)) = traverse_path(&mut *fs_root, &parent_path, true) {
                    let data = match entries.entry(filename.to_string()).or_insert(FsNode::File(Vec::new())) {
                        FsNode::File(d) => d,
                        _ => return -1,
                    };
                    let end_pos = offset + len;
                    if end_pos > data.len() { data.resize(end_pos, 0); }
                    unsafe { core::ptr::copy_nonoverlapping(ptr, data[offset..].as_mut_ptr(), len); }
                    let mut table = PROCESS_TABLE.lock();
                    if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                        if let Some(FileType::File { ref mut offset, .. }) = p.file_descriptors[fd as usize] {
                            *offset += len;
                        }
                    }
                    len as i32
                } else { -1 }
            } else { -1 }
        }
        FileType::PipeRead(_) => -1,
    }
}

pub fn vfs_close(pid: u32, fd: u32) -> i32 {
    // Grab the file type first, release table lock, then adjust pipe counts
    let fd_type = {
        let mut table = PROCESS_TABLE.lock();
        if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
            if fd < 8 {
                p.file_descriptors[fd as usize].take()
            } else {
                return -1;
            }
        } else {
            return -1;
        }
    }; // table lock dropped here

    match fd_type {
        Some(FileType::PipeWrite(buf)) => {
            let mut pipe = buf.lock();
            pipe.writer_count = pipe.writer_count.saturating_sub(1);
        }
        Some(FileType::PipeRead(buf)) => {
            let mut pipe = buf.lock();
            pipe.reader_count = pipe.reader_count.saturating_sub(1);
        }
        _ => {}
    }
    0
}
