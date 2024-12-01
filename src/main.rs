use errno::errno;
use libc::execvp;
use notify::event::RemoveKind;
use notify::{Config, ErrorKind, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::ffi::CString;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::{env::current_dir, path::Path, ptr};

const INDEX_LOCK_NAME: &'static str = "index.lock";
const GIT_DIR_NAME: &'static str = ".git";
const TIMEOUT_ENV_VAR: &'static str = "GIT_WAIT_TIMEOUT_MS";

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    match maybe_wait_for_index_lock(args) {
        Ok(args) => {
            if let Err(e) = run_git_cmd(&args) {
                eprintln!("ERROR: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("ERROR: {}", e);
            std::process::exit(1);
        }
    }
}

fn maybe_wait_for_index_lock(mut args: Vec<String>) -> Result<Vec<String>, String> {
    args[0] = "git".to_string();

    let mut dir = current_dir().map_err(|_| "unable to read current directory.".to_string())?;
    // Find .git dir.
    if traverse_to_git_dir(&mut dir) {
        let timeout = read_timeout_env_var()?;

        let index_lock_path = dir.join(INDEX_LOCK_NAME);
        if index_lock_path.exists() {
            print!("waiting on index.lock... ");
            stdout().flush().unwrap();
            wait(&index_lock_path, timeout)?;
            println!("done!");
            Ok(args)
        } else {
            Ok(args)
        }
    } else {
        // Run the git command anyway!
        Ok(args)
    }
}

fn read_timeout_env_var() -> Result<Option<Duration>, String> {
    if let Ok(timeout) = std::env::var(TIMEOUT_ENV_VAR) {
        let timeout = timeout
            .parse()
            .map_err(|e| format!("timeout parse error: {}", e))?;
        Ok(Some(Duration::from_millis(timeout)))
    } else {
        Ok(None)
    }
}

fn traverse_to_git_dir(dir: &mut PathBuf) -> bool {
    loop {
        dir.push(GIT_DIR_NAME);
        if dir.exists() {
            return true;
        }
        // Pop ".git" we just pushed.
        dir.pop();

        // Pop current dir, return if already at the top-level.
        if !dir.pop() {
            break false;
        }
    }
}

fn run_git_cmd(args: &[String]) -> Result<(), String> {
    // Unwrapping is fine here since the first arg is "git".
    let program_name = CString::new(args[0].as_bytes()).unwrap();

    // Convert args to vec of `CString`s.
    let args = args
        .into_iter()
        .map(|e| CString::new(e.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("invalid arg string: {}", e))?;

    // Convert args to `CString`s to vec of pointers.
    let mut arg_ptrs = args.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();
    arg_ptrs.push(ptr::null());

    // execvp only returns if there was an error.
    let result = unsafe { execvp(program_name.as_ptr(), arg_ptrs.as_ptr()) };
    if result == -1 {
        Err(format!("error executing git, code: {}", errno()))
    } else {
        Ok(())
    }
}

fn wait(path: &Path, timeout: Option<Duration>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Event>();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                tx.send(event).unwrap();
            }
        },
        Config::default(),
    )
    .map_err(|e| format!("unable to initialize file watcher: {}", e))?;

    if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
        return match e.kind {
            ErrorKind::PathNotFound => {
                // index.lock no longer exists at this point.
                Ok(())
            }
            _ => Err(format!("unable to watch index.lock: {}", e)),
        };
    }

    loop {
        if let Some(timeout) = timeout {
            match rx.recv_timeout(timeout) {
                Ok(event) => {
                    if event.kind == EventKind::Remove(RemoveKind::File) {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err("timed out!".to_string());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("broken channel".to_string());
                }
            }
        } else {
            for event in &rx {
                if event.kind == EventKind::Remove(RemoveKind::File) {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{maybe_wait_for_index_lock, traverse_to_git_dir};
    use lazy_static::lazy_static;
    use std::env::current_dir;
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::RecvTimeoutError;
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;
    use std::{env, fs};

    lazy_static! {
        // This global mutex is used to ensure access to the test dir is only every done
        // by one thread at a time for each test. Otherwise, multiple threads could try to
        // create/delete the same files and error out.
        static ref test_file_lock: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn repo_with_git_dir_is_valid() {
        with_test_dir(|test_dir| {
            fs::create_dir(&test_dir.path.join(".git")).unwrap();
            assert!(traverse_to_git_dir(&mut current_dir().unwrap()));
        });
    }

    #[test]
    fn repo_without_git_dir_is_invalid() {
        with_test_dir(|_| {
            assert!(!traverse_to_git_dir(&mut current_dir().unwrap()));
        });
    }

    #[test]
    fn wait_if_index_lock_is_present() {
        with_test_dir(|test_dir| {
            fs::create_dir(&test_dir.path.join(".git")).unwrap();

            // Create index.lock file.
            let index_file = test_dir.path.join(".git/index.lock");
            let _ = File::create(&index_file).unwrap();

            // Spawn a new thread to wait on the index.lock file and send the result in the
            // channel once done waiting.
            let (tx, rx) = mpsc::channel::<Result<Vec<String>, String>>();
            let handle = std::thread::spawn(move || {
                let result =
                    maybe_wait_for_index_lock(vec!["git".to_string(), "status".to_string()]);
                tx.send(result).unwrap();
            });

            // Wait 100ms. It should time out since index.lock file is still present.
            let result = rx.recv_timeout(Duration::from_millis(100));
            assert_eq!(result, Err(RecvTimeoutError::Timeout));

            fs::remove_file(index_file).unwrap();

            // Give enough time for the file to be deleted.
            let result = rx.recv_timeout(Duration::from_millis(200));
            assert!(result.unwrap().is_ok());

            handle.join().unwrap();
        });
    }

    fn with_test_dir(block: fn(&TestDir) -> ()) {
        let _lock = test_file_lock.lock().unwrap();
        let test_dir = TestDir::new();
        block(&test_dir);
    }

    struct TestDir {
        current_dir: PathBuf,
        path: Box<Path>,
    }

    impl TestDir {
        fn new() -> Self {
            let temp_dir = env::temp_dir().join("git-wait-test-dir");
            if temp_dir.exists() {
                fs::remove_dir_all(&temp_dir).unwrap();
            }

            fs::create_dir(&temp_dir).unwrap();

            let current_dir = env::current_dir().unwrap();
            env::set_current_dir(&temp_dir).unwrap();
            Self {
                path: temp_dir.into_boxed_path(),
                current_dir,
            }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            env::set_current_dir(&self.current_dir).unwrap();
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}
