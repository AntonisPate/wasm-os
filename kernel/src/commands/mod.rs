pub mod echo;

pub fn get_command(name: &str) -> Option<fn(usize, *const *const u8)> {
    match name {
        "echo" => Some(echo::echo_main),
        _ => None,
    }
}
