use exec;
use notify::event::RemoveKind;
use notify::{Config, ErrorKind, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use std::{env::current_dir, path::Path};

const INDEX_LOCK_NAME: &'static str = "index.lock";
const GIT_DIR_NAME: &'static str = ".git";
const TIMEOUT_ENV_VAR: &'static str = "GIT_WAIT_TIMEOUT_MS";

fn main() {
    // Get current dir.
    let dir = current_dir()
        .unwrap_or_else(|_| report_error("Unable to read current directory.".to_string()));

    let mut args = std::env::args().collect::<Vec<_>>();
    args[0] = "git".to_string();

    // Find .git dir.
    if let Some(git_dir) = find_git_directory(&dir) {
        let timeout = if let Ok(timeout) = std::env::var(TIMEOUT_ENV_VAR) {
            let timeout = timeout.parse().unwrap_or_else(|e| {
                report_error(format!("timeout parse error: {}", e));
            });
            Some(Duration::from_millis(timeout))
        } else {
            None
        };

        let index_lock_path = git_dir.join(INDEX_LOCK_NAME);
        if index_lock_path.exists() {
            print!("Waiting on index.lock... ");
            stdout().flush().unwrap();
            wait(&index_lock_path, timeout);
            println!("done!");
            run_git_cmd(&args);
        } else {
            run_git_cmd(&args);
        }
    } else {
        run_git_cmd(&args);
    }
}

fn find_git_directory(dir: &Path) -> Option<PathBuf> {
    let mut p = dir.to_path_buf();
    loop {
        p.push(GIT_DIR_NAME);
        if p.exists() {
            return Some(p);
        }
        // Pop ".git" we just pushed.
        p.pop();

        // Pop current dir, return if already at the top-level.
        if !p.pop() {
            break None;
        }
    }
}

fn run_git_cmd(args: &[String]) {
    let err = exec::execvp("git", args);
    report_error(format!("{}", err));
}

fn wait(path: &Path, timeout: Option<Duration>) {
    let (tx, rx) = mpsc::channel::<Event>();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                tx.send(event).unwrap();
            }
        },
        Config::default(),
    )
    .unwrap_or_else(|e| {
        report_error(format!("Unable to initialize file watcher: {}", e));
    });

    if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
        match e.kind {
            ErrorKind::PathNotFound => {
                // index.lock no longer exists at this point.
                return;
            }
            _ => {
                report_error(format!("Unable to watch index.lock: {}", e));
            }
        }
    }

    loop {
        if let Some(timeout) = timeout {
            match rx.recv_timeout(timeout) {
                Ok(event) => {
                    if event.kind == EventKind::Remove(RemoveKind::File) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    report_error("timed out!".to_string());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    report_error("broken channel".to_string());
                }
            }
        } else {
            for event in &rx {
                if event.kind == EventKind::Remove(RemoveKind::File) {
                    return;
                }
            }
        }
    }
}

fn report_error(msg: String) -> ! {
    eprintln!("ERROR: {}", msg);
    std::process::exit(1)
}
