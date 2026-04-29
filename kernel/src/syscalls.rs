use crate::fs::{RAM_FS, traverse_path, FsNode};
use crate::vfs;
use crate::process::{CURRENT_PROCESS, PROCESS_TABLE, ProcessState, FileType};
use core::slice;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::ToString;
use alloc::collections::BTreeMap;

pub enum Syscall {
    Read(u32, *mut u8, usize),
    Write(u32, *const u8, usize),
    Exit(i32),
    Spawn(*const u8, usize, *const u8, usize, u32),
    Open(*const u8, usize, u32),
    Close(u32),
    Mkdir(*const u8, usize),
    Unlink(*const u8, usize, bool),
    Chdir(*const u8, usize),
    ReadDir(*const u8, usize, *mut u8, usize),
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
        Syscall::Spawn(cmd_ptr, cmd_len, args_ptr, args_len, stdout_fd) => {
            if !validate_memory(pid, cmd_ptr as usize, cmd_len) || 
               !validate_memory(pid, args_ptr as usize, args_len) {
                return -2;
            }

            let cmd_name = unsafe { core::str::from_utf8(slice::from_raw_parts(cmd_ptr, cmd_len)).unwrap_or("") };
            let args_data = unsafe { slice::from_raw_parts(args_ptr, args_len) };

            // Get stdout_type and cwd from the current process
            let (stdout_type, cwd) = {
                let table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter().find(|p| p.id == pid) {
                    let fd_type = if stdout_fd < 8 {
                        p.file_descriptors[stdout_fd as usize].clone()
                    } else {
                        None
                    };
                    (fd_type, p.cwd.clone())
                } else {
                    (None, "/".to_string())
                }
            };

            if let Some(entry) = crate::commands::get_command(cmd_name) {
                // 1. Tokenize null-separated arguments from the shell buffer
                let mut arg_storage_vec = Vec::new();
                let mut current_start = 0;
                for i in 0..args_len {
                    if args_data[i] == 0 {
                        let arg = &args_data[current_start..i];
                        if !arg.is_empty() {
                            // Deep copy into kernel-owned storage
                            let mut arg_copy = vec![0u8; arg.len() + 1];
                            arg_copy[..arg.len()].copy_from_slice(arg);
                            arg_copy[arg.len()] = 0; // Ensure null-termination
                            arg_storage_vec.push(arg_copy);
                        }
                        current_start = i + 1;
                    }
                }

                // Handle the last argument if there's no trailing null byte
                if current_start < args_len {
                    let arg = &args_data[current_start..args_len];
                    if !arg.is_empty() {
                        let mut arg_copy = vec![0u8; arg.len() + 1];
                        arg_copy[..arg.len()].copy_from_slice(arg);
                        arg_copy[arg.len()] = 0;
                        arg_storage_vec.push(arg_copy);
                    }
                }
                
                // 2. Prepare final argv storage (argv[0] is command name)
                let mut final_args = Vec::new();
                let mut cmd_copy = vec![0u8; cmd_name.len() + 1];
                cmd_copy[..cmd_name.len()].copy_from_slice(cmd_name.as_bytes());
                cmd_copy[cmd_name.len()] = 0;
                
                final_args.push(cmd_copy);
                final_args.extend(arg_storage_vec);

                let argc = final_args.len();
                let mut arg_ptrs = Vec::with_capacity(argc);
                for arg in &final_args {
                    arg_ptrs.push(arg.as_ptr());
                }
                let argv = arg_ptrs.as_ptr();

                // 3. Spawn process and assign storage
                let new_pid = crate::kernel_spawn(0x200000, 0x10000, 7, stdout_type, cwd);
                let mut table = PROCESS_TABLE.lock();
                if let Some(p) = table.iter_mut().find(|p| p.id == new_pid) {
                    p.entry_point = Some(entry);
                    p.argc = argc;
                    p.argv = argv;
                    p.arg_storage = Some((final_args, arg_ptrs));
                    p.state = ProcessState::Ready;
                }
                new_pid as i32
            } else {
                -1 // Command not found
            }
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
            -1 // Out of FDs
        }
        Syscall::Close(fd) => {
            if fd >= 8 { return -9; }
            let mut table = PROCESS_TABLE.lock();
            if let Some(p) = table.iter_mut().find(|p| p.id == pid) {
                p.file_descriptors[fd as usize] = None;
            }
            0
        }
        Syscall::Mkdir(path_ptr, path_len) => {
            if !validate_memory(pid, path_ptr as usize, path_len) { return -2; }
            let raw_path = unsafe { core::str::from_utf8(slice::from_raw_parts(path_ptr, path_len)).unwrap_or("") };
            let path = vfs::resolve_path(pid, raw_path);
            
            let mut fs_root = RAM_FS.lock();
            let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(filename) = parts.pop() {
                let parent_path = parts.join("/");
                if let Some(FsNode::Directory(entries)) = traverse_path(&mut *fs_root, &parent_path, false) {
                    if !entries.contains_key(filename) {
                        entries.insert(filename.to_string(), FsNode::Directory(BTreeMap::new()));
                        0
                    } else { -1 } // Already exists
                } else { -1 } // Parent not found or not a directory
            } else { -1 } // Cannot mkdir /
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
                     if let Some(node) = entries.get(filename) {
                         match node {
                             FsNode::Directory(sub_entries) if !recursive => {
                                 if sub_entries.is_empty() {
                                     entries.remove(filename);
                                     0
                                 } else { -1 } // Directory not empty
                             },
                             _ => {
                                 entries.remove(filename);
                                 0
                             }
                         }
                     } else { -1 }
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
                unsafe {
                    core::ptr::copy_nonoverlapping(output.as_ptr(), out_ptr, copy_len);
                }
                copy_len as i32
            } else { -1 }
        }
    }
}

pub fn validate_memory(pid: u32, _ptr: usize, _len: usize) -> bool {
    // For now, all processes are internal and trusted.
    // In a real system, we would check bounds against the process's memory region.
    if pid >= 1 {
        return true;
    }

    let table = PROCESS_TABLE.lock();
    if let Some(proc) = table.iter().find(|p| p.id == pid) {
        let in_bounds = _ptr >= proc.memory_start && (_ptr + _len) <= (proc.memory_start + proc.size);
        return in_bounds;
    }
    false
}
