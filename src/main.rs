use std::path::Path;

use eyre::Result;

pub fn main() -> Result<()> {
    color_eyre::install()?;
    // argv[1] or "."
    let target_dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let ordered = nyoom::walk(Path::new(&target_dir), |_path, is_dir| is_dir)?;
    let mut buffer = String::new();
    // let mut last_was_file = false;

    for (path, _dir) in ordered {
        // if dir {
        //     if last_was_file {
        //         buffer.push('\n');
        //     }
        //     buffer.push_str(&format!("{}:\n", path.display()));
        //     last_was_file = false;
        // } else {
        //     last_was_file = true;
        //     buffer.push_str(&format!("{}\n", path.display()));
        // }
        buffer.push_str(&path.display().to_string());
        buffer.push('\n');
    }
    println!("{}", buffer);
    Ok(())
}
