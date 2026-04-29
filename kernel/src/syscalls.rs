use crate::fs::{RAM_FS, traverse_path, FsNode};
use crate::vfs;
use crate::process::{CURRENT_PROCESS, PROCESS_TABLE, ProcessState, FileType, BlockedReason};
use core::slice;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::{String, ToString};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;

pub enum Syscall {
    Read(u32, *mut u8, usize),
    Write(u32, *const u8, usize),
    Exit(i32),
    Spawn(*const u8, usize, *const u8, usize, u32, u32),
    Open(*const u8, usize, u32),
    Close(u32),
    Mkdir(*const u8, usize),
    Unlink(*const u8, usize, bool),
    Chdir(*const u8, usize),
    ReadDir(*const u8, usize, *mut u8, usize),
    GetCwd(*mut u8, usize),
    Wait(u32),
    Pipe(*mut u32, *mut u32),
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
            // Drain the FDs from the table under the table lock,
            // then release the table lock before touching any pipe mutexes.
            let drained_fds: [Option<FileType>; 8] = {
                let mut table = PROCESS_TABLE.lock();
                let mut fds = [const { None }; 8];
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    for fd in 0..8 {
                        fds[fd] = p.file_descriptors[fd].take();
                    }
                    p.state = ProcessState::Terminated;
                }
                fds
            }; // table lock dropped here

            // Now safely adjust pipe counts without holding the table lock
            for fd_opt in &drained_fds {
                match fd_opt {
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
            }
            0
        }
        Syscall::Spawn(cmd_ptr, cmd_len, args_ptr, args_len, stdin_fd, stdout_fd) => {
            if !validate_memory(pid, cmd_ptr as usize, cmd_len) { return -2; }
            if args_len > 0 && !validate_memory(pid, args_ptr as usize, args_len) { return -2; }

            let cmd_slice = unsafe { slice::from_raw_parts(cmd_ptr, cmd_len) };
            let cmd = core::str::from_utf8(cmd_slice).unwrap_or("");
            
            let args_slice = if args_len > 0 {
                unsafe { slice::from_raw_parts(args_ptr, args_len) }
            } else { &[] };

            if let Some(entry) = crate::commands::get_command(cmd) {
                let (stdin_type, stdout_type, cwd) = {
                    let table = PROCESS_TABLE.lock();
                    if let Some(p) = table.iter().find(|p| p.id == pid) {
                        (
                            p.file_descriptors[stdin_fd as usize].clone(),
                            p.file_descriptors[stdout_fd as usize].clone(),
                            p.cwd.clone()
                        )
                    } else { (None, None, String::from("/")) }
                };

                let new_pid = crate::kernel_spawn(0, 0, 0, stdin_type, stdout_type, cwd);
                
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == new_pid) {
                    p.entry_point = Some(entry);
                    
                    // Always prepare argv[0] as the command name
                    let mut storage = Vec::new();
                    let mut cmd_copy = Vec::from(cmd.as_bytes());
                    cmd_copy.push(0); // Null terminator
                    storage.push(cmd_copy);

                    if args_len > 0 {
                        let mut current_arg = Vec::new();
                        for &b in args_slice {
                            if b == 0 {
                                if !current_arg.is_empty() {
                                    current_arg.push(0);
                                    storage.push(current_arg);
                                    current_arg = Vec::new();
                                }
                            } else {
                                current_arg.push(b);
                            }
                        }
                        if !current_arg.is_empty() {
                            current_arg.push(0);
                            storage.push(current_arg);
                        }
                    }

                    let mut ptrs = Vec::new();
                    for s in &storage {
                        ptrs.push(s.as_ptr());
                    }
                    p.argc = storage.len();
                    p.argv = ptrs.as_ptr();
                    p.arg_storage = Some((storage, ptrs));
                    p.state = ProcessState::Ready;
                }
                new_pid as i32
            } else {
                -1
            }
        }
        Syscall::Pipe(read_fd_ptr, write_fd_ptr) => {
            let mut pipe_buf = crate::pipe::PipeBuffer::new(4096);
            pipe_buf.writer_count = 1; 
            pipe_buf.reader_count = 1;
            let buf = Arc::new(spin::Mutex::new(pipe_buf));
            
            let mut table = PROCESS_TABLE.lock();
            if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                let r_opt = p.file_descriptors.iter().position(|f| f.is_none());
                if let Some(r) = r_opt {
                    p.file_descriptors[r] = Some(FileType::PipeRead(buf.clone()));
                    let w_opt = p.file_descriptors.iter().position(|f| f.is_none());
                    if let Some(w) = w_opt {
                        p.file_descriptors[w] = Some(FileType::PipeWrite(buf));
                        unsafe {
                            *read_fd_ptr = r as u32;
                            *write_fd_ptr = w as u32;
                        }
                        return 0;
                    } else {
                        p.file_descriptors[r] = None;
                    }
                }
            }
            -1
        }
        Syscall::Open(path_ptr, path_len, _flags) => {
            if !validate_memory(pid, path_ptr as usize, path_len) { return -2; }
            let raw_path = unsafe { core::str::from_utf8(slice::from_raw_parts(path_ptr, path_len)).unwrap_or("") };
            let path = vfs::resolve_path(pid, raw_path);
            
            let mut table = PROCESS_TABLE.lock();
            if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                for i in 3..8 {
                    if p.file_descriptors[i].is_none() {
                        p.file_descriptors[i] = Some(FileType::File { path: path.clone(), offset: 0 });
                        return i as i32;
                    }
                }
            }
            -1
        }
        Syscall::Close(fd) => vfs::vfs_close(pid, fd),
        Syscall::Mkdir(path_ptr, path_len) => {
            if !validate_memory(pid, path_ptr as usize, path_len) { return -2; }
            let raw_path = unsafe { core::str::from_utf8(slice::from_raw_parts(path_ptr, path_len)).unwrap_or("") };
            let path = vfs::resolve_path(pid, raw_path);
            
            let mut fs_root = RAM_FS.lock();
            let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(filename) = parts.pop() {
                let parent_path = parts.join("/");
                if let Some(FsNode::Directory(entries)) = traverse_path(&mut *fs_root, &parent_path, true) {
                    if !entries.contains_key(filename) {
                        entries.insert(filename.to_string(), FsNode::Directory(BTreeMap::new()));
                        0
                    } else { -1 }
                } else { -1 }
            } else { -1 }
        }
        Syscall::Unlink(path_ptr, path_len, recursive) => {
            if !validate_memory(pid, path_ptr as usize, path_len) { return -2; }
            let raw_path = unsafe { core::str::from_utf8(slice::from_raw_parts(path_ptr, path_len)).unwrap_or("") };
            let path = vfs::resolve_path(pid, raw_path);
            
            let mut fs_root = RAM_FS.lock();
            let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(filename) = parts.pop() {
                let parent_path = parts.join("/");
                if let Some(FsNode::Directory(entries)) = traverse_path(&mut *fs_root, &parent_path, false) {
                    if entries.remove(filename).is_some() { 0 } else { -1 }
                } else { -1 }
            } else { -1 }
        }
        Syscall::Chdir(path_ptr, path_len) => {
            if !validate_memory(pid, path_ptr as usize, path_len) { return -2; }
            let raw_path = unsafe { core::str::from_utf8(slice::from_raw_parts(path_ptr, path_len)).unwrap_or("") };
            let path = vfs::resolve_path(pid, raw_path);
            
            let mut fs_root = RAM_FS.lock();
            if let Some(FsNode::Directory(_)) = traverse_path(&mut *fs_root, &path, false) {
                drop(fs_root);
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                    p.cwd = path;
                    0
                } else { -1 }
            } else { -1 }
        }
        Syscall::ReadDir(path_ptr, path_len, out_ptr, max_len) => {
            if !validate_memory(pid, path_ptr as usize, path_len) || !validate_memory(pid, out_ptr as usize, max_len) { return -2; }
            let raw_path = unsafe { core::str::from_utf8(slice::from_raw_parts(path_ptr, path_len)).unwrap_or("") };
            let path = vfs::resolve_path(pid, raw_path);
            
            let mut fs_root = RAM_FS.lock();
            if let Some(FsNode::Directory(entries)) = traverse_path(&mut *fs_root, &path, false) {
                let mut output = Vec::new();
                for (name, node) in entries {
                    let type_char = match node {
                        FsNode::File(_) => b'F',
                        FsNode::Directory(_) => b'D',
                    };
                    output.push(type_char);
                    output.extend_from_slice(name.as_bytes());
                    output.push(0);
                }
                let copy_len = output.len().min(max_len);
                unsafe { core::ptr::copy_nonoverlapping(output.as_ptr(), out_ptr, copy_len); }
                copy_len as i32
            } else { -1 }
        }
        Syscall::GetCwd(out_ptr, max_len) => {
            if !validate_memory(pid, out_ptr as usize, max_len) { return -2; }
            let mut table = PROCESS_TABLE.lock();
            if let Some(p) = table.iter().find(|p| p.id == pid) {
                let cwd_bytes = p.cwd.as_bytes();
                let copy_len = cwd_bytes.len().min(max_len);
                unsafe { core::ptr::copy_nonoverlapping(cwd_bytes.as_ptr(), out_ptr, copy_len); }
                copy_len as i32
            } else { -1 }
        }
        Syscall::Wait(target_pid) => {
            let mut table = PROCESS_TABLE.lock();
            if let Some(target) = table.iter().find(|p| p.id == target_pid) {
                if target.state == ProcessState::Terminated {
                    0
                } else {
                    if let Some(caller) = table.iter_mut().find(|p| p.id == pid) {
                        caller.state = ProcessState::Blocked(BlockedReason::Wait(target_pid));
                    }
                    -3
                }
            } else { -1 }
        }
    }
}

pub fn validate_memory(_pid: u32, _ptr: usize, _len: usize) -> bool {
    // Trusted for now
    true
}
