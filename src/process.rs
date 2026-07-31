use std::ffi::CString;
use std::path::Path;

use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{execvp, fork, ForkResult, Pid, setpgid, setsid};

use crate::logger::CesarLogger;

pub fn spawn_service(
    name: &str,
    exec_path: &str,
    logger: &CesarLogger,
) -> Result<u32, String> {
    spawn_service_env(name, exec_path, &[], None, logger)
}

pub fn spawn_service_env(
    name: &str,
    exec_path: &str,
    env_vars: &[(String, String)],
    work_dir: Option<&str>,
    logger: &CesarLogger,
) -> Result<u32, String> {
    let parts: Vec<&str> = exec_path.split_whitespace().collect();
    if parts.is_empty() {
        let msg = format!("Exec path '{}' is empty", exec_path);
        logger.log_error(name, &msg);
        return Err(msg);
    }

    let exec_bin = parts[0];
    if !Path::new(exec_bin).exists() {
        let msg = format!("Exec binary '{}' not found!", exec_bin);
        logger.log_error(name, &msg);
        return Err(msg);
    }

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

            if let Some(dir) = work_dir
                && let Ok(c_dir) = CString::new(dir) {
                    unsafe { libc::chdir(c_dir.as_ptr()); }
                }

            for (k, v) in env_vars {
                if let Ok(c_key) = CString::new(k.as_str())
                    && let Ok(c_value) = CString::new(v.as_str()) {
                        unsafe {
                            libc::setenv(c_key.as_ptr(), c_value.as_ptr(), 1);
                        }
                    }
            }

            let c_exec = CString::new(parts[0]).unwrap_or_else(|_| {
                CString::new("/bin/sh").expect("failed to create CString for /bin/sh")
            });

            let mut c_args: Vec<CString> = Vec::with_capacity(parts.len());
            for part in &parts {
                if let Ok(c) = CString::new(*part) {
                    c_args.push(c);
                }
            }

            let arg_refs: Vec<&CString> = c_args.iter().collect();
            let _ = execvp(&c_exec, &arg_refs);

            std::process::exit(127);
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

pub fn reap_zombies(children: &[(String, u32)]) -> Vec<(String, ProcessStatus)> {
    let mut reaped = Vec::new();
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => break,
            Ok(WaitStatus::Exited(pid, status)) => {
                let name = children
                    .iter()
                    .find(|(_, p)| *p == pid.as_raw() as u32)
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| format!("PID {}", pid));
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
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| format!("PID {}", pid));
                reaped.push((name, ProcessStatus::Signaled(signal as i32)));
            }
            _ => break,
        }
    }
    reaped
}
