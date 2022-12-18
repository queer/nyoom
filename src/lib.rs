use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use eyre::Result;

pub fn walk(dir: &Path) -> Result<Vec<Metadata>> {
    let (path_tx, path_rx) = std::sync::mpsc::channel();
    let (stat_tx, stat_rx) = std::sync::mpsc::channel();

    path_tx.send(dir.into())?;
    do_walk(path_tx, path_rx, stat_tx)?;

    let mut out = vec![];
    while let Ok(stat) = stat_rx.recv() {
        out.push(stat);
    }

    Ok(out)
}

fn do_walk(
    path_tx: Sender<PathBuf>,
    path_rx: Receiver<PathBuf>,
    stat_tx: Sender<Metadata>,
) -> Result<()> {
    loop {
        match path_rx.try_recv() {
            Ok(path) => {
                let stat = fs::metadata(&path)?;
                if stat.is_dir() {
                    let read = fs::read_dir(&path)?;
                    for entry in read {
                        path_tx.send(entry?.path())?;
                    }
                }
                stat_tx.send(stat)?;
            }
            Err(TryRecvError::Empty) => break,
            Err(_) => break,
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk() -> Result<()> {
        let out = walk(Path::new("./a"))?;
        assert_eq!(69, out.len());
        Ok(())
    }
}
