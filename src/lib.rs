use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossbeam::deque::{Injector, Stealer, Worker};
use crossbeam::thread;
use dashmap::DashMap;
use std::iter;

use eyre::Result;

pub fn walk<'a, F, O>(dir: &Path, walker: F) -> Result<DashMap<PathBuf, O>>
where
    F: Fn(PathBuf, bool) -> O + Send + Sync + 'a,
    O: Sized + Send + Sync + 'a,
{
    let path_injector = Injector::new();
    path_injector.push(dir.to_path_buf().as_os_str().into());
    let path_injector = Arc::new(path_injector);

    let out = thread::scope::<'a>(|scope| {
        let mut read_workers = vec![];
        let worker_count = num_cpus::get();
        let out = Arc::new(DashMap::new());
        let walker = Arc::new(walker);
        for _ in 0..worker_count {
            let path_injector = path_injector.clone();
            let out = out.clone();
            let walker = walker.clone();
            // let reader_queue = reader_queue.clone();
            let read_worker = scope.spawn(move |_| {
                do_walk(Worker::new_fifo(), path_injector, &[], walker, out).unwrap();
            });
            read_workers.push(read_worker);
        }

        let mut completed_workers = 0;
        for read_worker in read_workers {
            eprintln!(
                "awaiting read worker: {}",
                read_worker.thread().name().unwrap_or("<unknown>")
            );
            read_worker.join().unwrap();
            completed_workers += 1;
            eprintln!(
                "completed {}/{} read workers",
                completed_workers, worker_count
            );
        }

        out
    })
    .unwrap();

    match Arc::try_unwrap(out) {
        Ok(out) => Ok(out),
        Err(_) => unreachable!(),
    }
}

fn find_task<T>(
    local: &Worker<T>,
    global: &Arc<Injector<T>>,
    stealers: &[Stealer<T>],
) -> Option<T> {
    local.pop().or_else(|| {
        iter::repeat_with(|| {
            global
                .steal_batch_and_pop(local)
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        .find(|s| !s.is_retry())
        .and_then(|s| s.success())
    })
}

fn do_walk<'a, F, O>(
    local: Worker<OsString>,
    global: Arc<Injector<OsString>>,
    stealers: &[Stealer<OsString>],
    walker: Arc<F>,
    out: Arc<DashMap<PathBuf, O>>,
) -> Result<()>
where
    F: Fn(PathBuf, bool) -> O + Send + Sync + 'a,
    O: Sized + Send + Sync + 'a,
{
    // Potential wins:
    // - statx is slow, can we io_uring it or something?

    loop {
        while let Some(path) = find_task(&local, &global, stealers) {
            let is_dir = is_dir(&path)?;
            if is_dir {
                match fs::read_dir(&path) {
                    Ok(read) => {
                        for entry in read {
                            match entry {
                                Ok(entry) => {
                                    let file_name = entry.file_name();
                                    let mut next = path.clone();
                                    next.push(unsafe {
                                        OsStr::new(std::str::from_utf8_unchecked(&[
                                            std::path::MAIN_SEPARATOR as u8,
                                        ]))
                                    });
                                    next.push(file_name);
                                    global.push(next.clone());
                                }
                                Err(err) => {
                                    eprintln!("read_dir {:?}: {:?}", &path, err,);
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        if err.raw_os_error() == Some(libc::EACCES) {
                            continue;
                        }
                        panic!(
                            "read_dir error processing {:?}: {:?} (is_dir={})",
                            &path, err, is_dir
                        )
                    }
                }
            }

            let path = PathBuf::from(&path);
            let walk_res = walker(path.clone(), is_dir);
            out.insert(path, walk_res);
        }

        if global.is_empty() && local.is_empty() {
            break;
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn is_dir(path: &OsString) -> Result<bool> {
    // Slow path: Fall back to full stat when simple lstat isn't available.
    fs::symlink_metadata(path)
        .map(|stat| stat.is_dir())
        .map_err(|err| {
            if err.raw_os_error() == Some(libc::EACCES) {
                return err;
            }
            panic!("stat error processing {:?}: {:?}", &path, err)
        })
}

// Unscientific: It SEEMS slightly faster to do a partial lstat on Linux
#[cfg(target_os = "linux")]
fn is_dir(path: &OsString) -> Result<bool> {
    use nix::sys::stat::SFlag;

    match nix::sys::stat::lstat(path.as_os_str()) {
        Ok(stat) => Ok(stat.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFDIR.bits()),
        Err(err) => {
            if err == nix::errno::Errno::EACCES {
                eprintln!("EACCES: {:?}", path);
                return Ok(false);
            }
            if err == nix::errno::Errno::ENOENT {
                eprintln!("ENOENT: {:?}", path);
                return Ok(false);
            }
            panic!("lstat error processing {:?}: {:?}", &path, err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk() -> Result<()> {
        let out = walk(Path::new("./a"), move |_path, _is_dir| {})?;
        assert_eq!(69, out.len());
        Ok(())
    }
}
