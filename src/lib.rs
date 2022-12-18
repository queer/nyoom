use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crossbeam::deque::{Injector, Stealer, Worker};
use std::iter;

use eyre::Result;

pub fn walk(dir: &Path) -> Result<Vec<Metadata>> {
    let (stat_tx, stat_rx) = std::sync::mpsc::channel();
    let path_injector = Injector::new();

    path_injector.push(dir.to_path_buf().as_os_str().into());
    let path_injector = Arc::new(path_injector);
    let mut workers = Vec::with_capacity(num_cpus::get());
    for _ in 0..num_cpus::get() {
        let stat_tx = stat_tx.clone();
        let path_injector = path_injector.clone();
        let worker = std::thread::spawn(move || {
            do_walk(Worker::new_fifo(), path_injector, &[], stat_tx).unwrap();
        });
        workers.push(worker);
    }

    for worker in workers {
        match worker.join() {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Worker thread panicked: {:?}", err);
            }
        }
    }

    let mut out = vec![];
    while let Ok(stat) = stat_rx.try_recv() {
        out.push(stat);
    }

    Ok(out)
}

fn find_task<T>(
    local: &Worker<T>,
    global: &Arc<Injector<T>>,
    stealers: &[Stealer<T>],
) -> Option<T> {
    // Pop a task from the local queue, if not empty.
    local.pop().or_else(|| {
        // Otherwise, we need to look for a task elsewhere.
        iter::repeat_with(|| {
            // Try stealing a batch of tasks from the global queue.
            global
                .steal_batch_and_pop(local)
                // Or try stealing a task from one of the other threads.
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        // Loop while no task was stolen and any steal operation needs to be retried.
        .find(|s| !s.is_retry())
        // Extract the stolen task, if there is one.
        .and_then(|s| s.success())
    })
}

fn do_walk(
    local: Worker<OsString>,
    global: Arc<Injector<OsString>>,
    stealers: &[Stealer<OsString>],
    stat_tx: Sender<Metadata>,
) -> Result<()> {
    // Potential wins:
    // - statx is slow, can we io_uring it or something?
    // - path manipulation involves a lot of allocations
    // TODO: Real error handling

    while let Some(path) = find_task(&local, &global, stealers) {
        match fs::symlink_metadata(&path) {
            Ok(stat) => {
                if stat.is_dir() {
                    match fs::read_dir(&path) {
                        Ok(read) => {
                            for entry in read {
                                let file_name = entry?.file_name();
                                let mut next = path.clone();
                                next.push(unsafe {
                                    OsStr::new(std::str::from_utf8_unchecked(&[
                                        std::path::MAIN_SEPARATOR as u8,
                                    ]))
                                });
                                next.push(file_name);
                                global.push(next);
                            }
                        }
                        Err(err) => {
                            if err.raw_os_error() == Some(libc::EACCES) {
                                continue;
                            }
                            panic!("read_dir error processing {:?}: {:?}", &path, err)
                        }
                    }
                }
                stat_tx.send(stat)?;
            }
            Err(err) => {
                panic!("stat error processing {:?}: {:?}", &path, err)
            }
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
