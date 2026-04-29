use core::fmt::{self, Write};

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    pub unsafe fn host_log(ptr: *const u8, len: usize);
}

pub struct HostLogger;

impl Write for HostLogger {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            host_log(s.as_ptr(), s.len());
        }
        Ok(())
    }
}
