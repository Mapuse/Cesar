#![allow(clippy::await_holding_lock)]

use cesar::cli;
use cesar::commands;
use cesar::config;
use cesar::dag;
use cesar::event;
use cesar::health;
use cesar::ipc;
use cesar::logger;
use cesar::process;
use cesar::service;
use cesar::socket;
use cesar::visual;

use std::ffi::CStr;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use clap::Parser;
use nix::sys::signal::{self, SigHandler, Signal};
use tokio::time::sleep;

use cli::Cli;
use config::load_all_services;
use dag::DagEngine;
use logger::CesarLogger;
use service::RestartPolicy;
use socket::SocketActivator;

static CESAR_LOGGER: OnceLock<CesarLogger> = OnceLock::new();
static CESAR_DAG: OnceLock<Mutex<DagEngine>> = OnceLock::new();
static RUNNING_CHILDREN: OnceLock<Mutex<Vec<(String, u32)>>> = OnceLock::new();
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
/// reboot(2) command for the pending shutdown: RB_POWER_OFF or RB_AUTOBOOT.
static REBOOT_CMD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(libc::RB_POWER_OFF);
static REAP_NEEDED: AtomicBool = AtomicBool::new(false);
static RELOAD_NEEDED: AtomicBool = AtomicBool::new(false);

const RESTART_MAX_ATTEMPTS: u32 = 5;
const RESTART_BASE_DELAY_MS: u64 = 1000;
const RESTART_MAX_DELAY_MS: u64 = 60_000;
const RESTART_RESET_AFTER_SECS: u64 = 30;

fn get_logger() -> &'static CesarLogger {
    CESAR_LOGGER.get_or_init(CesarLogger::new)
}

fn get_dag() -> std::sync::MutexGuard<'static, DagEngine> {
    // Recover from poisoning: a panicking task must not cascade into a
    // PID-1 fatal error; the guarded data stays structurally valid.
    CESAR_DAG
        .get_or_init(|| Mutex::new(DagEngine::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn get_children() -> std::sync::MutexGuard<'static, Vec<(String, u32)>> {
    RUNNING_CHILDREN
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ── Control socket server (PID 1 side) ────────────────────────────────

fn start_control_server() {
    let _ = std::fs::create_dir_all("/run/cesar");
    let _ = std::fs::remove_file(ipc::CONTROL_SOCKET);
    let listener = match UnixListener::bind(ipc::CONTROL_SOCKET) {
        Ok(l) => l,
        Err(e) => {
            let msg = format!("control socket bind failed: {}", e);
            get_logger().log_error("ipc", &msg);
            eprintln!("[Error] :: {}", msg);
            return;
        }
    };
    // Bind honors the daemon umask; force an explicit group-writable mode so
    // only root (and the wheel/admin group) can talk to PID 1.
    let _ = std::fs::set_permissions(
        ipc::CONTROL_SOCKET,
        std::fs::Permissions::from_mode(0o660),
    );
    let logger = get_logger();
    logger.log_info("ipc", "Control socket listening");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(stream) = stream {
                std::thread::spawn(move || handle_control_conn(stream));
            }
        }
        let _ = std::fs::remove_file(ipc::CONTROL_SOCKET);
    });
}

/// UID of the process on the other end of the socket, via SO_PEERCRED.
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    use std::os::unix::io::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    if ret == 0 { Some(cred.uid) } else { None }
}

fn handle_control_conn(stream: UnixStream) {
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let Some(action) = ipc::parse_request(&line) else {
        let _ = writeln!(writer, "ERR unknown request");
        return;
    };
    // Mutating actions require root on the control socket; read-only
    // queries stay available to local unprivileged callers.
    if ipc::action_is_privileged(&action) && peer_uid(reader.get_ref()) != Some(0) {
        let uid = peer_uid(reader.get_ref()).map(|u| u.to_string()).unwrap_or_else(|| "?".into());
        let name = ipc::action_name(&action);
        let reply = ControlReply::err(format!(
            "permission denied for '{}' (uid {}): root required",
            name, uid
        ));
        let _ = writeln!(writer, "{}", reply.status_line);
        return;
    }
    let reply = execute_control_action(action);
    if writeln!(writer, "{}", reply.status_line).is_err() {
        return;
    }
    if !reply.body.is_empty() {
        let _ = writer.write_all(reply.body.as_bytes());
    }
}

struct ControlReply {
    status_line: String,
    body: String,
}

impl ControlReply {
    fn ok(body: String) -> Self {
        ControlReply { status_line: "OK 0".into(), body }
    }
    fn err(msg: impl Into<String>) -> Self {
        ControlReply { status_line: format!("ERR {}", msg.into()), body: String::new() }
    }
}

fn execute_control_action(action: ipc::ControlAction) -> ControlReply {
    let logger = get_logger();
    match action {
        ipc::ControlAction::Ping => ControlReply::ok(String::new()),
        ipc::ControlAction::List => {
            let dag = get_dag();
            let mut body = String::new();
            for (name, svc) in &dag.services {
                body.push_str(&format!(
                    "{} {} {}\n",
                    name,
                    svc.state,
                    svc.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
                ));
            }
            ControlReply::ok(body)
        }
        ipc::ControlAction::Status(name) => {
            let dag = get_dag();
            match dag.services.get(&name) {
                Some(svc) => ControlReply::ok(format!(
                    "{} {} {}\n",
                    name,
                    svc.state,
                    svc.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
                )),
                None => ControlReply::err(format!("service '{}' not found", name)),
            }
        }
        ipc::ControlAction::Start(name) => {
            let (exec, env_vars, work_dir) = {
                let mut dag = get_dag();
                let Some(svc) = dag.services.get(&name) else {
                    return ControlReply::err(format!("service '{}' not found", name));
                };
                if matches!(svc.state, service::ServiceState::Running | service::ServiceState::Starting) {
                    return ControlReply::err(format!("service '{}' already {}", name, svc.state));
                }
                if !svc.config.requires.is_empty() && !dag.all_deps_satisfied(&name) {
                    return ControlReply::err(format!("dependencies not satisfied for '{}'", name));
                }
                let exec = svc.config.exec.clone();
                let env_vars = svc.config.environment.clone();
                let work_dir = svc.config.working_directory.clone();
                // Reserve the slot inside the critical section so two
                // concurrent Starts cannot both pass the state check.
                dag.mark_starting(&name);
                (exec, env_vars, work_dir)
            };
            match process::spawn_service_env(&name, &exec, &env_vars, work_dir.as_deref(), logger) {
                Ok(pid) => {
                    get_dag().mark_running(&name, pid);
                    get_children().push((name.clone(), pid));
                    logger.log_service_event(&name, &format!("Service started via IPC (PID {})", pid));
                    event::EventBus::emit_service(&name, "started", pid);
                    ControlReply::ok(format!("Started '{}' (PID {})\n", name, pid))
                }
                Err(e) => {
                    get_dag().mark_failed(&name);
                    ControlReply::err(format!("failed to start '{}': {}", name, e))
                }
            }
        }
        ipc::ControlAction::Stop(name) => {
            let pid = get_children().iter().find(|(n, _)| *n == name).map(|(_, p)| *p);
            match pid {
                Some(pid) => {
                    if let Err(e) = process::terminate_group(pid, Duration::from_secs(10)) {
                        logger.log_error(&name, &e);
                        return ControlReply::err(e);
                    }
                    get_children().retain(|(n, _)| *n != name);
                    get_dag().mark_stopped(&name);
                    logger.log_service_event(&name, &format!("Stopped via IPC (PID {})", pid));
                    event::EventBus::emit_service(&name, "stopped", pid);
                    ControlReply::ok(format!("Stopped '{}' (PID {})\n", name, pid))
                }
                None => ControlReply::err(format!("service '{}' is not running", name)),
            }
        }
        ipc::ControlAction::Restart(name) => {
            // stop (wait for death), then start again.
            let stopped = execute_control_action(ipc::ControlAction::Stop(name.clone()));
            if let Some(pid_str) = stopped.status_line.strip_prefix("OK") {
                let _ = pid_str;
            } else if !stopped.status_line.contains("not running") {
                return stopped;
            }
            execute_control_action(ipc::ControlAction::Start(name))
        }
        ipc::ControlAction::Shutdown => {
            logger.log_info("shutdown", "Shutdown requested via control socket");
            REBOOT_CMD.store(libc::RB_POWER_OFF, Ordering::SeqCst);
            // Main loop performs the actual graceful shutdown so the reply
            // reaches the client first.
            SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
            ControlReply::ok("Shutdown initiated\n".into())
        }
        ipc::ControlAction::Reboot => {
            logger.log_info("reboot", "Reboot requested via control socket");
            REBOOT_CMD.store(libc::RB_AUTOBOOT, Ordering::SeqCst);
            SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
            ControlReply::ok("Reboot initiated\n".into())
        }
        ipc::ControlAction::Poweroff => {
            logger.log_info("poweroff", "Poweroff requested via control socket");
            REBOOT_CMD.store(libc::RB_POWER_OFF, Ordering::SeqCst);
            SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
            ControlReply::ok("Poweroff initiated\n".into())
        }
    }
}

fn setup_signal_handlers() {
    unsafe {
        signal::signal(Signal::SIGCHLD, SigHandler::Handler(handle_sigchld)).ok();
        signal::signal(Signal::SIGTERM, SigHandler::Handler(handle_sigterm)).ok();
        signal::signal(Signal::SIGINT, SigHandler::Handler(handle_sigterm)).ok();
        signal::signal(Signal::SIGHUP, SigHandler::Handler(handle_sighup)).ok();
        signal::signal(Signal::SIGPIPE, SigHandler::SigIgn).ok();
    }
}

extern "C" fn handle_sigchld(_sig: i32) {
    REAP_NEEDED.store(true, Ordering::SeqCst);
}

extern "C" fn handle_sigterm(_sig: i32) {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

extern "C" fn handle_sighup(_sig: i32) {
    RELOAD_NEEDED.store(true, Ordering::SeqCst);
}

fn process_reaped_children() {
    if !REAP_NEEDED.swap(false, Ordering::SeqCst) {
        return;
    }

    let children = get_children().clone();
    let reaped = process::reap_zombies(&children);

    for (name, status) in reaped {
        let Some(name) = name else {
            // Orphaned process reparented to init: normal, not a service failure.
            get_logger().log_info("init", &format!("Reaped orphaned child ({:?})", status));
            continue;
        };
        match status {
            process::ProcessStatus::Exited => {
                get_logger().log_warning(&name, "Process exited unexpectedly");
                get_children().retain(|(n, _)| n != &name);
                maybe_restart_service(&name, &status);
            }
            process::ProcessStatus::Failed(code) => {
                get_logger().log_error(
                    &name,
                    &format!("Process exited with failure code {}", code),
                );
                get_dag().mark_failed(&name);
                get_children().retain(|(n, _)| n != &name);
                maybe_restart_service(&name, &status);
            }
            process::ProcessStatus::Signaled(sig) => {
                get_logger().log_error(&name, &format!("Killed by signal {}", sig));
                get_dag().mark_failed(&name);
                get_children().retain(|(n, _)| n != &name);
                maybe_restart_service(&name, &status);
            }
            _ => {}
        }
    }
}

fn maybe_restart_service(name: &str, status: &process::ProcessStatus) {
    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
        return;
    }
    let (restart_policy, current_count, exec, env_vars, work_dir) = {
        let dag = get_dag();
        if let Some(svc) = dag.services.get(name) {
            (
                svc.config.restart.clone(),
                svc.restart_count,
                svc.config.exec.clone(),
                svc.config.environment.clone(),
                svc.config.working_directory.clone(),
            )
        } else {
            return;
        }
    };

    let should_restart = match restart_policy {
        RestartPolicy::Always => true,
        RestartPolicy::OnFailure => !matches!(status, process::ProcessStatus::Exited),
        RestartPolicy::Never => false,
    };
    if !should_restart {
        return;
    }

    if current_count >= RESTART_MAX_ATTEMPTS {
        get_logger().log_error(
            name,
            &format!(
                "Service exceeded max restart limit ({}), not restarting",
                RESTART_MAX_ATTEMPTS
            ),
        );
        return;
    }

    // Exponential backoff: 1s, 2s, 4s, ... capped at RESTART_MAX_DELAY_MS.
    let backoff_ms = RESTART_BASE_DELAY_MS
        .saturating_mul(1u64 << current_count.min(16))
        .min(RESTART_MAX_DELAY_MS);

    let name_owned = name.to_string();
    get_logger().log_info(
        name,
        &format!(
            "Auto-restarting service (attempt {}/{} in {}ms)",
            current_count + 1,
            RESTART_MAX_ATTEMPTS,
            backoff_ms
        ),
    );

    {
        let mut dag = get_dag();
        if let Some(svc) = dag.services.get_mut(&name_owned) {
            svc.restart_count += 1;
        }
    }

    let logger_ref = get_logger();
    tokio::spawn(async move {
        sleep(Duration::from_millis(backoff_ms)).await;
        // A shutdown may have been requested while we waited: never
        // resurrect services mid-shutdown.
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            return;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process::spawn_service_env(&name_owned, &exec, &env_vars, work_dir.as_deref(), logger_ref)
        }));
        match result {
            Ok(Ok(pid)) => {
                add_running_child(&name_owned, pid);
                get_dag().mark_running(&name_owned, pid);
                logger_ref.log_service_event(
                    &name_owned,
                    &format!("Service auto-restarted (PID {})", pid),
                );
                event::EventBus::emit_service(&name_owned, "restarted", pid);
            }
            Ok(Err(e)) => {
                logger_ref.log_error(&name_owned, &format!("Auto-restart failed: {}", e));
                event::EventBus::emit_service(&name_owned, "restart-failed", 0);
            }
            Err(_) => {
                logger_ref.log_error(&name_owned, "Auto-restart task panicked");
                get_dag().mark_failed(&name_owned);
                event::EventBus::emit_service(&name_owned, "restart-failed", 0);
            }
        }
    });
}

/// Services that stayed Running past RESTART_RESET_AFTER_SECS are considered
/// stable: clear their restart counter so a later crash gets a fresh budget.
fn reset_idle_restart_counters() {
    let mut dag = get_dag();
    let mut reset = Vec::new();
    for (name, svc) in &dag.services {
        if matches!(svc.state, service::ServiceState::Running)
            && svc.restart_count > 0
            && let Some(started) = svc.started_at
            && started.elapsed() >= Duration::from_secs(RESTART_RESET_AFTER_SECS)
        {
            reset.push(name.clone());
        }
    }
    for name in reset {
        if let Some(svc) = dag.services.get_mut(&name) {
            svc.restart_count = 0;
        }
        get_logger().log_info(&name, "Service stable; restart counter reset");
    }
}

fn add_running_child(name: &str, pid: u32) {
    get_children().push((name.to_string(), pid));
}

/// Best-effort external command from PID 1: spawn, poll, and kill after a
/// short timeout — never block boot/shutdown on a hung helper.
fn run_helper_timeout(program: &str, args: &[&str], timeout: Duration) {
    let mut child = match std::process::Command::new(program).args(args).spawn() {
        Ok(c) => c,
        Err(_) => return,
    };
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return,
        }
    }
}

fn plymouth_update(text: &str) {
    run_helper_timeout("plymouth", &["update", &format!("--text={}", text)], Duration::from_secs(2));
}

fn plymouth_message(text: &str) {
    run_helper_timeout("plymouth", &["message", &format!("--text={}", text)], Duration::from_secs(2));
}

fn plymouth_hide() {
    run_helper_timeout("plymouth", &["hide-splash"], Duration::from_secs(2));
}

fn plymouth_show() {
    run_helper_timeout("plymouth", &["show-splash"], Duration::from_secs(2));
}

async fn boot_sequence_silent() {
    let logger = get_logger();
    logger.log_boot_start();

    let report = health::HealthReport::run();
    if !report.is_healthy() {
        report.log_issues(logger);
        plymouth_message("[Done] :: System issues detected, check /var/log/cesar.md");
    }

    plymouth_show();
    plymouth_update("[Done] :: Loading services...");

    let configs = load_all_services();
    if configs.is_empty() {
        logger.log_info("boot", "No services found to start");
        plymouth_message("[Done] :: No services found");
        plymouth_hide();
        return;
    }

    {
        let mut dag = get_dag();
        for cfg in configs {
            dag.add_service(cfg);
        }
        if let Err(e) = dag.build_dependency_graph() {
            logger.log_error("boot", &e);
            eprintln!("[Error] :: {}", e);
            plymouth_message(&format!("[Error] :: {}", e));
            plymouth_hide();
            return;
        }
    }

    let (boot_order, total_services) = {
        let dag = get_dag();
        let order = dag.get_boot_order();
        let total = order.iter().map(|l| l.len()).sum();
        (order, total)
    };

    for name in get_dag().unresolved() {
        logger.log_warning(
            &name,
            "Unresolved at boot (cyclic dependency); service will not start",
        );
    }

    let mut failed_count = 0;
    let mut failed_services: Vec<(String, String)> = Vec::new();

    {
        let mut sockets = SocketActivator::new();
        let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
        let dag = get_dag();
        for (name, svc) in &dag.services {
            if let Some(ref socket_spec) = svc.config.socket
                && let Some((path, sock_type)) = SocketActivator::parse_socket_spec(socket_spec) {
                    // One socket per path: a second claimant would silently
                    // steal the first service's listeners.
                    if !seen_paths.insert(path.clone()) {
                        logger.log_error(name, &format!("Socket '{}' already registered by another service; skipping", path));
                        continue;
                    }
                    sockets.register(name, &path, sock_type.clone());
                    match socket::create_socket_for(&path, &sock_type) {
                        Ok(fd) => {
                            socket::register_listen_fd(name, fd);
                            logger.log_service_event(name, &format!("Socket activated: {} (fd {})", path, fd));
                        }
                        Err(e) => {
                            logger.log_error(name, &format!("Socket activation failed: {}", e));
                        }
                    }
                }
        }
    }

    for (level_idx, level) in boot_order.iter().enumerate() {
        plymouth_update(&format!(
            "[Done] :: Level {}/{} starting {} services",
            level_idx + 1,
            boot_order.len(),
            level.len()
        ));
        logger.log_info(
            "boot",
            &format!(
                "Level {}/{}: {} services: {}",
                level_idx + 1,
                boot_order.len(),
                level.len(),
                level.join(", ")
            ),
        );

        // Reap the previous level's exits first so dependency checks below
        // see Failed instead of zombie "Running" services.
        process_reaped_children();

        let mut handles = Vec::new();

        for name in level {
            plymouth_update(&format!("[Done] :: Starting {}...", name));
            let svc = {
                let mut dag = get_dag();
                dag.mark_starting(name);
                dag.services.get(name).cloned()
            };

            if let Some(svc) = svc {
                if !svc.config.requires.is_empty() {
                    let deps_ok = {
                        let dag = get_dag();
                        dag.all_deps_satisfied(name)
                    };
                    if !deps_ok {
                        logger.log_warning(name, "Dependencies not satisfied, skipping");
                        get_dag().mark_failed(name);
                        failed_services.push((name.clone(), "Dependencies not satisfied".to_string()));
                        failed_count += 1;
                        continue;
                    }
                }

                logger.log_service_event(name, "Service starting");
                let exec_path = svc.config.exec.clone();
                let service_name = name.clone();
                let env_vars = svc.config.environment.clone();
                let work_dir = svc.config.working_directory.clone();
                let logger_ref = get_logger();

                let handle = tokio::spawn(async move {
                    // catch_unwind: a spawn-task panic must not kill PID 1
                    // (meaningful again now that panic=unwind).
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process::spawn_service_env(&service_name, &exec_path, &env_vars, work_dir.as_deref(), logger_ref)
                    }))
                    .unwrap_or_else(|_| Err("spawn task panicked".to_string()));
                    match result {
                        Ok(pid) => {
                            add_running_child(&service_name, pid);
                            (service_name, Ok(pid))
                        }
                        Err(e) => (service_name, Err(e)),
                    }
                });
                handles.push(handle);
            }
        }

        for handle in handles {
            let result = match handle.await {
                Ok(result) => result,
                Err(_) => ("join-failed".to_string(), Err("spawn task panicked".to_string())),
            };
            match result {
                (name, Ok(pid)) => {
                    get_dag().mark_running(&name, pid);
                    logger.log_service_event(&name, &format!("Service started (PID {})", pid));
                    event::EventBus::emit_service(&name, "started", pid);
                }
                (name, Err(e)) => {
                    get_dag().mark_failed(&name);
                    logger.log_service_event(&name, &format!("Service failed: {}", e));
                    event::EventBus::emit_service(&name, "failed", 0);
                    plymouth_message(&format!("[Error] :: {} failed: {}", name, e));
                    failed_services.push((name.clone(), e.clone()));
                    logger.log_error(&name, &e);
                    failed_count += 1;
                }
            }
        }

        // Reap this level's exec failures (e.g. exit(127)) so zombies are
        // marked Failed before dependents of the next level are checked.
        process_reaped_children();

        sleep(Duration::from_millis(50)).await;
    }

    logger.log_boot_complete(total_services, failed_count);
    event::EventBus::emit_boot(total_services, failed_count);

    if !failed_services.is_empty() {
        let dag = get_dag();
        let error_tree = visual::build_error_tree(&dag, &failed_services);
        eprint!("{}", error_tree);
        logger.log_error("boot", &format!("{} services failed to start", failed_count));
        plymouth_message(&format!("[Error] :: Boot failed ({} errors)", failed_count));
    } else {
        plymouth_update("[Done] :: Boot complete");
        sleep(Duration::from_millis(500)).await;
        plymouth_hide();
    }
}

fn shutdown_graceful() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    let logger = get_logger();
    logger.log_info("shutdown", "Graceful shutdown initiated");
    event::EventBus::emit_shutdown();
    plymouth_update("[Done] :: Shutting down...");

    // Stop services in reverse topological order so dependents terminate
    // before the services they rely on.
    let stop_order: Vec<String> = {
        let dag = get_dag();
        let mut order: Vec<String> = dag.get_boot_order().into_iter().flatten().collect();
        order.reverse();
        order
    };
    let children = get_children().clone();
    let pid_by_name: std::collections::HashMap<String, u32> =
        children.iter().cloned().collect();

    for name in &stop_order {
        if let Some(pid) = pid_by_name.get(name) {
            get_dag().mark_stopping(name);
            logger.log_service_event(name, &format!("Sending SIGTERM (PID {})", pid));
            process::kill_service_group(*pid, Signal::SIGTERM as i32).ok();
        }
    }
    // Anything running but absent from the DAG still gets stopped.
    for (name, pid) in &children {
        if stop_order.contains(name) {
            continue;
        }
        logger.log_warning(name, &format!("SIGTERM to untracked service (PID {})", pid));
        process::kill_service_group(*pid, Signal::SIGTERM as i32).ok();
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if std::time::Instant::now() >= deadline {
            break;
        }
        process_reaped_children();
        std::thread::sleep(Duration::from_millis(50));
    }

    let children = get_children().clone();
    for (name, pid) in &children {
        if process::check_process(*pid) == process::ProcessStatus::Alive {
            logger.log_warning(name, &format!("SIGKILL to PID {}", pid));
            process::kill_service_group(*pid, Signal::SIGKILL as i32).ok();
        }
        get_dag().mark_stopped(name);
        logger.log_service_event(name, "Service stopped");
    }

    logger.log_info("shutdown", "System halted");
    plymouth_message("[Done] :: System halted");
    unsafe { libc::sync(); }
    // Best-effort: put writable filesystems into a safe state before the
    // final reboot(2). Failures here are non-fatal; sync() already ran.
    unsafe {
        libc::mount(
            std::ptr::null(),
            c"/".as_ptr(),
            std::ptr::null(),
            libc::MS_REMOUNT | libc::MS_RDONLY,
            std::ptr::null(),
        );
        for target in ["/run", "/home", "/var"] {
            let c_target = std::ffi::CString::new(target).expect("static mount path");
            libc::umount2(c_target.as_ptr(), libc::MNT_DETACH);
        }
        libc::sync();
    }
    // As PID 1 we must never `exit()` — the kernel panics. Hand control
    // back to the kernel to power off or reboot as requested.
    let cmd = REBOOT_CMD.load(Ordering::SeqCst);
    unsafe { libc::reboot(cmd); }
    // reboot() only returns on failure; block forever as a fallback.
    loop {
        std::thread::sleep(Duration::from_secs(3600));
        process_reaped_children();
    }
}

fn reload_services() {
    let logger = get_logger();
    logger.log_info("reload", "Service configuration reload requested");
    let new_configs = load_all_services();
    let mut dag = get_dag();

    // Diff out services deleted from disk; stop running ones first so no
    // orphaned child keeps running untracked.
    let incoming: std::collections::HashSet<&str> =
        new_configs.iter().map(|c| c.name.as_str()).collect();
    let removed: Vec<String> = dag
        .services
        .keys()
        .filter(|name| !incoming.contains(name.as_str()))
        .cloned()
        .collect();
    for name in removed {
        let pid = dag.services.get(&name).and_then(|svc| svc.pid);
        if let Some(pid) = pid {
            logger.log_info(&name, &format!("Removed from config; stopping (PID {})", pid));
            process::terminate_group(pid, Duration::from_secs(5)).ok();
            get_children().retain(|(n, _)| n != &name);
        }
        dag.remove_service(&name);
        logger.log_info(&name, "Service removed by reload");
    }

    // Update existing entries and insert newly enabled services.
    for cfg in new_configs {
        let name = cfg.name.clone();
        match dag.services.get_mut(&name) {
            Some(svc) => svc.config = cfg,
            None => {
                logger.log_info(&name, "New service added by reload");
                dag.add_service(cfg);
            }
        }
    }

    // The graph swap is atomic inside build_dependency_graph: on error the
    // previous graph stays live and we only report.
    if let Err(e) = dag.build_dependency_graph() {
        logger.log_error("reload", &e);
    }

    for name in dag.unresolved() {
        logger.log_warning(
            &name,
            "Unresolved after reload (cyclic dependency); service not scheduled",
        );
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    cps::configure(cps::Options::new("cesar"));

    let is_init = unsafe { libc::getpid() == 1 };

    if is_init {
        // PID 1 inherits the kernel default umask (0); tighten it before
        // any socket/log/directory is created so nothing lands world- or
        // group-writable by accident.
        unsafe { libc::umask(0o022); }
        setup_signal_handlers();
        mount_virtual_filesystems();
        // The event bus listener is intentionally NOT bound here: PID 1 only
        // emits (fire-and-forget datagrams). Leaving /run/cesar/event.sock
        // unbound lets external consumers (`csr service watch`, monitors)
        // bind it and actually receive events; binding inside PID 1 would
        // fill the buffer and silently drop everything.
        let logger = get_logger();
        logger.log_info("csr", "[Done] :: Init System starting as PID 1");

        boot_sequence_silent().await;
        start_control_server();

        loop {
            process_reaped_children();
            reset_idle_restart_counters();

            if RELOAD_NEEDED.swap(false, Ordering::SeqCst) {
                reload_services();
            }

            if SHUTDOWN_REQUESTED.swap(false, Ordering::SeqCst) {
                shutdown_graceful();
            }

            sleep(Duration::from_millis(100)).await;
        }
    } else {
        let cli = Cli::parse();

        match cli.command {
            Some(cmd) => {
                // If a live init is running, forward live-state commands to
                // it so status/start/stop/restart/list reflect reality.
                if let Some(action) = cli_control_action(&cmd) {
                    ipc::try_forward(&action);
                }
                let mut dag = get_dag();
                let logger = get_logger();
                commands::execute(cmd, &mut dag, logger);
            }
            None => {
                let mut dag = get_dag();
                let logger = get_logger();
                boot_sequence_for_cli(&mut dag, logger).await;
            }
        }
    }
}

/// Map CLI commands to control-socket actions (None = local-only).
fn cli_control_action(cmd: &cli::TopCommand) -> Option<ipc::ControlAction> {
    use cli::{ServiceCommand, SystemCommand, TopCommand};
    match cmd {
        TopCommand::Service(ServiceCommand::Start(args)) => Some(ipc::ControlAction::Start(args.name.clone())),
        TopCommand::Service(ServiceCommand::Stop(args)) => Some(ipc::ControlAction::Stop(args.name.clone())),
        TopCommand::Service(ServiceCommand::Restart(args)) => Some(ipc::ControlAction::Restart(args.name.clone())),
        TopCommand::Service(ServiceCommand::List(_)) => Some(ipc::ControlAction::List),
        TopCommand::Service(ServiceCommand::Status(args)) => {
            args.name.as_ref().map(|n| ipc::ControlAction::Status(n.clone()))
        }
        TopCommand::System(SystemCommand::Shutdown(_)) => Some(ipc::ControlAction::Shutdown),
        TopCommand::System(SystemCommand::Reboot(_)) => Some(ipc::ControlAction::Reboot),
        TopCommand::System(SystemCommand::Poweroff(_)) => Some(ipc::ControlAction::Poweroff),
        _ => None,
    }
}

async fn boot_sequence_for_cli(dag: &mut DagEngine, logger: &CesarLogger) {
    logger.log_boot_start();

    let report = health::HealthReport::run();
    if !report.is_healthy() {
        report.log_issues(logger);
    }

    let configs = load_all_services();
    for cfg in configs {
        dag.add_service(cfg);
    }
    if let Err(e) = dag.build_dependency_graph() {
        logger.log_error("boot", &e);
        eprintln!("[Error] :: {}", e);
        return;
    }

    let boot_order = dag.get_boot_order();
    let total: usize = boot_order.iter().map(|l| l.len()).sum();

    for name in dag.unresolved() {
        logger.log_warning(
            &name,
            "Unresolved at boot (cyclic dependency); service will not start",
        );
        eprintln!("\x1b[33m⚠\x1b[0m Service '{}' is on a dependency cycle and will not start", name);
    }

    let mut failed_services: Vec<(String, String)> = Vec::new();

    for level in boot_order.iter() {
        let mut handles = Vec::new();

        for name in level {
            let svc = {
                dag.mark_starting(name);
                dag.services.get(name).cloned()
            };

            if let Some(svc) = svc {
                if !svc.config.requires.is_empty()
                    && !dag.all_deps_satisfied(name) {
                        logger.log_warning(name, "Dependencies not satisfied, skipping");
                        dag.mark_failed(name);
                        failed_services.push((name.clone(), "Dependencies not satisfied".to_string()));
                        continue;
                    }

                logger.log_service_event(name, "Service starting");
                let exec_path = svc.config.exec.clone();
                let service_name = name.clone();
                let env_vars = svc.config.environment.clone();
                let work_dir = svc.config.working_directory.clone();
                let logger_ref = get_logger();

                let handle = tokio::spawn(async move {
                    match process::spawn_service_env(&service_name, &exec_path, &env_vars, work_dir.as_deref(), logger_ref) {
                        Ok(pid) => (service_name, Ok(pid)),
                        Err(e) => (service_name, Err(e)),
                    }
                });
                handles.push(handle);
            }
        }

        for handle in handles {
            let result = handle.await.expect("service spawn task");
            match result {
                (name, Ok(pid)) => {
                    dag.mark_running(&name, pid);
                    logger.log_service_event(&name, &format!("Service started (PID {})", pid));
                    event::EventBus::emit_service(&name, "started", pid);
                }
                (name, Err(e)) => {
                    dag.mark_failed(&name);
                    logger.log_service_event(&name, &format!("Service failed: {}", e));
                    event::EventBus::emit_service(&name, "failed", 0);
                    failed_services.push((name, e));
                }
            }
        }
    }

    logger.log_boot_complete(total, failed_services.len());
    event::EventBus::emit_boot(total, failed_services.len());

    if !failed_services.is_empty() {
        let error_tree = visual::build_error_tree(dag, &failed_services);
        eprint!("{}", error_tree);
        std::process::exit(1);
    } else {
        println!("\x1b[32m✓\x1b[0m Boot complete. {} services running.", total);
    }
}

fn mount_virtual_filesystems() {
    // /run must exist for the control socket; create+mount tmpfs if absent.
    if !std::path::Path::new("/run").exists() {
        let _ = std::fs::create_dir("/run");
        let ret = unsafe {
            libc::mount(
                c"tmpfs".as_ptr(), c"/run".as_ptr(), c"tmpfs".as_ptr(),
                libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
                c"mode=0755".as_ptr().cast(),
            )
        };
        if ret != 0 {
            eprintln!("[Warning] :: mount /run failed: {}", std::io::Error::last_os_error());
        }
    }
    unsafe {
        let mounts: [(&CStr, &CStr, &CStr, libc::c_ulong); 3] = [
            (c"proc", c"/proc", c"proc", libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC),
            (c"sysfs", c"/sys", c"sysfs", libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC),
            (c"devtmpfs", c"/dev", c"devtmpfs", libc::MS_NOSUID | libc::MS_NOEXEC),
        ];
        for (source, target, fstype, flags) in mounts {
            let ret = libc::mount(
                source.as_ptr(), target.as_ptr(), fstype.as_ptr(),
                flags, std::ptr::null(),
            );
            if ret != 0 {
                eprintln!("[Warning] :: mount {} failed: {}", target.to_string_lossy(), std::io::Error::last_os_error());
            }
        }
    }
}
