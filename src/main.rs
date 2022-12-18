use std::path::Path;

use eyre::Result;

pub fn main() -> Result<()> {
    color_eyre::install()?;
    nyoom::walk(Path::new("/usr"))?;
    Ok(())
}
