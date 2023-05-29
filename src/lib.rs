use std::path::PathBuf;

use dashmap::DashSet;
use floppy_disk::prelude::*;

pub async fn walk<'a, F: FloppyDisk<'a>, P: Into<PathBuf>>(
    disk: F,
    path: P,
) -> std::io::Result<DashSet<PathBuf>> {
    let path = path.into();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let out = DashSet::new();

    tx.send(path).unwrap();

    while let Ok(next) = rx.try_recv() {
        match disk.read_dir(&next).await {
            Ok(mut dir) => {
                while let Some(entry) = dir.next_entry().await? {
                    let path = entry.path();

                    if entry.file_type().await?.is_dir() {
                        tx.send(path.clone()).unwrap();
                    }
                    out.insert(path.clone());
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    continue;
                }
            }
        }
    }

    Ok(out)
}
