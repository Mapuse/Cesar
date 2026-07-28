#![allow(clippy::await_holding_lock)]

mod cli;
mod commands;
mod config;
mod dag;
mod health;
mod logger;
mod process;
mod service;
mod socket;
mod visual;

use std::ffi::CStr;
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
use socket::{SocketActivator, create_unix_socket};

static CESAR_LOGGER: OnceLock<CesarLogger> = OnceLock::new();
static CESAR_DAG: OnceLock<Mutex<DagEngine>> = OnceLock::new();
static RUNNING_CHILDREN: OnceLock<Mutex<Vec<(String, u32)>>> = OnceLock::new();
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static REAP_NEEDED: AtomicBool = AtomicBool::new(false);
static RELOAD_NEEDED: AtomicBool = AtomicBool::new(false);

fn get_logger() -> &'static CesarLogger {
    CESAR_LOGGER.get_or_init(CesarLogger::new)
}

fn get_dag() -> std::sync::MutexGuard<'static, DagEngine> {
    CESAR_DAG
        .get_or_init(|| Mutex::new(DagEngine::new()))
        .lock()
        .unwrap()
}

fn get_children() -> std::sync::MutexGuard<'static, Vec<(String, u32)>> {
    RUNNING_CHILDREN
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
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
        match status {
            process::ProcessStatus::Exited => {
                get_logger().log_warning(&name, "Process exited unexpectedly");
                get_children().retain(|(n, _)| n != &name);
                maybe_restart_service(&name);
            }
            process::ProcessStatus::Failed(code) => {
                get_logger().log_error(
                    &name,
                    &format!("Process exited with failure code {}", code),
                );
                get_dag().mark_failed(&name);
                get_children().retain(|(n, _)| n != &name);
                maybe_restart_service(&name);
            }
            process::ProcessStatus::Signaled(sig) => {
                get_logger().log_error(&name, &format!("Killed by signal {}", sig));
                get_dag().mark_failed(&name);
                get_children().retain(|(n, _)| n != &name);
                maybe_restart_service(&name);
            }
            _ => {}
        }
    }
}

fn maybe_restart_service(name: &str) {
    let (restart_policy, max_restarts, current_count, exec, env_vars, work_dir) = {
        let dag = get_dag();
        if let Some(svc) = dag.services.get(name) {
            (
                svc.config.restart.clone(),
                5,
                svc.restart_count,
                svc.config.exec.clone(),
                svc.config.environment.clone(),
                svc.config.working_directory.clone(),
            )
        } else {
            return;
        }
    };

    match restart_policy {
        RestartPolicy::Always | RestartPolicy::OnFailure => {
            if current_count >= max_restarts {
                get_logger().log_error(
                    name,
                    &format!(
                        "Service exceeded max restart limit ({}), not restarting",
                        max_restarts
                    ),
                );
                return;
            }

            let name_owned = name.to_string();
            get_logger().log_info(
                name,
                &format!(
                    "Auto-restarting service (attempt {}/{})",
                    current_count + 1,
                    max_restarts
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
                sleep(Duration::from_millis(1000)).await;
                match process::spawn_service_env(&name_owned, &exec, &env_vars, work_dir.as_deref(), logger_ref) {
                    Ok(pid) => {
                        add_running_child(&name_owned, pid);
                        let mut dag = get_dag();
                        dag.mark_running(&name_owned, pid);
                        logger_ref.log_service_event(
                            &name_owned,
                            &format!("Service auto-restarted (PID {})", pid),
                        );
                    }
                    Err(e) => {
                        logger_ref.log_error(&name_owned, &format!("Auto-restart failed: {}", e));
                    }
                }
            });
        }
        RestartPolicy::Never => {}
    }
}

fn add_running_child(name: &str, pid: u32) {
    get_children().push((name.to_string(), pid));
}

fn plymouth_update(text: &str) {
    let _ = std::process::Command::new("plymouth")
        .args(["update", &format!("--text={}", text)])
        .output();
}

fn plymouth_message(text: &str) {
    let _ = std::process::Command::new("plymouth")
        .args(["message", &format!("--text={}", text)])
        .output();
}

fn plymouth_hide() {
    let _ = std::process::Command::new("plymouth")
        .args(["hide-splash"])
        .output();
}

fn plymouth_show() {
    let _ = std::process::Command::new("plymouth")
        .args(["show-splash"])
        .output();
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

    let mut failed_count = 0;
    let mut failed_services: Vec<(String, String)> = Vec::new();

    {
        let mut sockets = SocketActivator::new();
        let dag = get_dag();
        for (name, svc) in &dag.services {
            if let Some(ref socket_spec) = svc.config.socket
                && let Some((path, sock_type)) = SocketActivator::parse_socket_spec(socket_spec) {
                    sockets.register(name, &path, sock_type);
                    if let Err(e) = create_unix_socket(&path) {
                        logger.log_error(name, &format!("Socket activation failed: {}", e));
                    } else {
                        logger.log_service_event(name, &format!("Socket activated: {}", path));
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

        let mut handles = Vec::new();

        for name in level {
            plymouth_update(&format!("[Done] :: Starting {}...", name));
            let svc = {
                let mut dag = get_dag();
                dag.mark_starting(name);
                dag.services.get(name).cloned()
            };

            if let Some(svc) = svc {
                if !svc.is_ready() && !svc.config.requires.is_empty() {
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
                    let result = process::spawn_service_env(&service_name, &exec_path, &env_vars, work_dir.as_deref(), logger_ref);
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
            let result = handle.await.unwrap();
            match result {
                (name, Ok(pid)) => {
                    get_dag().mark_running(&name, pid);
                    logger.log_service_event(&name, &format!("Service started (PID {})", pid));
                }
                (name, Err(e)) => {
                    get_dag().mark_failed(&name);
                    logger.log_service_event(&name, &format!("Service failed: {}", e));
                    plymouth_message(&format!("[Error] :: {} failed: {}", name, e));
                    failed_services.push((name.clone(), e.clone()));
                    logger.log_error(&name, &e);
                    failed_count += 1;
                }
            }
        }

        sleep(Duration::from_millis(50)).await;
    }

    logger.log_boot_complete(total_services, failed_count);

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
    let logger = get_logger();
    logger.log_info("shutdown", "Graceful shutdown initiated");
    plymouth_update("[Done] :: Shutting down...");

    let children = get_children().clone();
    for (name, pid) in &children {
        get_dag().mark_stopping(name);
        logger.log_service_event(name, &format!("Sending SIGTERM (PID {})", pid));
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
    std::process::exit(0);
}

fn reload_services() {
    let logger = get_logger();
    logger.log_info("reload", "Service configuration reload requested");
    let new_configs = load_all_services();
    let mut dag = get_dag();
    for cfg in new_configs {
        let name = cfg.name.clone();
        if dag.services.contains_key(&name) {
            dag.services.get_mut(&name).unwrap().config = cfg;
        } else {
            dag.add_service(cfg);
        }
    }
    if let Err(e) = dag.build_dependency_graph() {
        logger.log_error("reload", &e);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let is_init = unsafe { libc::getpid() == 1 };

    if is_init {
        setup_signal_handlers();
        mount_virtual_filesystems();

        let logger = get_logger();
        logger.log_info("csr", "[Done] :: Init System starting as PID 1");

        boot_sequence_silent().await;

        loop {
            process_reaped_children();

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

    let mut failed_services: Vec<(String, String)> = Vec::new();

    for level in boot_order.iter() {
        let mut handles = Vec::new();

        for name in level {
            let svc = {
                dag.mark_starting(name);
                dag.services.get(name).cloned()
            };

            if let Some(svc) = svc {
                if !svc.is_ready() && !svc.config.requires.is_empty()
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
            let result = handle.await.unwrap();
            match result {
                (name, Ok(pid)) => {
                    dag.mark_running(&name, pid);
                    logger.log_service_event(&name, &format!("Service started (PID {})", pid));
                }
                (name, Err(e)) => {
                    dag.mark_failed(&name);
                    logger.log_service_event(&name, &format!("Service failed: {}", e));
                    failed_services.push((name, e));
                }
            }
        }
    }

    logger.log_boot_complete(total, failed_services.len());

    if !failed_services.is_empty() {
        let error_tree = visual::build_error_tree(dag, &failed_services);
        eprint!("{}", error_tree);
        std::process::exit(1);
    } else {
        println!("\x1b[32m✓\x1b[0m Boot complete. {} services running.", total);
    }
}

fn mount_virtual_filesystems() {
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
