use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

pub enum FsNode {
    File(Vec<u8>),
    Directory(BTreeMap<String, FsNode>),
}

pub static RAM_FS: Mutex<FsNode> = Mutex::new(FsNode::Directory(BTreeMap::new()));

pub fn traverse_path<'a>(root: &'a mut FsNode, path: &str, create_dirs: bool) -> Option<&'a mut FsNode> {
    let mut current = root;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        if let FsNode::Directory(entries) = current {
            if !entries.contains_key(segment) {
                if create_dirs {
                    entries.insert(segment.to_string(), FsNode::Directory(BTreeMap::new()));
                } else {
                    return None;
                }
            }
            current = entries.get_mut(segment).unwrap();
        } else {
            return None; // Path component is a file, cannot traverse
        }
    }
    Some(current)
}
