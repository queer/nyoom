use std::path::Path;

use eyre::Result;

pub fn main() -> Result<()> {
    color_eyre::install()?;
    // argv[1] or "."
    let target_dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let stats = nyoom::walk(Path::new(&target_dir))?;
    let mut buffer = String::new();
    let mut last_was_file = false;
    for (path, stat) in stats {
        if stat.is_dir() {
            // if last_was_file {
            //     buffer.push('\n');
            // }
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
