use std::ffi::CString;

use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, execvp, fork, setpgid, setsid};

use crate::logger::CesarLogger;

/// Quote-aware tokenizer for `Exec` lines: splits on whitespace but keeps
/// single- or double-quoted segments (and embedded spaces) as one word.
pub fn tokenize_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has_word = false;
    for c in exec.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                    has_word = true;
                } else if c.is_whitespace() {
                    if has_word {
                        words.push(std::mem::take(&mut cur));
                        has_word = false;
                    }
                } else {
                    cur.push(c);
                    has_word = true;
                }
            }
        }
    }
    if has_word {
        words.push(cur);
    }
    words
}

pub fn spawn_service_env(
    name: &str,
    exec_path: &str,
    env_vars: &[(String, String)],
    work_dir: Option<&str>,
    logger: &CesarLogger,
) -> Result<u32, String> {
    if exec_path.contains('\0') {
        let msg = format!(
            "Exec line for '{}' contains NUL and cannot be executed",
            name
        );
        logger.log_error(name, &msg);
        return Err(msg);
    }
    // execvp(3) resolves bare names via $PATH, so a missing file check on
    // argv[0] here would reject perfectly runnable commands.
    let parts = tokenize_exec(exec_path);
    if parts.is_empty() {
        let msg = format!("Exec path '{}' is empty", exec_path);
        logger.log_error(name, &msg);
        return Err(msg);
    }

    // Pre-compute every CString/env block BEFORE forking: allocations,
    // locks and error paths in the child are exactly what we must avoid.
    let c_exec = CString::new(parts[0].as_str()).map_err(|_| {
        let msg = format!("Invalid exec path for service '{}'", name);
        logger.log_error(name, &msg);
        msg
    })?;
    let c_args: Vec<CString> = parts
        .iter()
        .map(|p| CString::new(p.as_str()))
        .collect::<Result<_, _>>()
        .map_err(|_| {
            let msg = format!("Exec arguments for '{}' contain NUL", name);
            logger.log_error(name, &msg);
            msg
        })?;
    let mut c_env: Vec<(CString, CString)> = Vec::with_capacity(env_vars.len());
    for (k, v) in env_vars {
        match (CString::new(k.as_str()), CString::new(v.as_str())) {
            (Ok(ck), Ok(cv)) => c_env.push((ck, cv)),
            _ => {
                let msg = format!("Environment value for '{}' contains NUL", name);
                logger.log_error(name, &msg);
                return Err(msg);
            }
        }
    }
    let c_work_dir = match work_dir {
        Some(dir) => match CString::new(dir) {
            Ok(c) => Some(c),
            Err(_) => {
                let msg = format!("WorkingDirectory for '{}' contains NUL", name);
                logger.log_error(name, &msg);
                return Err(msg);
            }
        },
        None => None,
    };

    // Snapshot listening sockets in the parent (locking in the forked child
    // would risk deadlock against other threads holding the registry).
    let listen_fds = crate::socket::listen_fds_for(name);

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            logger.log_info(name, &format!("Spawned PID {}", child));
            Ok(child.as_raw() as u32)
        }
        Ok(ForkResult::Child) => {
            let _ = setsid();
            let _ = setpgid(Pid::from_raw(0), Pid::from_raw(0));

            unsafe {
                libc::umask(0o022);
            }

            log_to_devnull();

            // Socket activation: hand listening fds to the service on
            // fds 3..3+n and export sd_listen_fds-style environment vars.
            // dup2 always clears FD_CLOEXEC on the new descriptor.
            if !listen_fds.is_empty() {
                unsafe {
                    for (i, fd) in listen_fds.iter().enumerate() {
                        libc::dup2(*fd, 3 + i as i32);
                    }
                    let n_str =
                        std::ffi::CString::new(listen_fds.len().to_string()).expect("LISTEN_FDS");
                    let pid_str =
                        std::ffi::CString::new(libc::getpid().to_string()).expect("LISTEN_PID");
                    libc::setenv(c"LISTEN_FDS".as_ptr(), n_str.as_ptr(), 1);
                    libc::setenv(c"LISTEN_PID".as_ptr(), pid_str.as_ptr(), 1);
                }
            }
            if let Some(ref dir) = c_work_dir {
                unsafe {
                    libc::chdir(dir.as_ptr());
                }
            }

            for (ck, cv) in &c_env {
                unsafe {
                    libc::setenv(ck.as_ptr(), cv.as_ptr(), 1);
                }
            }

            let arg_refs: Vec<&CString> = c_args.iter().collect();
            let _ = execvp(&c_exec, &arg_refs);

            // exec failed: raw _exit — no atexit handlers may run in this
            // forked clone of the init process.
            unsafe {
                libc::_exit(127);
            }
        }
        Err(e) => {
            let msg = format!("Fork failed for '{}': {}", name, e);
            logger.log_error(name, &msg);
            Err(msg)
        }
    }
}

fn log_to_devnull() {
    unsafe {
        let devnull = CString::new("/dev/null").expect("NUL-free /dev/null path");
        let fd = libc::open(devnull.as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::dup2(fd, libc::STDIN_FILENO);
            libc::dup2(fd, libc::STDOUT_FILENO);
            libc::dup2(fd, libc::STDERR_FILENO);
            libc::close(fd);
        }
    }
}

pub fn check_process(pid: u32) -> ProcessStatus {
    // A zombie still answers kill(pid, 0); consult /proc state so
    // unreaped-but-dead children are classified as dead.
    if let Some(state) = proc_state(pid) {
        return if state == 'Z' {
            ProcessStatus::Unknown
        } else {
            ProcessStatus::Alive
        };
    }
    // /proc entry unreadable/missing: fall back to the signal probe.
    unsafe {
        if libc::kill(pid as i32, 0) == 0 {
            ProcessStatus::Alive
        } else {
            match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::EPERM) => ProcessStatus::Alive,
                _ => ProcessStatus::Unknown,
            }
        }
    }
}

fn proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    parse_proc_state(&stat)
}

/// Extract the state char from /proc/<pid>/stat text. The comm field may
/// contain spaces/parens, so scan from the LAST ')' before the state field.
fn parse_proc_state(stat: &str) -> Option<char> {
    let after_comm = stat.rsplit_once(')').map(|(_, rest)| rest)?;
    after_comm.split_whitespace().next()?.chars().next()
}

pub fn kill_service(pid: u32, signal: i32) -> Result<(), String> {
    unsafe {
        if libc::kill(pid as i32, signal) == 0 {
            Ok(())
        } else {
            Err(format!("Failed to send signal {} to PID {}", signal, pid))
        }
    }
}

pub fn kill_service_group(pid: u32, signal: i32) -> Result<(), String> {
    unsafe {
        let pgid = libc::getpgid(pid as i32);
        if pgid >= 0 {
            if libc::kill(-pgid, signal) == 0 {
                Ok(())
            } else {
                Err(format!(
                    "Failed to send signal {} to process group {}",
                    signal, pgid
                ))
            }
        } else {
            Err(format!("Failed to get process group for PID {}", pid))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Alive,
    Exited,
    Failed(i32),
    Signaled(i32),
    Stopped,
    Unknown,
}

/// Reap any available children of init.
///
/// Returns `(service_name, status)` pairs; `service_name` is `None` when the
/// reaped pid was an orphaned/untracked process reparented to init — which is
/// normal PID 1 behavior, not a service failure.
pub fn reap_zombies(children: &[(String, u32)]) -> Vec<(Option<String>, ProcessStatus)> {
    let mut reaped = Vec::new();
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => break,
            Ok(WaitStatus::Exited(pid, status)) => {
                let name = children
                    .iter()
                    .find(|(_, p)| *p == pid.as_raw() as u32)
                    .map(|(n, _)| n.clone());
                if status == 0 {
                    reaped.push((name, ProcessStatus::Exited));
                } else {
                    reaped.push((name, ProcessStatus::Failed(status)));
                }
            }
            Ok(WaitStatus::Signaled(pid, signal, _)) => {
                let name = children
                    .iter()
                    .find(|(_, p)| *p == pid.as_raw() as u32)
                    .map(|(n, _)| n.clone());
                reaped.push((name, ProcessStatus::Signaled(signal as i32)));
            }
            _ => break,
        }
    }
    reaped
}

/// Poll until `pid` stops existing (or turns zombie) or `timeout` elapses.
pub fn wait_for_death(pid: u32, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if check_process(pid) != ProcessStatus::Alive {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// SIGTERM the process group of `pid`, wait up to `grace`, then SIGKILL
/// any leftovers. Shared by the IPC and CLI stop/restart paths.
pub fn terminate_group(pid: u32, grace: std::time::Duration) -> Result<(), String> {
    kill_service_group(pid, libc::SIGTERM)?;
    if wait_for_death(pid, grace) {
        return Ok(());
    }
    kill_service_group(pid, libc::SIGKILL).ok();
    if wait_for_death(pid, std::time::Duration::from_secs(2)) {
        Ok(())
    } else {
        Err(format!("process group of PID {} refused to die", pid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_state_zombie_is_classified_dead() {
        // 1234 (cat) S 1 ... fields before the ')' may contain spaces/parens.
        let stat = "1234 (cat) Z 1 1234 1234 0 -1 4194560 ...";
        assert_eq!(parse_proc_state(stat), Some('Z'));
    }

    #[test]
    fn proc_state_handles_spaces_and_parens_in_comm() {
        let stat = "42 (systemd-u) S 1 42 42 0 -1 ...";
        assert_eq!(parse_proc_state(stat), Some('S'));
        let tricky = "7 (bad) name (x)) R 1 7 7 0 -1 ...";
        assert_eq!(parse_proc_state(tricky), Some('R'));
    }

    #[test]
    fn proc_state_missing_or_malformed_stat() {
        assert_eq!(parse_proc_state("garbage without parens"), None);
        assert_eq!(parse_proc_state(""), None);
        assert_eq!(parse_proc_state("12 () I 0 12 12 0 -1"), Some('I'));
    }

    #[test]
    fn check_process_reports_zombie_as_not_alive() {
        // Our own forked child that exits immediately becomes a zombie until
        // reaped; it must not be reported Alive.
        match unsafe { fork() } {
            Ok(ForkResult::Child) => unsafe {
                libc::_exit(0);
            },
            Ok(ForkResult::Parent { child }) => {
                let pid = child.as_raw() as u32;
                // Spin briefly until the child is a zombie (or already reaped
                // by some handler), then classify it.
                for _ in 0..100 {
                    if parse_proc_state(
                        &std::fs::read_to_string(format!("/proc/{}/stat", pid)).unwrap_or_default(),
                    ) == Some('Z')
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert_ne!(check_process(pid), ProcessStatus::Alive);
                let _ = waitpid(child, None);
            }
            Err(e) => panic!("fork failed: {}", e),
        }
    }

    #[test]
    fn tokenize_exec_respects_quotes() {
        assert_eq!(
            tokenize_exec(r#"nginx -g "daemon off;" -c /etc/nginx/nginx.conf"#),
            vec!["nginx", "-g", "daemon off;", "-c", "/etc/nginx/nginx.conf"]
        );
        assert_eq!(
            tokenize_exec("sh -c 'echo hello world'"),
            vec!["sh", "-c", "echo hello world"]
        );
        assert_eq!(tokenize_exec("  spaced   out  "), vec!["spaced", "out"]);
        assert_eq!(
            tokenize_exec("\"\""),
            vec![""],
            "empty quoted word stays a real argv item"
        );
        assert_eq!(
            tokenize_exec(r#"env "A=B C" tail"#),
            vec!["env", "A=B C", "tail"]
        );
        assert!(tokenize_exec("").is_empty());
    }
}
