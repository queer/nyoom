use std::path::Path;
use std::time::Instant;

use eyre::Result;

pub fn main() -> Result<()> {
    color_eyre::install()?;
    // argv[1] or "."
    let target_dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let ordered = nyoom::Walker::default().walk(Path::new(&target_dir), |_path, is_dir| is_dir)?;
    let mut buffer =
        String::with_capacity((ordered.total_path_sizes + ordered.paths.len() as u64) as usize);
    // let mut last_was_file = false;

    let out_start = Instant::now();
    for (path, _dir) in ordered.paths {
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
    eprintln!("output took: {:?}", out_start.elapsed());
    Ok(())
}
