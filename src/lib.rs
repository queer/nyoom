use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossbeam::deque::{Injector, Stealer, Worker};
use crossbeam::thread;
use dashmap::DashMap;
use std::iter;

use eyre::Result;

/// The results of traversing a directory tree. Contains a map of paths to the
/// result of the walker function, and the total size of all paths. The latter
/// is useful for preallocating a buffer for the output.
pub struct WalkResults<O: Sized + Sync + Send + Copy> {
    /// All paths visited during directory tree-walking.
    pub paths: DashMap<PathBuf, O>,
    /// The total size of all paths visited during directory tree-walking.
    pub total_path_sizes: u64,
}

impl<O: Sized + Send + Sync + Copy> WalkResults<O> {
    pub fn paths_ordered(&self) -> BTreeMap<PathBuf, O> {
        let mut out = BTreeMap::new();

        for part in self.paths.iter() {
            out.insert(part.key().clone(), *part.value());
        }

        out
    }
}

pub struct Walker {
    num_threads: usize,
}

impl Default for Walker {
    fn default() -> Self {
        Self::new(num_cpus::get())
    }
}

impl Walker {
    pub fn new(num_threads: usize) -> Self {
        Self { num_threads }
    }

    /// Walk a directory tree, calling the walker function on each path. Results
    /// **ARE NOT ORDERED.**
    ///
    /// The walking process is as follows:
    /// - Take in a path to walk from
    /// - Push it into the walk queue
    /// - Spawn `numcpu` worker threads
    /// - Each worker thread:
    ///   - Pops a path from the queue, attempting to steal from other worker
    ///     threads when possible
    ///   - If the path is a directory, push all its children into the queue
    ///   - Call the walker function on the path
    ///   - Push the result into the output map
    ///   - Track total path sizes
    /// - Join all worker threads
    ///
    /// The work-stealing queue is implemented on top of
    /// `crossbeam::deque::Injector`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::path::Path;
    /// use nyoom::Walker;
    ///
    /// let walker = Walker::default();
    /// let results = walker.walk(Path::new("."), |path, is_dir| {
    ///    if is_dir {
    ///       format!("{}:", path.display());
    ///    } else {
    ///       format!("{}", path.display());
    ///    }
    ///    is_dir
    /// }).unwrap();
    ///
    /// assert!(results.paths.len() > 0);
    /// ```
    pub fn walk<'a, F, O>(&self, dir: &Path, walker: F) -> Result<WalkResults<O>>
    where
        F: Fn(PathBuf, bool) -> O + Send + Sync + 'a,
        O: Sized + Send + Sync + Copy + 'a,
    {
        let path_injector = Arc::new(Injector::new());
        path_injector.push(dir.to_path_buf().as_os_str().into());

        let (out, path_sizes) = thread::scope::<'a>(|scope| {
            let mut read_workers = vec![];
            let worker_count = self.num_threads;
            let out = Arc::new(DashMap::new());
            let walker = Arc::new(walker);
            for _ in 0..worker_count {
                let path_injector = path_injector.clone();
                let out = out.clone();
                let walker = walker.clone();
                // let reader_queue = reader_queue.clone();
                let read_worker = scope.spawn(move |_| {
                    do_walk(Worker::new_fifo(), path_injector, &[], walker, out).unwrap()
                });
                read_workers.push(read_worker);
            }

            let mut path_sizes = 0;
            for read_worker in read_workers {
                path_sizes += read_worker.join().unwrap();
            }

            (out, path_sizes)
        })
        .unwrap();

        match Arc::try_unwrap(out) {
            Ok(out) => Ok(WalkResults {
                paths: out,
                total_path_sizes: path_sizes,
            }),
            Err(_) => unreachable!(),
        }
    }
}

fn find_task<T>(
    local: &Worker<T>,
    global: &Arc<Injector<T>>,
    stealers: &[Stealer<T>],
) -> Option<T> {
    // Find a task to steal from the global queue if none are available locally
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
) -> Result<u64>
where
    F: Fn(PathBuf, bool) -> O + Send + Sync + 'a,
    O: Sized + Send + Sync + 'a,
{
    let mut path_sizes = 0;
    loop {
        // If a task is available, process it.
        while let Some(path) = find_task(&local, &global, stealers) {
            // If the currently-processed path is a directory, it needs special
            // processing. On Linux this is just an lstat call.
            let is_dir = is_dir(&path)?;

            // If the path is a directory, push all its children into the queue.
            if is_dir {
                match fs::read_dir(&path) {
                    Ok(read) => {
                        for entry in read {
                            match entry {
                                Ok(entry) => {
                                    let file_name = entry.file_name();
                                    let mut next = path.clone();
                                    // Safety: we know that the path separator
                                    // is a valid UTF-8 character.
                                    next.push(unsafe {
                                        OsStr::new(std::str::from_utf8_unchecked(&[
                                            std::path::MAIN_SEPARATOR as u8,
                                        ]))
                                    });
                                    next.push(file_name);
                                    global.push(next.clone());
                                }
                                Err(err) => {
                                    eprintln!("read_dir error @ {:?}: {:?}", &path, err,);
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        if err.raw_os_error() == Some(libc::EACCES) {
                            // eprintln!("EACCES: {:?}", path);
                            continue;
                        }
                        panic!(
                            "read_dir error processing {:?}: {:?} (is_dir={})",
                            &path, err, is_dir
                        )
                    }
                }
            }

            // Call the walker function on the path and store the result.
            let path = PathBuf::from(&path);
            let walk_res = walker(path.clone(), is_dir);
            path_sizes += path.as_os_str().len() as u64;
            out.insert(path, walk_res);
        }

        // If we've run out of tasks, we're done! :D
        if global.is_empty() && local.is_empty() {
            break;
        }
    }

    Ok(path_sizes)
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
                // eprintln!("EACCES: {:?}", path);
                return Ok(false);
            }
            if err == nix::errno::Errno::ENOENT {
                // eprintln!("ENOENT: {:?}", path);
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
        let out = Walker::default().walk(Path::new("./a"), move |_path, _is_dir| {})?;
        assert_eq!(69, out.paths.len());
        Ok(())
    }

    #[test]
    fn test_walk_ordered() -> Result<()> {
        let out = Walker::default().walk(Path::new("./a"), move |_path, _is_dir| {})?;
        assert_eq!(69, out.paths_ordered().len());
        Ok(())
    }
}
