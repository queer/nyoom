use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossbeam::queue::SegQueue;

use eyre::Result;

pub fn main() -> Result<()> {
    color_eyre::install()?;
    // argv[1] or "."
    let target_dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let queue = Arc::new(SegQueue::new());
    let walker_queue = queue.clone();
    nyoom::walk(Path::new(&target_dir), |path| {
        walker_queue.push(path);
    })?;
    let mut buffer = String::new();
    let mut last_was_file = false;
    let mut ordered = BTreeMap::new();
    while let Some((path, dir)) = queue.pop() {
        ordered.insert(PathBuf::from(path), dir);
    }

    for (path, dir) in ordered {
        if dir {
            if last_was_file {
                buffer.push('\n');
            }
            buffer.push_str(&format!("{}:\n", path.display()));
            last_was_file = false;
        } else {
            last_was_file = true;
            buffer.push_str(&format!("{}\n", path.display()));
        }
    }
    println!("{}", buffer);
    Ok(())
}
