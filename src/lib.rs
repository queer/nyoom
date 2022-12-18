use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crossbeam::deque::{Injector, Stealer, Worker};
use crossbeam::queue::SegQueue;
use crossbeam::thread;
use crossbeam::utils::Backoff;
use std::iter;

use eyre::Result;

pub fn walk<'a, F, O>(dir: &Path, mut walker: F) -> Result<BTreeMap<OsString, O>>
where
    F: FnMut((OsString, bool)) -> O + Send + 'a,
    O: Sized + Send,
{
    let path_queue = Arc::new(SegQueue::new());
    let path_injector = Injector::new();
    path_injector.push(dir.to_path_buf().as_os_str().into());


    let path_injector = Arc::new(path_injector);
    let done = Arc::new(AtomicBool::new(false));

    let reader_queue = path_queue.clone();
    let reader_done = done.clone();

    let out = thread::scope(|scope| {
        let mut scoped_workers = vec![];
        for _ in 0..(num_cpus::get() - 1) {
            let path_injector = path_injector.clone();
            let path_queue = path_queue.clone();
            let scoped_worker = scope.spawn(move |_| {
                do_walk(Worker::new_fifo(), path_injector, &[], path_queue).unwrap();
            });
            scoped_workers.push(scoped_worker);
        }

        let reader_handle = scope.spawn(move |_| {
            let mut out = BTreeMap::new();
            let backoff = Backoff::new();
    
            loop {
                if let Some((path, dir)) = reader_queue.pop() {
                    backoff.reset();
                    out.insert(path.clone(), walker((path, dir)));
                } else {
                    backoff.spin();
                }
                if reader_done.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }

            out
        });

        for scoped_worker in scoped_workers {
            scoped_worker.join().unwrap();
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        let out: BTreeMap<OsString, O> = reader_handle.join().unwrap();
        out
    }).unwrap();

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
    path_queue: Arc<SegQueue<(OsString, bool)>>,
) -> Result<()> {
    // Potential wins:
    // - statx is slow, can we io_uring it or something?
    // - path manipulation involves a lot of allocations
    // TODO: Real error handling

    while let Some(path) = find_task(&local, &global, stealers) {
        let is_dir = is_dir(&path);
        if is_dir {
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

        path_queue.push((path, is_dir));
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn is_dir(path: &OsString) -> bool {
    // Slow path: Fall back to full stat when simple lstat isn't available.
    fs::symlink_metadata(path).map(|stat| stat.is_dir()).unwrap_or(false)
}

// Unscientific: It SEEMS slightly faster to do a partial lstat on Linux
#[cfg(target_os = "linux")]
fn is_dir(path: &OsString) -> bool {
    let mode = nix::sys::stat::lstat(path.as_os_str()).unwrap().st_mode;
    mode & libc::S_IFDIR == libc::S_IFDIR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk() -> Result<()> {
        let queue = Arc::new(SegQueue::new());
        let walker_queue = queue.clone();
        walk(Path::new("./a"), move |path| {
            walker_queue.push(path);
        })?;
        assert_eq!(69, queue.len());
        Ok(())
    }
}
