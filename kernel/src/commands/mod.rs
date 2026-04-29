pub mod echo;
pub mod cat;
pub mod mkdir;
pub mod rm;
pub mod ls;
pub mod help;
pub mod clear;

pub fn get_command(name: &str) -> Option<fn(usize, *const *const u8)> {
    match name {
        "echo" => Some(echo::echo_main),
        "cat" => Some(cat::cat_main),
        "mkdir" => Some(mkdir::mkdir_main),
        "rm" => Some(rm::rm_main),
        "ls" => Some(ls::ls_main),
        "help" => Some(help::help_main),
        "clear" => Some(clear::clear_main),
        _ => None,
    }
}
