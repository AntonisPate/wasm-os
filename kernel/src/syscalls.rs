use crate::process::{CURRENT_PROCESS, PROCESS_TABLE, ProcessState};
use crate::vfs;
use core::slice;
use alloc::vec::Vec;
use alloc::vec;

pub enum Syscall {
    Read(u32, *mut u8, usize),
    Write(u32, *const u8, usize),
    Exit(i32),
    Spawn(*const u8, usize, *const u8, usize),
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
        Syscall::Spawn(cmd_ptr, cmd_len, args_ptr, args_len) => {
            if !validate_memory(pid, cmd_ptr as usize, cmd_len) || 
               !validate_memory(pid, args_ptr as usize, args_len) {
                return -2;
            }

            let cmd_name = unsafe { core::str::from_utf8(slice::from_raw_parts(cmd_ptr, cmd_len)).unwrap_or("") };
            let args_data = unsafe { slice::from_raw_parts(args_ptr, args_len) };

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
                let new_pid = crate::kernel_spawn(0x200000, 0x10000, 7); // Symbolic memory for internal commands
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
