use std::path::Path;
use std::time::Instant;

use eyre::Result;
use floppy_disk::tokio_fs::TokioFloppyDisk;

#[tokio::main]
pub async fn main() -> Result<()> {
    color_eyre::install()?;
    let target_dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let ordered = nyoom::Walker::default()
        .walk(
            &TokioFloppyDisk::new(),
            Path::new(&target_dir),
            |_path, is_dir| is_dir,
        )
        .await?;
    let mut buffer =
        String::with_capacity((ordered.total_path_sizes + ordered.paths.len() as u64) as usize);

    let out_start = Instant::now();
    for (path, _dir) in ordered.paths {
        buffer.push_str(&path.display().to_string());
        buffer.push('\n');
    }
    println!("{}", buffer);
    eprintln!("output took: {:?}", out_start.elapsed());
    Ok(())
}
