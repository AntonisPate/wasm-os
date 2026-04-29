use crate::syscalls::{dispatch_syscall, Syscall};
use crate::vfs;
use core::slice;
use alloc::vec::Vec;

pub fn ls_main(argc: usize, argv: *const *const u8) {
    let args = unsafe { slice::from_raw_parts(argv, argc) };
    
    let mut show_tree = false;
    let mut force_root = false;
    let mut path_idx = 0;

    for i in 1..argc {
        let arg_ptr = args[i];
        let mut arg_len = 0;
        unsafe {
            while *arg_ptr.add(arg_len) != 0 {
                arg_len += 1;
            }
            let arg = core::str::from_utf8(slice::from_raw_parts(arg_ptr, arg_len)).unwrap_or("");
            if arg == "-t" {
                show_tree = true;
            } else if arg == "-ta" {
                show_tree = true;
                force_root = true;
            } else {
                path_idx = i;
            }
        }
    }

    let initial_path = if force_root {
        "/"
    } else if path_idx == 0 {
        "."
    } else {
        unsafe {
            let ptr = args[path_idx];
            let mut len = 0;
            while *ptr.add(len) != 0 { len += 1; }
            core::str::from_utf8(slice::from_raw_parts(ptr, len)).unwrap_or(".")
        }
    };

    list_recursive(initial_path, 0, show_tree);
    dispatch_syscall(Syscall::Exit(0));
}

fn list_recursive(path: &str, indent: usize, recursive: bool) {
    let mut out_buffer = [0u8; 1024];
    let res = dispatch_syscall(Syscall::ReadDir(path.as_ptr(), path.len(), out_buffer.as_mut_ptr(), out_buffer.len()));
    
    if res < 0 {
        if indent == 0 {
            let msg = alloc::format!("ls: Could not read directory '{}'\r\n", path);
            dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        }
        return;
    }

    let data = &out_buffer[..res as usize];
    let mut current = 0;
    while current < data.len() {
        let type_char = data[current] as char;
        current += 1;
        
        let start = current;
        while current < data.len() && data[current] != 0 {
            current += 1;
        }
        let name = core::str::from_utf8(&data[start..current]).unwrap_or("");
        current += 1; // skip null

        // Print with indentation
        for _ in 0..indent {
            dispatch_syscall(Syscall::Write(vfs::STDOUT, b"  ".as_ptr(), 2));
        }

        if type_char == 'D' {
            let msg = alloc::format!("{}/\r\n", name);
            dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
            
            if recursive {
                let mut sub_path = alloc::string::String::from(path);
                if !sub_path.ends_with('/') { sub_path.push('/'); }
                sub_path.push_str(name);
                list_recursive(&sub_path, indent + 1, recursive);
            }
        } else {
            let msg = alloc::format!("{}\r\n", name);
            dispatch_syscall(Syscall::Write(vfs::STDOUT, msg.as_ptr(), msg.len()));
        }
    }
}
