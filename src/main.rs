use std::time::Instant;

use eyre::Result;
use floppy_disk::tokio_fs::TokioFloppyDisk;

#[tokio::main]
pub async fn main() -> Result<()> {
    color_eyre::install()?;
    let target_dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let results = nyoom::walk(&TokioFloppyDisk::new(), target_dir).await?;

    let mut buffer = String::new();
    let out_start = Instant::now();
    for path in results {
        buffer.push_str(&path.display().to_string());
        buffer.push('\n');
    }
    println!("{}", buffer);
    eprintln!("output took: {:?}", out_start.elapsed());
    Ok(())
}
