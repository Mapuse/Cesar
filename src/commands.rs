use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process;

use crate::cli::*;
use crate::config;
use crate::dag::DagEngine;
use crate::health;
use crate::logger::CesarLogger;
use crate::process as cprocess;
use crate::service::ServiceState;
use crate::visual;
use nix::sys::signal::Signal;

pub fn execute(cmd: TopCommand, dag: &mut DagEngine, logger: &CesarLogger) {
    match cmd {
        TopCommand::Service(c) => handle_service(c, dag, logger),
        TopCommand::System(c) => handle_system(c, dag, logger),
        TopCommand::Config(c) => handle_config(c, logger),
        TopCommand::Log(c) => handle_log(c, logger),
        TopCommand::Socket(c) => handle_socket(c, logger),
        TopCommand::Daemon(c) => handle_daemon(c, dag, logger),
        TopCommand::Snapshot(c) => handle_snapshot(c, dag, logger),
        TopCommand::Security(c) => handle_security(c, dag, logger),
        TopCommand::Query(c) => handle_query(c, dag, logger),
        TopCommand::Debug(c) => handle_debug(c, dag, logger),
        TopCommand::Self_(c) => handle_self(c, logger),
        TopCommand::Plugin(c) => handle_plugin(c),
        TopCommand::Theme(c) => handle_theme(c),
        TopCommand::Tui(c) => handle_tui(c),
    }
}

fn load_services_if_empty(dag: &mut DagEngine) {
    if dag.services.is_empty() {
        let configs = config::load_all_services();
        for cfg in configs {
            dag.add_service(cfg);
        }
        let _ = dag.build_dependency_graph();
    }
}

fn print_service_diagnostics(name: &str, error_msg: &str, dag: &DagEngine) {
    if let Some(svc) = dag.services.get(name) {
        let exec = &svc.config.exec;
        if error_msg.contains("not found") || error_msg.contains("No such file") {
            if !Path::new(exec).exists() {
                eprintln!("  \x1b[33m→\x1b[0m Binary '{}' does not exist", exec);
                let parts: Vec<&str> = exec.split_whitespace().collect();
                if !parts.is_empty() {
                    let bin_name = Path::new(parts[0]).file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !bin_name.is_empty() {
                        let search_paths = ["/bin", "/sbin", "/usr/bin", "/usr/sbin", "/system/bin"];
                        for sp in &search_paths {
                            let full = format!("{}/{}", sp, bin_name);
                            if Path::new(&full).exists() {
                                eprintln!("  \x1b[33m→\x1b[0m Found similar binary at {}", full);
                                eprintln!("  \x1b[32m→\x1b[0m Fix: Update Exec in /etc/cesar/services/{}.ini to '{}'", name, full);
                                return;
                            }
                        }
                        eprintln!("  \x1b[31m✗\x1b[0m Binary '{}' not found in any search path", exec);
                        eprintln!("  \x1b[32m→\x1b[0m Fix: Install {} or update Exec in service config", bin_name);
                    }
                }
            }
        } else if error_msg.contains("permission") || error_msg.contains("Permission denied") {
            eprintln!("  \x1b[31m✗\x1b[0m Permission denied executing '{}'", exec);
            let parts: Vec<&str> = exec.split_whitespace().collect();
            if let Some(bin) = parts.first() {
                let meta = fs::metadata(bin);
                match meta {
                    Ok(m) => {
                        let mode = m.permissions().mode();
                        eprintln!("  \x1b[33m→\x1b[0m File mode: {:o}", mode);
                        if mode & 0o111 == 0 {
                            eprintln!("  \x1b[32m→\x1b[0m Fix: chmod +x {}", bin);
                        }
                    }
                    Err(e) => {
                        eprintln!("  \x1b[33m→\x1b[0m Cannot stat binary: {}", e);
                    }
                }
            }
        } else if error_msg.contains("depend") || error_msg.contains("Required") {
            eprintln!("  \x1b[31m✗\x1b[0m Dependency failure for service '{}'", name);
            for req in &svc.config.requires {
                if let Some(dep_svc) = dag.services.get(req) {
                    let alive = dep_svc.pid.is_some_and(|pid| {
                        cprocess::check_process(pid) == cprocess::ProcessStatus::Alive
                    });
                    if !alive {
                        eprintln!("  \x1b[33m→\x1b[0m Required service '{}' is not running (state: {})", req, dep_svc.state);
                    }
                } else {
                    eprintln!("  \x1b[33m→\x1b[0m Required service '{}' not found in configuration", req);
                }
            }
            eprintln!("  \x1b[32m→\x1b[0m Fix: Check dependency services with `csr service status {}`", name);
        } else {
            eprintln!("  \x1b[31m✗\x1b[0m Service '{}' failed: {}", name, error_msg);
            eprintln!("  \x1b[32m→\x1b[0m Fix: Check logs with `csr log view --service {}`", name);
        }
    } else {
        eprintln!("  \x1b[31m✗\x1b[0m Service '{}' not found in DAG", name);
        eprintln!("  \x1b[32m→\x1b[0m Fix: Check /etc/cesar/services/ for {}.ini", name);
    }
}


fn handle_system(cmd: SystemCommand, dag: &mut DagEngine, logger: &CesarLogger) {
    match cmd {
        SystemCommand::Boot(args) => {
            let _ = logger;
            if args.splash {
                let _ = std::process::Command::new("plymouth").args(["show-splash"]).output();
                println!("\x1b[32m✓\x1b[0m Plymouth splash enabled");
            }
            println!("Boot: splash={}, verbose={}, single={}, emergency={}", args.splash, args.verbose, args.single, args.emergency);
        }
        SystemCommand::Shutdown(args) => {
            logger.log_info("shutdown", "Graceful shutdown initiated");
            println!("\x1b[33m⚠\x1b[0m System shutting down...");
            if !args.force {
                println!("Sending SIGTERM to all services (timeout={}s)...", args.timeout);
                load_services_if_empty(dag);
                let service_names: Vec<String> = dag.services.keys().cloned().collect();
                for name in &service_names {
                    if let Some(svc) = dag.services.get(name)
                        && let Some(pid) = svc.pid {
                            unsafe { libc::kill(pid as i32, libc::SIGTERM); }
                        }
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.timeout);
                loop {
                    if std::time::Instant::now() >= deadline { break; }
                    let all_stopped = service_names.iter().all(|n| {
                        dag.services.get(n).map(|s| !matches!(s.state, ServiceState::Running)).unwrap_or(true)
                    });
                    if all_stopped { break; }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                for name in &service_names {
                    if let Some(svc) = dag.services.get(name)
                        && matches!(svc.state, ServiceState::Running)
                            && let Some(pid) = svc.pid {
                                unsafe { libc::kill(pid as i32, libc::SIGKILL); }
                            }
                }
                println!("Services stopped.");
            }
            unsafe { libc::sync(); }
            unsafe { libc::reboot(if args.reboot { libc::RB_AUTOBOOT } else { libc::RB_POWER_OFF }); }
        }
        SystemCommand::Reboot(_args) => {
            logger.log_info("reboot", "Reboot initiated");
            println!("\x1b[33m⚠\x1b[0m Rebooting...");
            unsafe { libc::sync(); }
            unsafe { libc::reboot(libc::RB_AUTOBOOT); }
        }
        SystemCommand::Poweroff(_args) => {
            logger.log_info("poweroff", "Poweroff initiated");
            println!("\x1b[33m⚠\x1b[0m Powering off...");
            unsafe { libc::sync(); }
            unsafe { libc::reboot(libc::RB_POWER_OFF); }
        }
        SystemCommand::Emergency(args) => {
            let reason = args.reason.unwrap_or_else(|| "unknown".to_string());
            logger.log_error("emergency", &reason);
            eprintln!("\x1b[31m✗\x1b[0m EMERGENCY: {}", reason);
            let _ = std::process::Command::new("plymouth").args(["message", &format!("--text=Cesar EMERGENCY: {}", reason)]).output();
        }
        SystemCommand::Suspend(args) => {
            let state_path = "/sys/power/state";
            let state = if args.hibernate || args.hybrid { "disk" } else { "mem" };
            match fs::read_to_string(state_path) {
                Ok(content) => {
                    let states: Vec<&str> = content.split_whitespace().collect();
                    if states.contains(&state) {
                        match fs::write(state_path, state) {
                            Ok(()) => println!("\x1b[32m✓\x1b[0m Suspending ({})...", state),
                            Err(e) => eprintln!("\x1b[31m✗\x1b[0m Suspend failed: {}", e),
                        }
                    } else {
                        eprintln!("\x1b[31m✗\x1b[0m State '{}' not supported. Available: {}", state, content.trim());
                    }
                }
                Err(e) => {
                    eprintln!("\x1b[31m✗\x1b[0m Cannot read /sys/power/state: {}", e);
                    eprintln!("  Ensure CONFIG_PM_SLEEP is enabled in the kernel");
                }
            }
        }
        SystemCommand::Resume => {
            println!("\x1b[32m✓\x1b[0m System resumed from suspend");
            let _ = std::process::Command::new("plymouth").args(["hide-splash"]).output();
        }
        SystemCommand::Freeze(_args) => {
            let freezer = "/sys/fs/cgroup/freezer/freezer.state";
            if Path::new(freezer).exists() {
                match fs::write(freezer, "FROZEN") {
                    Ok(()) => println!("\x1b[32m✓\x1b[0m Processes frozen"),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Freeze failed: {}", e),
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Cgroup freezer not found at {}", freezer);
            }
        }
        SystemCommand::Thaw => {
            let freezer = "/sys/fs/cgroup/freezer/freezer.state";
            if Path::new(freezer).exists() {
                match fs::write(freezer, "THAWED") {
                    Ok(()) => println!("\x1b[32m✓\x1b[0m Processes thawed"),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Thaw failed: {}", e),
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Cgroup freezer not found");
            }
        }
        SystemCommand::Mount(args) => {
            if let (Some(source), Some(target)) = (&args.source, &args.target) {
                let fs_type = args.fs_type.as_deref().unwrap_or("auto");
                let options = args.options.as_deref().unwrap_or("");
                let mut flags: libc::c_ulong = 0;
                if args.recursive { flags |= libc::MS_BIND | libc::MS_REC; }
                if args.remount { flags |= libc::MS_REMOUNT; }
                let c_source = std::ffi::CString::new(source.as_str()).expect("mount source CString");
                let c_target = std::ffi::CString::new(target.as_str()).expect("mount target CString");
                let c_fstype = std::ffi::CString::new(fs_type).expect("fs type CString");
                let c_options = std::ffi::CString::new(options).expect("mount options CString");
                let ret = unsafe { libc::mount(c_source.as_ptr(), c_target.as_ptr(), c_fstype.as_ptr(), flags, c_options.as_ptr() as *const libc::c_void) };
                if ret == 0 {
                    println!("\x1b[32m✓\x1b[0m Mounted {} on {} ({})", source, target, fs_type);
                } else {
                    let err = std::io::Error::last_os_error();
                    eprintln!("\x1b[31m✗\x1b[0m Mount failed: {}", err);
                    eprintln!("  Source: {}, Target: {}, Type: {}", source, target, fs_type);
                }
            } else {
                let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
                for line in mounts.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 { println!("  {:<30} {:<20} {}", parts[0], parts[1], parts[2]); }
                }
            }
        }
        SystemCommand::Umount(args) => {
            if let Some(ref target) = args.target {
                let c_target = std::ffi::CString::new(target.as_str()).expect("umount target CString");
                let mut flags: libc::c_int = 0;
                if args.lazy { flags |= libc::MNT_DETACH; }
                if args.force { flags |= libc::MNT_FORCE; }
                let ret = unsafe { libc::umount2(c_target.as_ptr(), flags) };
                if ret == 0 {
                    println!("\x1b[32m✓\x1b[0m Unmounted {}", target);
                } else {
                    let err = std::io::Error::last_os_error();
                    eprintln!("\x1b[31m✗\x1b[0m Unmount failed: {}", err);
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Target required: csr system umount -t /mount/point");
            }
        }
        SystemCommand::Sync(_args) => {
            unsafe { libc::sync(); }
            println!("\x1b[32m✓\x1b[0m Filesystems synced");
        }
        SystemCommand::Hostname(args) => {
            if let Some(ref h) = args.set {
                let c_hostname = std::ffi::CString::new(h.as_str()).expect("hostname CString");
                let ret = unsafe { libc::sethostname(c_hostname.as_ptr(), h.len()) };
                if ret == 0 { println!("\x1b[32m✓\x1b[0m Hostname set to '{}'", h); }
                else { eprintln!("\x1b[31m✗\x1b[0m Failed to set hostname: {}", std::io::Error::last_os_error()); }
            } else {
                let hostname = hostname::get().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
                if args.short { println!("{}", hostname.split('.').next().unwrap_or(&hostname)); }
                else { println!("{}", hostname); }
            }
        }
        SystemCommand::Uptime(args) => {
            match fs::read_to_string("/proc/uptime") {
                Ok(uptime_str) => {
                    let secs: f64 = uptime_str.split_whitespace().next().unwrap_or("0").parse().unwrap_or(0.0);
                    if args.seconds { println!("{:.0}", secs); }
                    else { println!("up {:.0} days, {:.0}:{:02.0}", secs / 86400.0, (secs % 86400.0) / 3600.0, (secs % 3600.0) / 60.0); }
                }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Cannot read /proc/uptime: {}", e),
            }
        }
        SystemCommand::Kernel(args) => match args.command {
            Some(KernelSubCommand::List) => {
                match fs::read_to_string("/proc/modules") {
                    Ok(modules) => { for line in modules.lines() { let name = line.split_whitespace().next().unwrap_or(""); println!("  {}", name); } }
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Cannot read /proc/modules: {}", e),
                }
            }
            Some(KernelSubCommand::Log { follow, lines, level }) => {
                if follow {
                    println!("Following kernel log (Ctrl+C to stop)...");
                    let _ = process::Command::new("dmesg").arg("-w").status();
                } else {
                    let output = process::Command::new("dmesg").output();
                    match output {
                        Ok(o) if o.status.success() => {
                            let dmesg = String::from_utf8_lossy(&o.stdout);
                            let all_lines: Vec<&str> = dmesg.lines().collect();
                            let start = all_lines.len().saturating_sub(lines);
                            for line in &all_lines[start..] {
                                if let Some(ref lvl) = level && !line.contains(lvl) { continue; }
                                println!("{}", line);
                            }
                        }
                        _ => { for line in fs::read_to_string("/dev/kmsg").unwrap_or_default().lines().take(lines) { println!("{}", line); } }
                    }
                }
                let _ = level;
            }
            Some(KernelSubCommand::Parameters { grep }) => {
                let sysctl_dir = "/proc/sys/kernel";
                if let Ok(entries) = fs::read_dir(sysctl_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if let Some(ref pattern) = grep && !name.contains(pattern) { continue; }
                        if let Ok(val) = fs::read_to_string(entry.path()) { println!("kernel.{} = {}", name, val.trim()); }
                    }
                }
            }
            Some(KernelSubCommand::Module { name, load, unload }) => {
                if let Some(ref mod_name) = name {
                    if load {
                        match process::Command::new("modprobe").arg(mod_name).status() {
                            Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Module '{}' loaded", mod_name),
                            _ => {
                                let content = fs::read_to_string("/proc/modules").unwrap_or_default();
                                if content.lines().any(|l| l.starts_with(mod_name)) { println!("\x1b[33m⚠\x1b[0m Module '{}' already loaded", mod_name); }
                                else { eprintln!("\x1b[31m✗\x1b[0m modprobe failed. Install kmod: apt install kmod"); }
                            }
                        }
                    } else if unload {
                        match process::Command::new("rmmod").arg(mod_name).status() {
                            Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Module '{}' unloaded", mod_name),
                            _ => eprintln!("\x1b[31m✗\x1b[0m rmmod failed. Install kmod: apt install kmod"),
                        }
                    } else {
                        let content = fs::read_to_string("/proc/modules").unwrap_or_default();
                        if let Some(line) = content.lines().find(|l| l.starts_with(mod_name)) {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            println!("Module: {}", mod_name);
                            if parts.len() >= 2 { println!("  Size: {} bytes", parts[1]); }
                            if parts.len() >= 3 { println!("  Used: {}", parts[2]); }
                        } else { eprintln!("\x1b[31m✗\x1b[0m Module '{}' not loaded", mod_name); }
                    }
                } else { eprintln!("\x1b[31m✗\x1b[0m Module name required: csr system kernel module -n <name> -l"); }
            }
            None => { let v = fs::read_to_string("/proc/version").unwrap_or_default(); println!("{}", v.trim()); }
        },
        SystemCommand::Env(args) => {
            if let Some(ref key_val) = args.set {
                if let Some((k, v)) = key_val.split_once('=') {
                    unsafe { std::env::set_var(k, v); }
                    println!("\x1b[32m✓\x1b[0m {}={}", k, v);
                } else { eprintln!("\x1b[31m✗\x1b[0m Invalid format. Use: KEY=VALUE"); }
            } else if let Some(ref key) = args.get {
                match std::env::var(key) {
                    Ok(v) => println!("{}={}", key, v),
                    Err(_) => eprintln!("\x1b[31m✗\x1b[0m '{}' not set", key),
                }
            } else if let Some(ref key) = args.unset {
                unsafe { std::env::remove_var(key); }
                println!("\x1b[32m✓\x1b[0m '{}' unset", key);
            } else {
                for (k, v) in std::env::vars() { println!("{}={}", k, v); }
            }
        }
        SystemCommand::Resource(_args) => {
            if let Ok(stat) = fs::read_to_string("/proc/stat")
                && let Some(cpu_line) = stat.lines().next() {
                    let parts: Vec<&str> = cpu_line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let idle: u64 = parts[4].parse().unwrap_or(0);
                        let total: u64 = parts[1..].iter().filter_map(|p| p.parse::<u64>().ok()).sum();
                        let used = total - idle;
                        println!("CPU:    {:.1}% used", if total > 0 { used as f64 / total as f64 * 100.0 } else { 0.0 });
                    }
                }
            if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
                let mut mem_total = 0u64; let mut mem_avail = 0u64;
                for line in meminfo.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let val: u64 = parts[1].parse().unwrap_or(0);
                        if parts[0] == "MemTotal:" { mem_total = val; }
                        if parts[0] == "MemAvailable:" { mem_avail = val; }
                    }
                }
                if mem_total > 0 { println!("Memory: {:.1}% used ({:.1}MB / {:.1}MB)", (mem_total - mem_avail) as f64 / mem_total as f64 * 100.0, (mem_total - mem_avail) as f64 / 1024.0, mem_total as f64 / 1024.0); }
            }
            if let Ok(loadavg) = fs::read_to_string("/proc/loadavg") {
                let parts: Vec<&str> = loadavg.split_whitespace().collect();
                if parts.len() >= 3 { println!("Load:   {} (1m), {} (5m), {} (15m)", parts[0], parts[1], parts[2]); }
            }
        }
        SystemCommand::Cgroup(args) => {
            if args.list {
                let cgroup_root = "/sys/fs/cgroup";
                match fs::read_dir(cgroup_root) {
                    Ok(entries) => {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let procs = entry.path().join("cgroup.procs");
                            let count = fs::read_to_string(&procs).map(|c| c.lines().count()).unwrap_or(0);
                            println!("  {:<30} {:>6} processes", name, count);
                        }
                    }
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Cannot read /sys/fs/cgroup: {}", e),
                }
            }
        }
        SystemCommand::Device(args) => {
            if args.list {
                match fs::read_dir("/dev") {
                    Ok(entries) => {
                        for entry in entries.flatten() {
                            println!("  {}", entry.file_name().to_string_lossy());
                        }
                    }
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Cannot read /dev: {}", e),
                }
            }
        }
    }
}


fn handle_config(cmd: ConfigCommand, _logger: &CesarLogger) {
    match cmd {
        ConfigCommand::Show(args) => {
            let path = args.file.unwrap_or_else(|| "/etc/cesar".to_string());
            if path.ends_with(".ini") || path.ends_with(".toml") || path.ends_with(".conf") || path.ends_with(".service") {
                match fs::read_to_string(&path) {
                    Ok(content) => { println!("--- {} ---", path); print!("{}", content); }
                    Err(e) => { eprintln!("\x1b[31m✗\x1b[0m Cannot read {}: {}", path, e); }
                }
            } else if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() { println!("  {}", entry.file_name().to_string_lossy()); }
            } else { eprintln!("\x1b[31m✗\x1b[0m Cannot read {}", path); }
        }
        ConfigCommand::Get(args) => {
            let config_path = "/etc/cesar/cesar.ini";
            match fs::read_to_string(config_path) {
                Ok(content) => {
                    for line in content.lines() {
                        if let Some((key, value)) = line.trim().split_once('=')
                            && key.trim() == args.key { println!("{}", value.trim()); return; }
                    }
                    if let Some(ref default) = args.default { println!("{}", default); }
                    else { eprintln!("\x1b[31m✗\x1b[0m Key '{}' not found in {}", args.key, config_path); }
                }
                Err(e) => {
                    if let Some(ref default) = args.default { println!("{}", default); }
                    else { eprintln!("\x1b[31m✗\x1b[0m Cannot read {}: {}", config_path, e); }
                }
            }
        }
        ConfigCommand::Set(args) => {
            let config_path = "/etc/cesar/cesar.ini";
            fs::create_dir_all("/etc/cesar").ok();
            let mut content = fs::read_to_string(config_path).unwrap_or_default();
            let mut found = false;
            let new_lines: Vec<String> = content.lines().map(|line| {
                if let Some((key, _)) = line.trim().split_once('=') {
                    if key.trim() == args.key { found = true; format!("{} = {}", args.key, args.value) }
                    else { line.to_string() }
                } else { line.to_string() }
            }).collect();
            if found {
                content = new_lines.join("\n");
                content.push('\n');
            } else {
                let section = format!("[cesar]\n{} = {}\n", args.key, args.value);
                if content.is_empty() {
                    content = section;
                } else {
                    content.push('\n');
                    content.push_str(&section);
                }
            }
            match fs::write(config_path, &content) {
                Ok(()) => println!("\x1b[32m✓\x1b[0m Set {} = {}", args.key, args.value),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed: {}", e),
            }
        }
        ConfigCommand::Edit(args) => {
            let path = args.file.unwrap_or_else(|| "/etc/cesar/cesar.ini".to_string());
            let editor = args.editor.unwrap_or_else(|| std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()));
            let _ = process::Command::new(&editor).arg(&path).status();
        }
        ConfigCommand::Diff(args) => {
            let file1 = args.file.unwrap_or_else(|| "/etc/cesar/cesar.ini".to_string());
            let file2 = args.target.unwrap_or_else(|| "/system/lib/cesar/cesar.ini".to_string());
            let ctx = args.context.unwrap_or(3);
            let c1 = fs::read_to_string(&file1);
            let c2 = fs::read_to_string(&file2);
            match (&c1, &c2) {
                (Ok(a), Ok(b)) => {
                    if a == b { println!("\x1b[32m✓\x1b[0m Files are identical"); }
                    else {
                        let l1: Vec<&str> = a.lines().collect();
                        let l2: Vec<&str> = b.lines().collect();
                        let mut last_diff = -(ctx as isize);
                        for i in 0..l1.len().max(l2.len()) {
                            let a = l1.get(i).unwrap_or(&"");
                            let b = l2.get(i).unwrap_or(&"");
                            if a != b {
                                let idx = i as isize;
                                if idx - last_diff > 1 && last_diff >= 0 {
                                    println!("\x1b[90m---\x1b[0m");
                                }
                                println!("\x1b[31m- {}\x1b[0m", a);
                                println!("\x1b[32m+ {}\x1b[0m", b);
                                last_diff = idx;
                            }
                        }
                    }
                }
                (Err(e), _) => eprintln!("\x1b[31m✗\x1b[0m Cannot read {}: {}", file1, e),
                (_, Err(e)) => eprintln!("\x1b[31m✗\x1b[0m Cannot read {}: {}", file2, e),
            }
        }
        ConfigCommand::Validate(_args) => {
            let dirs = ["/system/lib/cesar/services", "/etc/cesar/services"];
            let mut valid = 0; let mut invalid = 0;
            for dir in &dirs {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                            match config::parse_cesar_config(path.to_str().unwrap_or_default()) {
                                Ok(_) => { println!("\x1b[32m✓\x1b[0m {}", path.display()); valid += 1; }
                                Err(e) => { invalid += 1; eprintln!("\x1b[31m✗\x1b[0m {}: {}", path.display(), e); }
                            }
                        }
                    }
                }
            }
            println!("\n{} valid, {} invalid", valid, invalid);
        }
        ConfigCommand::Import(args) => {
            let dest = "/etc/cesar/cesar.ini";
            fs::create_dir_all("/etc/cesar").ok();
            match fs::copy(&args.file, dest) {
                Ok(_) => println!("\x1b[32m✓\x1b[0m Config imported from '{}'", args.file),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Import failed: {}", e),
            }
        }
        ConfigCommand::Export(args) => {
            let output = args.output.unwrap_or_else(|| format!("/tmp/cesar-config-{}.toml", chrono::Local::now().format("%Y%m%d_%H%M%S")));
            match fs::copy("/etc/cesar/cesar.ini", &output) {
                Ok(_) => println!("\x1b[32m✓\x1b[0m Config exported to '{}'", output),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Export failed: {}", e),
            }
        }
        ConfigCommand::Backup(args) => {
            let output = args.output.unwrap_or_else(|| format!("/var/backup/cesar-{}", chrono::Local::now().format("%Y%m%d_%H%M%S")));
            match process::Command::new("cp").args(["-a", "/etc/cesar", &output]).status() {
                Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Backed up to '{}'", output),
                _ => {
                    if let Err(e) = fs::create_dir_all(&output) {
                        eprintln!("\x1b[31m✗\x1b[0m Backup failed: cannot create '{}': {}", output, e);
                        return;
                    }
                    let mut failures = 0;
                    if let Ok(entries) = fs::read_dir("/etc/cesar") {
                        for entry in entries.flatten() {
                            if fs::copy(entry.path(), Path::new(&output).join(entry.file_name())).is_err() {
                                failures += 1;
                            }
                        }
                    }
                    if failures > 0 {
                        eprintln!("\x1b[33m⚠\x1b[0m Backed up to '{}' ({} files failed to copy)", output, failures);
                    } else {
                        println!("\x1b[32m✓\x1b[0m Backed up to '{}'", output);
                    }
                }
            }
        }
        ConfigCommand::Restore(args) => {
            if !args.force { eprintln!("\x1b[33m⚠\x1b[0m Use --force to confirm restore"); return; }
            match process::Command::new("cp").args(["-a", &args.file, "/etc/cesar"]).status() {
                Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Restored from '{}'", args.file),
                _ => eprintln!("\x1b[31m✗\x1b[0m Restore failed"),
            }
        }
        ConfigCommand::Schema(_args) => {
            println!("Cesar service config schema v{}", env!("CARGO_PKG_VERSION"));
            println!("Service config keys (INI format):");
            println!("  Name            = service name (required)");
            println!("  Exec            = command to execute (required)");
            println!("  Requires        = comma-separated dependency list");
            println!("  Restart         = always | on-failure | never");
            println!("  Socket          = unix socket path");
            println!("  Description     = human-readable description");
            println!("  Environment     = space-separated KEY=VALUE pairs");
            println!("  WorkingDirectory = path to set as working directory");
        }
        ConfigCommand::Migrate(args) => {
            println!("Cesar config migration v{}", env!("CARGO_PKG_VERSION"));
            let dirs = ["/system/lib/cesar/services", "/etc/cesar/services"];
            let mut migrated = 0u32;
            for dir in &dirs {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("ini")
                            && let Ok(content) = fs::read_to_string(&path)
                                && !content.contains("Environment") && content.contains("Exec") {
                                    let append = "\nEnvironment =\nWorkingDirectory =\n";
                                    let new_content = format!("{}{}", content.trim_end(), append);
                                    println!("  {} — adding Environment/WorkingDirectory", path.display());
                                    if !args.dry_run {
                                        fs::write(&path, &new_content).ok();
                                    }
                                    migrated += 1;
                                }
                    }
                }
            }
            if migrated == 0 { println!("\x1b[32m✓\x1b[0m All configs up to date"); }
            else if args.dry_run { println!("\x1b[33m⚠\x1b[0m {} file(s) would be migrated (dry-run)", migrated); }
            else { println!("\x1b[32m✓\x1b[0m Migrated {} file(s)", migrated); }
        }
    }
}


fn handle_log(cmd: LogCommand, logger: &CesarLogger) {
    let content = logger.read_log();
    match cmd {
        LogCommand::View(args) => {
            let lines: Vec<&str> = content.lines().collect();
            let mut shown = 0;
            let iter: Box<dyn Iterator<Item = &&str>> = if args.reverse { Box::new(lines.iter().rev()) } else { Box::new(lines.iter()) };
            for line in iter {
                if shown >= args.lines { break; }
                if let Some(ref svc) = args.service && !line.contains(&format!("[{}]", svc)) && !line.starts_with(">") { continue; }
                if let Some(ref grep) = args.grep && !line.contains(grep) && !line.starts_with("##") { continue; }
                println!("{}", line);
                shown += 1;
            }
        }
        LogCommand::Tail(args) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(args.lines);
            for line in &lines[start..] { println!("{}", line); }
        }
        LogCommand::Head(args) => {
            for (i, line) in content.lines().enumerate() { if i >= args.lines { break; } println!("{}", line); }
        }
        LogCommand::Grep(args) => {
            let count = content.lines().filter(|l| l.to_lowercase().contains(&args.pattern.to_lowercase())).count();
            if args.count { println!("{}", count); }
            else { for (i, line) in content.lines().enumerate() { if line.to_lowercase().contains(&args.pattern.to_lowercase()) { println!("L{}: {}", i + 1, line); } } }
        }
        LogCommand::Clear(args) => {
            if args.yes { logger.clear_log(); println!("\x1b[32m✓\x1b[0m Log cleared"); }
            else { println!("\x1b[33m⚠\x1b[0m Use -y to confirm"); }
        }
        LogCommand::Rotate(args) => {
            let log_path = "/var/log/cesar.md";
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let rotated = format!("{}.{}", log_path, ts);
            match fs::rename(log_path, &rotated) {
                Ok(()) => { println!("\x1b[32m✓\x1b[0m Rotated to '{}'", rotated); logger.clear_log(); }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Rotation failed: {}", e),
            }
            let _ = args;
        }
        LogCommand::Archive(args) => {
            let output = args.output.unwrap_or_else(|| "/var/log/cesar-archive".to_string());
            fs::create_dir_all(&output).ok();
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let dest = format!("{}/cesar-{}.md", output, ts);
            match fs::copy("/var/log/cesar.md", &dest) {
                Ok(_) => println!("\x1b[32m✓\x1b[0m Archived to '{}'", dest),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Archive failed: {}", e),
            }
        }
        LogCommand::Export(args) => {
            let output = args.output.unwrap_or_else(|| format!("/tmp/cesar-logs-{}.txt", chrono::Local::now().format("%Y%m%d_%H%M%S")));
            match fs::copy("/var/log/cesar.md", &output) {
                Ok(_) => println!("\x1b[32m✓\x1b[0m Exported to '{}'", output),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Export failed: {}", e),
            }
        }
        LogCommand::Follow(args) => {
            println!("Following log (Ctrl+C to stop)...");
            let log_path = "/var/log/cesar.md";
            let mut last_len = fs::metadata(log_path).map(|m| m.len() as usize).unwrap_or(0);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(args.sleep));
                if let Ok(new_content) = fs::read_to_string(log_path)
                    && new_content.len() > last_len { print!("{}", &new_content[last_len..]); last_len = new_content.len(); std::io::stdout().flush().ok(); }
            }
        }
        LogCommand::Errors(_args) => {
            let sections = parse_log_sections(&content);
            let mut total = 0;
            for (name, lines) in &sections {
                for line in lines {
                    if line.contains("CRITICAL") || line.contains("ERROR") {
                        println!("  [{}] {}", name, line.trim_start_matches('>').trim());
                        total += 1;
                    }
                }
            }
            if total == 0 { println!("\x1b[32m✓ No errors found\x1b[0m"); }
            else { println!("\n{} error(s) found", total); }
        }
        LogCommand::Warnings(_args) => {
            let mut count = 0;
            for line in content.lines() { if line.contains("WARNING") { println!("  {}", line.trim_start_matches('>').trim()); count += 1; } }
            if count == 0 { println!("\x1b[32m✓ No warnings found\x1b[0m"); } else { println!("\n{} warning(s) found", count); }
        }
        LogCommand::Stats(_args) => {
            let sections = parse_log_sections(&content);
            println!("Log Statistics\n{}", "─".repeat(40));
            for (name, lines) in &sections {
                let errors = lines.iter().filter(|l| l.contains("CRITICAL")).count();
                let warnings = lines.iter().filter(|l| l.contains("WARNING")).count();
                println!("  {:<20} {} entries ({} err, {} warn)", name, lines.len(), errors, warnings);
            }
        }
        LogCommand::Summary(_args) => {
            let lines: Vec<&str> = content.lines().collect();
            let errors = lines.iter().filter(|l| l.contains("CRITICAL")).count();
            let warnings = lines.iter().filter(|l| l.contains("WARNING")).count();
            println!("Log Summary\n{}", "─".repeat(40));
            println!("  Total:   {}", lines.len());
            println!("  Errors:  {}", errors);
            println!("  Warnings: {}", warnings);
        }
    }
}

fn parse_log_sections(content: &str) -> Vec<(String, Vec<String>)> {
    let mut sections = Vec::new();
    let mut current: Option<String> = None;
    let mut lines = Vec::new();
    for line in content.lines() {
        if line.starts_with("## [") {
            if let Some(name) = current.take() { sections.push((name, lines.clone())); lines.clear(); }
            current = Some(line.trim_start_matches("## [").trim_end_matches(']').to_string());
        } else if current.is_some() && !line.trim().is_empty() { lines.push(line.to_string()); }
    }
    if let Some(name) = current { sections.push((name, lines)); }
    sections
}


fn handle_socket(cmd: SocketCommand, _logger: &CesarLogger) {
    match cmd {
        SocketCommand::List(_) => {
            let dirs = ["/run/cesar/sockets", "/run/cesar"];
            let mut found = false;
            for dir in &dirs {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.ends_with(".sock") || name.ends_with(".socket") {
                            let state = if entry.path().exists() { "\x1b[32mactive\x1b[0m" } else { "\x1b[31minactive\x1b[0m" };
                            println!("  {:<30} {}", name, state);
                            found = true;
                        }
                    }
                }
            }
            if !found { println!("No active sockets found"); }
        }
        SocketCommand::Status(args) => {
            let path = args.path.unwrap_or_else(|| "/run/cesar/sockets".to_string());
            if Path::new(&path).exists() { println!("Socket: {} — active", path); }
            else { eprintln!("\x1b[31m✗\x1b[0m Socket '{}' not found", path); }
        }
        SocketCommand::Create(args) => {
            match crate::socket::create_unix_socket(&args.path) {
                Ok(()) => println!("\x1b[32m✓\x1b[0m Socket created at '{}'", args.path),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m {}", e),
            }
        }
        SocketCommand::Destroy(args) => {
            match fs::remove_file(&args.path) {
                Ok(()) => println!("\x1b[32m✓\x1b[0m Socket '{}' destroyed", args.path),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to destroy socket '{}': {}", args.path, e),
            }
        }
        SocketCommand::Monitor(args) => {
            println!("Monitoring '{}' (Ctrl+C to stop)...", args.path);
            let mut was_present = Path::new(&args.path).exists();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(args.interval));
                let is_present = Path::new(&args.path).exists();
                let ts = chrono::Local::now().format("%H:%M:%S");
                if is_present && !was_present { println!("[{}] \x1b[32m✓\x1b[0m Socket appeared", ts); }
                else if !is_present && was_present { println!("[{}] \x1b[31m✗\x1b[0m Socket disappeared", ts); }
                was_present = is_present;
            }
        }
        SocketCommand::Trace(args) => {
            println!("Tracing socket '{}' (Ctrl+C to stop)...", args.path);
            let log_path = "/var/log/cesar.md";
            let mut last_len = fs::metadata(log_path).map(|m| m.len() as usize).unwrap_or(0);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if let Ok(content) = fs::read_to_string(log_path)
                    && content.len() > last_len {
                        for line in content[last_len..].lines() {
                            if line.contains("socket") || line.contains(&args.path) { println!("{}", line.trim_start_matches('>').trim()); }
                        }
                        last_len = content.len();
                    }
            }
        }
        SocketCommand::Activate(args) => {
            if Path::new(&args.path).exists() { println!("\x1b[32m✓\x1b[0m Socket '{}' already active", args.path); }
            else {
                match crate::socket::create_unix_socket(&args.path) {
                    Ok(()) => println!("\x1b[32m✓\x1b[0m Socket '{}' activated", args.path),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed: {}", e),
                }
            }
        }
        SocketCommand::Query(args) => {
            if !Path::new(&args.path).exists() { eprintln!("\x1b[31m✗\x1b[0m Socket '{}' not found", args.path); return; }
            use std::os::unix::net::UnixStream;
            use std::io::Write;
            match UnixStream::connect(&args.path) {
                Ok(mut stream) => {
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(args.timeout))).ok();
                    if let Some(ref data) = args.data {
                        stream.write_all(data.as_bytes()).ok();
                        let mut response = Vec::new();
                        std::io::Read::read(&mut stream, &mut response).ok();
                        println!("Received {} bytes", response.len());
                    } else { println!("Connected to '{}'. Use -d to send data", args.path); }
                }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Cannot connect: {}", e),
            }
        }
    }
}


fn handle_daemon(cmd: DaemonCommand, dag: &mut DagEngine, logger: &CesarLogger) {
    match cmd {
        DaemonCommand::Start(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                let exec = svc.config.exec.clone();
                let env_vars = svc.config.environment.clone();
                let work_dir = svc.config.working_directory.clone();
                let name = args.name.clone();
                match cprocess::spawn_service_env(&name, &exec, &env_vars, work_dir.as_deref(), logger) {
                    Ok(pid) => { dag.mark_running(&name, pid); println!("\x1b[32m✓\x1b[0m Daemon '{}' started (PID {})", name, pid); }
                    Err(e) => { dag.mark_failed(&name); eprintln!("\x1b[31m✗\x1b[0m {}", e); }
                }
            } else { eprintln!("\x1b[31m✗\x1b[0m Daemon '{}' not found", args.name); }
        }
        DaemonCommand::Stop(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name)
                && let Some(pid) = svc.pid {
                    let _ = cprocess::kill_service_group(pid, Signal::SIGTERM as i32);
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while std::time::Instant::now() < deadline {
                        if cprocess::check_process(pid) != cprocess::ProcessStatus::Alive { break; }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    if cprocess::check_process(pid) == cprocess::ProcessStatus::Alive {
                        let _ = cprocess::kill_service_group(pid, Signal::SIGKILL as i32);
                    }
                    dag.mark_stopped(&args.name);
                    println!("\x1b[32m✓\x1b[0m Daemon '{}' stopped", args.name);
                }
        }
        DaemonCommand::Restart(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name).cloned() {
                if let Some(pid) = svc.pid {
                    let _ = cprocess::kill_service_group(pid, Signal::SIGTERM as i32);
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    while std::time::Instant::now() < deadline {
                        if cprocess::check_process(pid) != cprocess::ProcessStatus::Alive { break; }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    dag.mark_stopped(&args.name);
                }
                let exec = svc.config.exec.clone();
                let env_vars = svc.config.environment.clone();
                let work_dir = svc.config.working_directory.clone();
                let name = args.name.clone();
                match cprocess::spawn_service_env(&name, &exec, &env_vars, work_dir.as_deref(), logger) {
                    Ok(pid) => { dag.mark_running(&name, pid); println!("\x1b[32m✓\x1b[0m Daemon '{}' restarted (PID {})", name, pid); }
                    Err(e) => { dag.mark_failed(&name); eprintln!("\x1b[31m✗\x1b[0m {}", e); }
                }
            }
        }
        DaemonCommand::Status(args) => {
            load_services_if_empty(dag);
            if let Some(ref name) = args.name {
                if let Some(svc) = dag.services.get(name) {
                    let icon = match svc.state { ServiceState::Running => "\x1b[32m●\x1b[0m", ServiceState::Failed => "\x1b[31m✗\x1b[0m", ServiceState::Reloading => "\x1b[33m↻\x1b[0m", ServiceState::Stopping => "\x1b[33m⏹\x1b[0m", _ => "\x1b[90m○\x1b[0m" };
                    println!("{} Daemon: {} [{}]", icon, name, svc.state);
                }
            } else { visual::print_status(dag); }
        }
        DaemonCommand::List(args) => {
            load_services_if_empty(dag);
            for (name, svc) in &dag.services {
                let icon = match svc.state { ServiceState::Running => "\x1b[32m●\x1b[0m", ServiceState::Failed => "\x1b[31m✗\x1b[0m", ServiceState::Reloading => "\x1b[33m↻\x1b[0m", ServiceState::Stopping => "\x1b[33m⏹\x1b[0m", _ => "\x1b[90m○\x1b[0m" };
                println!("{} {}", icon, name);
            }
            let _ = args;
        }
        DaemonCommand::Log(args) => {
            let log_content = logger.read_log();
            let lines: Vec<&str> = log_content.lines().collect();
            let start = lines.len().saturating_sub(args.lines);
            for line in &lines[start..] { println!("{}", line); }
        }
        DaemonCommand::Install(args) => {
            if let Some(ref from) = args.from {
                let dest = format!("/etc/cesar/services/{}.ini", args.name);
                fs::create_dir_all("/etc/cesar/services").ok();
                match fs::copy(from, &dest) {
                    Ok(_) => println!("\x1b[32m✓\x1b[0m Daemon '{}' installed from '{}'", args.name, from),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Install failed: {}", e),
                }
            } else { eprintln!("\x1b[31m✗\x1b[0m Source required: csr daemon install -n {} -f /path/to/service.ini", args.name); }
        }
        DaemonCommand::Uninstall(args) => {
            let path = format!("/etc/cesar/services/{}.ini", args.name);
            match fs::remove_file(&path) {
                Ok(()) => { println!("\x1b[32m✓\x1b[0m Daemon '{}' uninstalled", args.name); fs::remove_file(format!("/etc/cesar/enabled/{}.ini", args.name)).ok(); }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Uninstall failed: {}", e),
            }
        }
        DaemonCommand::Update(args) => {
            if let Some(ref from) = args.from {
                let dest = format!("/etc/cesar/services/{}.ini", args.name);
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                if Path::new(&dest).exists() { fs::copy(&dest, format!("{}.{}", dest, ts)).ok(); }
                match fs::copy(from, &dest) {
                    Ok(_) => println!("\x1b[32m✓\x1b[0m Daemon '{}' updated", args.name),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Update failed: {}", e),
                }
            } else { eprintln!("\x1b[31m✗\x1b[0m Source required: csr daemon update -n {} -f /path/to/new.ini", args.name); }
        }
        DaemonCommand::Rollback(args) => {
            let path = format!("/etc/cesar/services/{}.ini", args.name);
            if let Ok(entries) = fs::read_dir("/etc/cesar/services") {
                let mut backups: Vec<_> = entries.flatten().filter(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.starts_with(&format!("{}.", args.name)) && n.ends_with(".ini")
                }).collect();
                backups.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
                if let Some(latest) = backups.first() {
                    fs::copy(latest.path(), &path).ok();
                    println!("\x1b[32m✓\x1b[0m Rolled back to '{}'", latest.file_name().to_string_lossy());
                } else { eprintln!("\x1b[31m✗\x1b[0m No backups found for '{}'", args.name); }
            }
        }
        DaemonCommand::Pin(args) => {
            let pin_path = format!("/etc/cesar/pinned/{}.version", args.name);
            fs::create_dir_all("/etc/cesar/pinned").ok();
            if let Some(ref version) = args.version {
                fs::write(&pin_path, version).ok();
                println!("\x1b[32m✓\x1b[0m Daemon '{}' pinned to '{}'", args.name, version);
            } else if Path::new(&pin_path).exists() {
                let v = fs::read_to_string(&pin_path).unwrap_or_default();
                println!("Daemon '{}' pinned to '{}'", args.name, v.trim());
            } else { println!("Daemon '{}' is not pinned", args.name); }
        }
        DaemonCommand::Trust(args) => {
            let trust_path = format!("/etc/cesar/trust/{}.key", args.name);
            fs::create_dir_all("/etc/cesar/trust").ok();
            if let Some(ref key) = args.key {
                fs::write(&trust_path, key).ok();
                println!("\x1b[32m✓\x1b[0m Trust key set for '{}'", args.name);
            } else if args.verify {
                if Path::new(&trust_path).exists() {
                    let key = fs::read_to_string(&trust_path).unwrap_or_default();
                    println!("Trust key for '{}': {}", args.name, key.trim());
                } else { println!("No trust key for '{}'", args.name); }
            }
        }
    }
}


fn handle_snapshot(cmd: SnapshotCommand, dag: &mut DagEngine, _logger: &CesarLogger) {
    let snap_dir = "/var/lib/cesar/snapshots";
    match cmd {
        SnapshotCommand::Create(args) => {
            fs::create_dir_all(snap_dir).ok();
            let snap_path = format!("{}/{}", snap_dir, args.name);
            if Path::new(&snap_path).exists() { eprintln!("\x1b[33m⚠\x1b[0m Snapshot '{}' already exists. Delete first.", args.name); return; }
            fs::create_dir_all(&snap_path).ok();
            load_services_if_empty(dag);
            let mut data = format!("# Snapshot: {}\n# Created: {}\n\n", args.name, chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
            for (name, svc) in &dag.services {
                data.push_str(&format!("[{}]\nstate = {}\npid = {:?}\nexec = {}\n\n", name, svc.state, svc.pid, svc.config.exec));
            }
            fs::write(format!("{}/state.toml", snap_path), &data).ok();
            let services_dir = format!("{}/services", snap_path);
            fs::create_dir_all(&services_dir).ok();
            for dir in &["/system/lib/cesar/services", "/etc/cesar/services"] {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if entry.path().extension().map(|e| e.to_str() == Some("ini")).unwrap_or(false) {
                            fs::copy(entry.path(), format!("{}/{}", services_dir, entry.file_name().to_string_lossy())).ok();
                        }
                    }
                }
            }
            println!("\x1b[32m✓\x1b[0m Snapshot '{}' created ({} services)", args.name, dag.services.len());
        }
        SnapshotCommand::List(_) => {
            if !Path::new(snap_dir).exists() { println!("No snapshots found"); return; }
            match fs::read_dir(snap_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            let state_file = entry.path().join("state.toml");
                            let desc = fs::read_to_string(&state_file).map(|c| c.lines().find(|l| l.starts_with("# Created:")).map(|l| l.trim_start_matches("# Created:").trim().to_string()).unwrap_or_default()).unwrap_or_default();
                            println!("  {:<20} {}", name, desc);
                        }
                    }
                }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m {}", e),
            }
        }
        SnapshotCommand::Restore(args) => {
            let snap_path = format!("{}/{}", snap_dir, args.name);
            if !Path::new(&snap_path).exists() { eprintln!("\x1b[31m✗\x1b[0m Snapshot '{}' not found", args.name); return; }
            if !args.force { eprintln!("\x1b[33m⚠\x1b[0m Use --force to confirm"); return; }
            let services_dir = format!("{}/services", snap_path);
            if let Ok(entries) = fs::read_dir(&services_dir) {
                let mut restored = 0;
                for entry in entries.flatten() {
                    let dest = format!("/etc/cesar/services/{}", entry.file_name().to_string_lossy());
                    fs::create_dir_all("/etc/cesar/services").ok();
                    if fs::copy(entry.path(), &dest).is_ok() { restored += 1; }
                }
                println!("\x1b[32m✓\x1b[0m Snapshot '{}' restored ({} services)", args.name, restored);
            }
        }
        SnapshotCommand::Delete(args) => {
            let snap_path = format!("{}/{}", snap_dir, args.name);
            if !args.force { eprintln!("\x1b[33m⚠\x1b[0m Use --force to confirm"); return; }
            match fs::remove_dir_all(&snap_path) {
                Ok(()) => println!("\x1b[32m✓\x1b[0m Snapshot '{}' deleted", args.name),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Delete failed: {}", e),
            }
        }
        SnapshotCommand::Diff(args) => {
            let from_path = format!("{}/{}/state.toml", snap_dir, args.from);
            let to_path = format!("{}/{}/state.toml", snap_dir, args.to);
            let from = fs::read_to_string(&from_path).unwrap_or_default();
            let to = fs::read_to_string(&to_path).unwrap_or_default();
            if from == to { println!("\x1b[32m✓\x1b[0m Snapshots are identical"); }
            else {
                let f: Vec<&str> = from.lines().collect();
                let t: Vec<&str> = to.lines().collect();
                for i in 0..f.len().max(t.len()) {
                    let a = f.get(i).unwrap_or(&""); let b = t.get(i).unwrap_or(&"");
                    if a != b { println!("\x1b[31m- {}\x1b[0m ({})", a, args.from); println!("\x1b[32m+ {}\x1b[0m ({})", b, args.to); }
                }
            }
        }
        SnapshotCommand::Export(args) => {
            let snap_path = format!("{}/{}", snap_dir, args.name);
            match process::Command::new("tar").args(["czf", &args.output, "-C", snap_dir, &args.name]).status() {
                Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Exported to '{}'", args.output),
                _ => {
                    fs::create_dir_all(&args.output).ok();
                    if let Ok(entries) = fs::read_dir(&snap_path) {
                        for entry in entries.flatten() { fs::copy(entry.path(), format!("{}/{}", args.output, entry.file_name().to_string_lossy())).ok(); }
                    }
                    println!("\x1b[32m✓\x1b[0m Exported to '{}'", args.output);
                }
            }
        }
        SnapshotCommand::Import(args) => {
            let name = args.name.unwrap_or_else(|| format!("import-{}", chrono::Local::now().format("%Y%m%d_%H%M%S")));
            let tmp_dir = format!("/tmp/cesar-import-{}", chrono::Local::now().format("%Y%m%d%H%M%S"));
            let final_dir = format!("{}/{}", snap_dir, name);
            if fs::create_dir_all(&tmp_dir).is_err() {
                eprintln!("\x1b[31m✗\x1b[0m Import failed: cannot create '{}'", tmp_dir);
                return;
            }
            match process::Command::new("tar").args(["xzf", &args.file, "-C", &tmp_dir]).status() {
                Ok(s) if s.success() => {
                    fs::remove_dir_all(&final_dir).ok();
                    if fs::rename(&tmp_dir, &final_dir).is_err() {
                        let _ = fs::remove_dir_all(&tmp_dir);
                        eprintln!("\x1b[31m✗\x1b[0m Import failed: cannot move extracted snapshot to '{}'", final_dir);
                        return;
                    }
                    println!("\x1b[32m✓\x1b[0m Imported as '{}'", name);
                }
                _ => {
                    let _ = fs::remove_dir_all(&tmp_dir);
                    match fs::read_dir(&args.file) {
                        Ok(entries) => {
                            fs::create_dir_all(&final_dir).ok();
                            for entry in entries.flatten() {
                                let _ = fs::copy(entry.path(), format!("{}/{}", final_dir, entry.file_name().to_string_lossy()));
                            }
                            println!("\x1b[32m✓\x1b[0m Imported as '{}'", name);
                        }
                        Err(e) => eprintln!("\x1b[31m✗\x1b[0m Import failed: {}", e),
                    }
                }
            }
        }
    }
}


fn handle_security(cmd: SecurityCommand, dag: &mut DagEngine, _logger: &CesarLogger) {
    match cmd {
        SecurityCommand::Audit(args) => {
            load_services_if_empty(dag);
            let mut issues = 0;
            println!("Security Audit Report\n{}", "─".repeat(50));
            if args.services || args.all || (!args.config && !args.network) {
                println!("\n[Service Security]");
                for (name, svc) in &dag.services {
                    let exec = &svc.config.exec;
                    if !Path::new(exec).exists() { println!("  \x1b[33m⚠\x1b[0m {}: Exec '{}' not found", name, exec); issues += 1; }
                    for dep in &svc.config.requires {
                        if !dag.services.contains_key(dep) { println!("  \x1b[31m✗\x1b[0m {}: Depends on undefined service '{}'", name, dep); issues += 1; }
                    }
                }
            }
            if args.config || args.all || (!args.services && !args.network) {
                println!("\n[Config Security]");
                for dir in &["/system/lib/cesar/services", "/etc/cesar/services"] {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("ini")
                                && let Ok(meta) = fs::metadata(&path) {
                                    use std::os::linux::fs::MetadataExt;
                                    if meta.st_uid() != 0 { println!("  \x1b[33m⚠\x1b[0m {}: Not root-owned (uid={})", path.display(), meta.st_uid()); issues += 1; }
                                }
                        }
                    }
                }
            }
            if args.network || args.all || (!args.services && !args.config) {
                println!("\n[Network Security]");
                if let Ok(net) = fs::read_to_string("/proc/net/tcp") {
                    let listening: Vec<&str> = net.lines().filter(|l| l.contains(" 0A ")).collect();
                    println!("  Listening TCP sockets: {}", listening.len());
                }
            }
            if issues == 0 { println!("\n\x1b[32m✓ No security issues found\x1b[0m"); }
            else { println!("\n\x1b[33m⚠ {} issue(s) found\x1b[0m", issues); }
        }
        SecurityCommand::Scan(_args) => {
            println!("Security Scan\n{}", "─".repeat(40));
            if let Ok(content) = fs::read_to_string("/proc/self/status")
                && let Some(line) = content.lines().find(|l| l.starts_with("Seccomp:")) {
                    let mode = line.split_whitespace().last().unwrap_or("0");
                    if mode == "0" { println!("  \x1b[33m⚠\x1b[0m Seccomp not active"); }
                    else { println!("  \x1b[32m✓\x1b[0m Seccomp active (mode={})", mode); }
                }
            if let Ok(meta) = fs::metadata("/etc/cesar") {
                use std::os::linux::fs::MetadataExt;
                let mode = meta.st_mode();
                if mode & 0o022 != 0 { println!("  \x1b[33m⚠\x1b[0m /etc/cesar is world-accessible ({:o})", mode & 0o7777); }
                else { println!("  \x1b[32m✓\x1b[0m /etc/cesar permissions OK"); }
            }
            println!("\x1b[32m✓ Scan complete\x1b[0m");
        }
        SecurityCommand::Policy(args) => {
            if args.show {
                let policy_path = "/etc/cesar/security.policy";
                if Path::new(policy_path).exists() { println!("{}", fs::read_to_string(policy_path).unwrap_or_default()); }
                else { println!("No policy configured. Default: standard"); }
            } else if let Some(ref p) = args.set {
                fs::write("/etc/cesar/security.policy", format!("policy = {}\n", p)).ok();
                println!("\x1b[32m✓\x1b[0m Policy set to '{}'", p);
            }
        }
        SecurityCommand::Cap(args) => {
            if args.list {
                let pid = args.pid.unwrap_or(1);
                if let Ok(content) = fs::read_to_string(format!("/proc/{}/status", pid)) {
                    for line in content.lines() {
                        if line.starts_with("Cap") { println!("  {}", line); }
                    }
                } else { eprintln!("\x1b[31m✗\x1b[0m Cannot read capabilities for PID {}", pid); }
            }
        }
        SecurityCommand::Seccomp(_args) => {
            println!("Seccomp status:");
            if let Ok(content) = fs::read_to_string("/proc/self/status")
                && let Some(line) = content.lines().find(|l| l.starts_with("Seccomp:")) {
                    println!("  {}", line);
                }
        }
        SecurityCommand::Sandbox(args) => {
            if args.list { println!("Active sandboxes: (none)"); }
        }
        SecurityCommand::Trust(args) => {
            if args.keys {
                let trust_dir = "/etc/cesar/trust";
                if let Ok(entries) = fs::read_dir(trust_dir) {
                    for entry in entries.flatten() { println!("  {}", entry.file_name().to_string_lossy()); }
                } else { println!("No trust keys"); }
            }
        }
    }
}


fn handle_query(cmd: QueryCommand, dag: &mut DagEngine, logger: &CesarLogger) {
    match cmd {
        QueryCommand::Service(args) => {
            load_services_if_empty(dag);
            if let Some(ref name) = args.name {
                if let Some(svc) = dag.services.get(name) {
                    if args.json { println!("{{\"name\":\"{}\",\"state\":\"{}\",\"pid\":{}}}", name, svc.state, svc.pid.map_or("null".to_string(), |p| p.to_string())); }
                    else { println!("{}: {} [{}]", name, svc.state, svc.pid.map_or("-".to_string(), |p| p.to_string())); }
                }
            } else {
                let count = dag.services.len();
                let running = dag.services.values().filter(|s| matches!(s.state, ServiceState::Running)).count();
                println!("{} services ({} running)", count, running);
            }
        }
        QueryCommand::Boot(args) => {
            let content = logger.read_log();
            if args.time { for line in content.lines() { if line.contains("Boot") { println!("{}", line.trim_start_matches('#').trim()); } } }
            else if args.errors { for line in content.lines() { if line.contains("CRITICAL") { println!("{}", line.trim_start_matches('>').trim()); } } }
            else { println!("Boot: OK"); }
        }
        QueryCommand::System(_args) => {
            println!("System: Cudane / Cesar v{}", env!("CARGO_PKG_VERSION"));
            let hostname = hostname::get().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
            println!("Hostname: {}", hostname);
            if let Ok(v) = fs::read_to_string("/proc/version") { println!("Kernel: {}", v.trim()); }
            if let Ok(u) = fs::read_to_string("/proc/uptime") { println!("Uptime: {}s", u.split_whitespace().next().unwrap_or("0")); }
        }
        QueryCommand::Dependency(args) => {
            load_services_if_empty(dag);
            let _ = dag.build_dependency_graph();
            if let Some(ref name) = args.name {
                if let Some(svc) = dag.services.get(name) {
                    println!("{} requires: {}", name, if svc.config.requires.is_empty() { "none".to_string() } else { svc.config.requires.join(", ") });
                }
            } else {
                let order = dag.get_boot_order();
                for (i, level) in order.iter().enumerate() { println!("  [{}] {}", i + 1, level.join(", ")); }
            }
        }
        QueryCommand::History(args) => {
            let content = logger.read_log();
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(args.limit);
            for line in &lines[start..] {
                if line.contains("EVENT:") || line.contains("INFO:") { println!("{}", line.trim_start_matches('>').trim()); }
            }
        }
        QueryCommand::Resource(_args) => {
            if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines().take(3) { println!("  {}", line); }
            }
        }
        QueryCommand::Health(_args) => {
            load_services_if_empty(dag);
            let total = dag.services.len();
            let running = dag.services.values().filter(|s| matches!(s.state, ServiceState::Running)).count();
            let failed = dag.services.values().filter(|s| matches!(s.state, ServiceState::Failed)).count();
            if failed == 0 { println!("\x1b[32m✓\x1b[0m Health: OK ({}/{} running)", running, total); }
            else { println!("\x1b[31m✗\x1b[0m Health: DEGRADED ({}/{} running, {} failed)", running, total, failed); }
        }
    }
}


fn handle_debug(cmd: DebugCommand, _dag: &mut DagEngine, _logger: &CesarLogger) {
    match cmd {
        DebugCommand::Trace(args) => {
            let ret = process::Command::new("strace").args(["-p", &args.pid.to_string()]).status();
            match ret {
                Ok(s) => { let _ = s; }
                Err(e) => {
                    eprintln!("\x1b[31m✗\x1b[0m strace not found: {}", e);
                    eprintln!("  Install strace: apt install strace / pacman -S strace");
                    eprintln!("  Alternative: cat /proc/{}/syscall", args.pid);
                    if let Ok(syscall) = fs::read_to_string(format!("/proc/{}/syscall", args.pid)) {
                        println!("Current syscall: {}", syscall.trim());
                    }
                }
            }
        }
        DebugCommand::Strace(args) => {
            let mut cmd = process::Command::new("strace");
            cmd.arg("-p").arg(args.pid.to_string());
            if let Some(ref filter) = args.filter { cmd.arg("-e").arg(filter); }
            if let Some(ref output) = args.output { cmd.arg("-o").arg(output); }
            let ret = cmd.status();
            match ret {
                Ok(s) => { let _ = s; }
                Err(e) => {
                    eprintln!("\x1b[31m✗\x1b[0m strace not found: {}", e);
                    eprintln!("  Install: apt install strace");
                }
            }
        }
        DebugCommand::Dump(_args) => {
            println!("System dump:");
            if let Ok(m) = fs::read_to_string("/proc/meminfo") { for line in m.lines().take(5) { println!("  {}", line); } }
            if let Ok(l) = fs::read_to_string("/proc/loadavg") { println!("  Load: {}", l.trim()); }
            if let Ok(s) = fs::read_to_string("/proc/stat") && let Some(cpu) = s.lines().next() { println!("  CPU: {}", cpu); }
        }
        DebugCommand::Core(args) => {
            let ret = process::Command::new("gdb").args(["-batch", "-ex", "thread apply all bt", &format!("--pid={}", args.pid)]).status();
            match ret {
                Ok(s) if s.success() => {}
                _ => {
                    eprintln!("\x1b[33m⚠\x1b[0m gdb not available or failed for PID {}", args.pid);
                    eprintln!("  Install: apt install gdb");
                    if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", args.pid)) {
                        for line in status.lines().take(10) { println!("  {}", line); }
                    }
                }
            }
        }
        DebugCommand::Profile(args) => {
            let ret = process::Command::new("perf").args(["record", "-p", &args.pid.to_string(), "-g", "--", "sleep", &args.duration.to_string()]).status();
            match ret {
                Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Profile recorded (perf.data)"),
                _ => {
                    eprintln!("\x1b[33m⚠\x1b[0m perf not available or failed");
                    eprintln!("  Install: apt install linux-tools-common");
                    eprintln!("  Alternative: cat /proc/{}/status", args.pid);
                    if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", args.pid)) {
                        for line in status.lines() { println!("  {}", line); }
                    }
                }
            }
            let _ = args;
        }
        DebugCommand::Stress(args) => {
            println!("Stress test: cpu={}, memory={}, io={}, duration={}s", args.cpu, args.memory, args.io, args.duration);
            if args.cpu {
                let _ = process::Command::new("stress").args(["--cpu", "4", "--timeout", &args.duration.to_string()]).status();
            }
            if args.memory {
                let _ = process::Command::new("stress").args(["--vm", "2", "--vm-bytes", "256M", "--timeout", &args.duration.to_string()]).status();
            }
            if args.io {
                let _ = process::Command::new("stress").args(["--io", "4", "--timeout", &args.duration.to_string()]).status();
            }
            if !args.cpu && !args.memory && !args.io {
                eprintln!("\x1b[33m⚠\x1b[0m No stress type selected. Use -c (cpu), -m (memory), -i (io)");
                eprintln!("  Install stress: apt install stress / pacman -S stress");
            }
        }
        DebugCommand::Test(_args) => {
            let mut passed = 0;
            let mut failed = 0;


            print!("Test config parsing... ");
            match config::parse_cesar_config("/system/lib/cesar/services/network.ini") {
                Ok(_) => { println!("\x1b[32m✓\x1b[0m"); passed += 1; }
                Err(e) => { println!("\x1b[33m⚠\x1b[0m (no config: {})", e); }
            }


            print!("Test DAG engine... ");
            let mut test_dag = DagEngine::new();
            test_dag.add_service(crate::service::ServiceConfig::new("test-a", "/bin/true"));
            test_dag.add_service(crate::service::ServiceConfig::new("test-b", "/bin/true"));
            match test_dag.build_dependency_graph() {
                Ok(()) => { println!("\x1b[32m✓\x1b[0m"); passed += 1; }
                Err(e) => { println!("\x1b[31m✗\x1b[0m {}", e); failed += 1; }
            }


            print!("Test service directory... ");
            let dirs = ["/system/lib/cesar/services", "/etc/cesar/services"];
            let mut found = false;
            for dir in &dirs {
                if Path::new(dir).exists() && fs::read_dir(dir).map(|e| e.count() > 0).unwrap_or(false) {
                    found = true;
                    break;
                }
            }
            if found { println!("\x1b[32m✓\x1b[0m"); passed += 1; }
            else { println!("\x1b[33m⚠\x1b[0m (no service dirs found)"); }


            print!("Test logger... ");
            let logger = CesarLogger::new();
            logger.log_info("test", "self-test message");
            let log = logger.read_log();
            if log.contains("self-test message") { println!("\x1b[32m✓\x1b[0m"); passed += 1; }
            else { println!("\x1b[31m✗\x1b[0m"); failed += 1; }

            println!("\n{} passed, {} failed", passed, failed);
            if failed > 0 { std::process::exit(1); }
        }
    }
}


fn handle_self(cmd: SelfCommand, _logger: &CesarLogger) {
    let version = env!("CARGO_PKG_VERSION");
    match cmd {
        SelfCommand::Status(args) => {
            let is_init = unsafe { libc::getpid() == 1 };
            println!("Cesar Init System v{}", version);
            println!("  PID:     {}", unsafe { libc::getpid() });
            println!("  Role:    {}", if is_init { "PID 1 (Init)" } else { "CLI Tool" });
            let uptime = fs::read_to_string("/proc/uptime").unwrap_or_default();
            let secs: f64 = uptime.split_whitespace().next().unwrap_or("0").parse().unwrap_or(0.0);
            println!("  Uptime:  {:.0}s", secs);
            if args.verbose {
                println!("  Binary:  /system/bin/csr");
                println!("  Config:  /etc/cesar/");
                println!("  Logs:    /var/log/cesar.md");
            }

            let report = health::HealthReport::run();
            println!();
            report.print_status();
        }
        SelfCommand::Update(args) => {
            if args.check {
                println!("Current version: {}", version);
                println!("Check: csr is built from source. No remote update available.");
                println!("  To update: git pull && cargo build --release");
            } else {
                println!("\x1b[33m⚠\x1b[0m Self-update not available for source-built binaries");
                println!("  Update manually: git pull && cargo build --release");
            }
        }
        SelfCommand::Version(args) => {
            if args.json { println!("{{\"name\":\"cesar\",\"version\":\"{}\"}}", version); }
            else if args.short { println!("{}", version); }
            else { println!("Cesar Init System v{}\nSovereign PID 1 for Cudane\nLicense: Unlicense", version); }
        }
        SelfCommand::Completions(args) => {
            let shell = args.shell.unwrap_or_else(|| {
                std::env::var("SHELL").unwrap_or_default()
                    .split('/').next_back().unwrap_or("bash").to_string()
            });
            println!("# Cesar completions for {}", shell);
            println!("# Source this file: source <(csr self completions -s {})", shell);
            match shell.as_str() {
                "bash" => {
                    println!("_cs() {{ local cur prev words cword; _init_completion || return; COMPREPLY=(); if [[ $cword -eq 1 ]]; then COMPREPLY=($(compgen -W \"service system config log socket daemon snapshot security query debug self plugin theme tui\" -- $cur)); fi; }}; complete -F _cs csr");
                }
                "zsh" => {
                    println!("#compdef csr");
                    println!("_cs() {{ _arguments '1:command:(service system config log socket daemon snapshot security query debug self plugin theme tui)' }}; compdef _cs csr");
                }
                "fish" => {
                    println!("complete -c csr -f -a '(service system config log socket daemon snapshot security query debug self plugin theme tui)'");
                }
                _ => println!("# Shell '{}' not fully supported. Basic completion:", shell),
            }
        }
        SelfCommand::Config(args) => {
            if args.show {
                println!("Cesar configuration:");
                println!("  Service dir (system): /system/lib/cesar/services/");
                println!("  Service dir (user):   /etc/cesar/services/");
                println!("  Log file:             /var/log/cesar.md");
            }
        }

    }
}


fn handle_service(cmd: ServiceCommand, dag: &mut DagEngine, logger: &CesarLogger) {
    match cmd {
        ServiceCommand::Start(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                if !svc.is_ready() {
                    eprintln!("\x1b[33m⚠\x1b[0m Service '{}' is in a transitional state ({})", args.name, svc.state);
                    return;
                }
                if !svc.config.requires.is_empty() && !dag.all_deps_satisfied(&args.name) {
                    eprintln!("\x1b[33m⚠\x1b[0m Dependencies not satisfied for '{}'", args.name);
                    eprintln!("  Fix: Start dependency services first, or use --force");
                    return;
                }
                let exec = svc.config.exec.clone();
                let name = args.name.clone();
                let env_vars = svc.config.environment.clone();
                let work_dir = svc.config.working_directory.clone();
                logger.log_service_event(&name, "Service starting");
                match cprocess::spawn_service_env(&name, &exec, &env_vars, work_dir.as_deref(), logger) {
                    Ok(pid) => {
                        dag.mark_running(&name, pid);
                        logger.log_service_event(&name, &format!("Service started (PID {})", pid));
                        println!("\x1b[32m✓\x1b[0m Service '{}' started (PID {})", name, pid);
                    }
                    Err(e) => {
                        dag.mark_failed(&name);
                        logger.log_service_event(&name, &format!("Service failed: {}", e));
                        eprintln!("\x1b[31m✗\x1b[0m Service '{}' failed: {}", name, e);
                        print_service_diagnostics(&name, &e, dag);
                    }
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name);
                eprintln!("  Check /etc/cesar/services/ or /system/lib/cesar/services/");
            }
        }
        ServiceCommand::Stop(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                if let Some(pid) = svc.pid {
                    dag.mark_stopping(&args.name);
                    logger.log_service_event(&args.name, &format!("Sending SIGTERM (PID {})", pid));
                    match cprocess::kill_service_group(pid, Signal::SIGTERM as i32) {
                        Ok(()) => {
                            dag.mark_stopped(&args.name);
                            logger.log_service_event(&args.name, "Service stopped");
                            println!("\x1b[32m✓\x1b[0m Service '{}' stopped", args.name);
                        }
                        Err(e) => {
                            eprintln!("\x1b[31m✗\x1b[0m {}", e);
                            if cprocess::check_process(pid) == cprocess::ProcessStatus::Unknown {
                                eprintln!("  Process {} no longer exists, cleaning up state", pid);
                                dag.mark_stopped(&args.name);
                            }
                        }
                    }
                } else {
                    println!("\x1b[33m⚠\x1b[0m Service '{}' is not running", args.name);
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name);
            }
        }
        ServiceCommand::Restart(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                if let Some(pid) = svc.pid {
                    dag.mark_stopping(&args.name);
                    logger.log_service_event(&args.name, &format!("Restart: sending SIGTERM (PID {})", pid));
                    let _ = cprocess::kill_service_group(pid, Signal::SIGTERM as i32);
                    dag.mark_stopped(&args.name);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                let exec = dag.services[&args.name].config.exec.clone();
                let env_vars = dag.services[&args.name].config.environment.clone();
                let work_dir = dag.services[&args.name].config.working_directory.clone();
                let name = args.name.clone();
                logger.log_service_event(&name, "Service starting (restart)");
                match cprocess::spawn_service_env(&name, &exec, &env_vars, work_dir.as_deref(), logger) {
                    Ok(pid) => {
                        dag.mark_running(&name, pid);
                        logger.log_service_event(&name, &format!("Service restarted (PID {})", pid));
                        println!("\x1b[32m✓\x1b[0m Service '{}' restarted (PID {})", name, pid);
                    }
                    Err(e) => {
                        dag.mark_failed(&name);
                        eprintln!("\x1b[31m✗\x1b[0m Restart failed: {}", e);
                        print_service_diagnostics(&name, &e, dag);
                    }
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name);
            }
        }
        ServiceCommand::Reload(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                if let Some(pid) = svc.pid {
                    if cprocess::check_process(pid) == cprocess::ProcessStatus::Alive {
                        let sig = match args.signal.as_deref() {
                            Some("TERM") | Some("15") => Signal::SIGTERM,
                            Some("KILL") | Some("9") => Signal::SIGKILL,
                            Some("USR1") | Some("10") => Signal::SIGUSR1,
                            Some("USR2") | Some("12") => Signal::SIGUSR2,
                            _ => Signal::SIGHUP,
                        };
                        dag.mark_reloading(&args.name);
                        logger.log_service_event(&args.name, &format!("Service reloading (signal: {})", sig as i32));
                        match cprocess::kill_service(pid, sig as i32) {
                            Ok(()) => {
                                dag.mark_running(&args.name, pid);
                                logger.log_service_event(&args.name, "Service reloaded");
                                println!("\x1b[32m✓\x1b[0m Reload signal ({}) sent to '{}' (PID {})", sig as i32, args.name, pid);
                            }
                            Err(e) => {
                                dag.mark_running(&args.name, pid);
                                eprintln!("\x1b[31m✗\x1b[0m Failed to send signal to '{}': {}", args.name, e);
                            }
                        }
                    } else {
                        eprintln!("\x1b[31m✗\x1b[0m Service '{}' process {} is not alive", args.name, pid);
                        dag.mark_stopped(&args.name);
                    }
                } else {
                    println!("\x1b[33m⚠\x1b[0m Service '{}' is not running (no PID)", args.name);
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name);
            }
        }
        ServiceCommand::Kill(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                if let Some(pid) = svc.pid {
                    let sig = match args.signal.as_deref() {
                        Some("KILL") | Some("9") => Signal::SIGKILL,
                        Some("TERM") | Some("15") => Signal::SIGTERM,
                        Some("HUP") | Some("1") => Signal::SIGHUP,
                        Some("USR1") | Some("10") => Signal::SIGUSR1,
                        Some("USR2") | Some("12") => Signal::SIGUSR2,
                        _ => Signal::SIGTERM,
                    };
                    match cprocess::kill_service_group(pid, sig as i32) {
                        Ok(()) => {
                            println!("\x1b[32m✓\x1b[0m Signal {} sent to '{}' (PID {})", sig as i32, args.name, pid);
                            if sig == Signal::SIGKILL || sig == Signal::SIGTERM {
                                if sig == Signal::SIGTERM {
                                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                                    while std::time::Instant::now() < deadline
                                        && cprocess::check_process(pid) == cprocess::ProcessStatus::Alive {
                                            std::thread::sleep(std::time::Duration::from_millis(100));
                                        }
                                }
                                dag.mark_stopped(&args.name);
                            }
                        }
                        Err(e) => eprintln!("\x1b[31m✗\x1b[0m {}", e),
                    }
                } else {
                    println!("\x1b[33m⚠\x1b[0m Service '{}' is not running (no PID)", args.name);
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name);
            }
        }
        ServiceCommand::Enable(args) => {
            let user_dir = "/etc/cesar/services";
            let enabled_dir = "/etc/cesar/enabled";
            let sys_dir = "/system/lib/cesar/services";
            let sys_path = format!("{}/{}.ini", sys_dir, args.name);
            let user_path = format!("{}/{}.ini", user_dir, args.name);
            let enabled_path = format!("{}/{}.ini", enabled_dir, args.name);
            let source = if Path::new(&user_path).exists() {
                Some(user_path.clone())
            } else if Path::new(&sys_path).exists() {
                Some(sys_path.clone())
            } else {
                None
            };
            if let Some(src) = source {
                fs::create_dir_all(enabled_dir).ok();
                if Path::new(&enabled_path).exists() {
                    println!("\x1b[33m⚠\x1b[0m Service '{}' is already enabled", args.name);
                } else {
                    match std::os::unix::fs::symlink(&src, &enabled_path) {
                        Ok(()) => println!("\x1b[32m✓\x1b[0m Service '{}' enabled", args.name),
                        Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to enable '{}': {}", args.name, e),
                    }
                }
                if args.now {
                    load_services_if_empty(dag);
                    if let Some(svc) = dag.services.get(&args.name) {
                        let exec = svc.config.exec.clone();
                        match cprocess::spawn_service(&args.name, &exec, logger) {
                            Ok(pid) => { dag.mark_running(&args.name, pid); println!("\x1b[32m✓\x1b[0m Service '{}' started (PID {})", args.name, pid); }
                            Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to start '{}': {}", args.name, e),
                        }
                    } else {
                        eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found after enabling", args.name);
                    }
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' config not found", args.name);
            }
        }
        ServiceCommand::Disable(args) => {
            if args.now {
                load_services_if_empty(dag);
                if let Some(svc) = dag.services.get(&args.name)
                    && let Some(pid) = svc.pid
                        && cprocess::check_process(pid) == cprocess::ProcessStatus::Alive {
                            dag.mark_stopping(&args.name);
                            println!("\x1b[33m⚠\x1b[0m Stopping '{}' (PID {})...", args.name, pid);
                            let _ = cprocess::kill_service_group(pid, libc::SIGTERM);
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            let _ = cprocess::kill_service_group(pid, libc::SIGKILL);
                            dag.mark_stopped(&args.name);
                            println!("\x1b[32m✓\x1b[0m Service '{}' stopped", args.name);
                        }
            }
            let enabled_path = format!("/etc/cesar/enabled/{}.ini", args.name);
            if Path::new(&enabled_path).exists() {
                match fs::remove_file(&enabled_path) {
                    Ok(()) => println!("\x1b[32m✓\x1b[0m Service '{}' disabled", args.name),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to disable '{}': {}", args.name, e),
                }
            } else {
                println!("\x1b[33m⚠\x1b[0m Service '{}' was not enabled", args.name);
            }
        }
        ServiceCommand::Status(args) => {
            load_services_if_empty(dag);
            if let Some(ref name) = args.name {
                if let Some(svc) = dag.services.get(name) {
                    if args.json {
                        println!("{{\"name\":\"{}\",\"state\":\"{}\",\"pid\":{}}}", name, svc.state, svc.pid.map_or("null".to_string(), |p| p.to_string()));
                    } else if !args.quiet {
                        let state_str = match svc.state {
                            ServiceState::Running => "\x1b[32mrunning\x1b[0m",
                            ServiceState::Failed => "\x1b[31mfailed\x1b[0m",
                            ServiceState::Starting => "\x1b[33mstarting\x1b[0m",
                            ServiceState::Reloading => "\x1b[33mreloading\x1b[0m",
                            ServiceState::Stopping => "\x1b[33mstopping\x1b[0m",
                            _ => "\x1b[90mstopped\x1b[0m",
                        };
                        println!("Service: {}", name);
                        println!("State:   {}", state_str);
                        println!("Exec:    {}", svc.config.exec);
                        println!("PID:     {}", svc.pid.map_or("-".to_string(), |p| p.to_string()));
                        println!("Deps:    {}", if svc.config.requires.is_empty() { "none".to_string() } else { svc.config.requires.join(", ") });
                        println!("Restart: {:?}", svc.config.restart);
                        let enabled_path = format!("/etc/cesar/enabled/{}.ini", name);
                        println!("Enabled: {}", if Path::new(&enabled_path).exists() { "yes" } else { "no" });
                        if let Some(pid) = svc.pid {
                            let alive = cprocess::check_process(pid) == cprocess::ProcessStatus::Alive;
                            println!("Process: {}", if alive { "\x1b[32malive\x1b[0m" } else { "\x1b[31mdead\x1b[0m" });
                            if !alive {
                                eprintln!("\x1b[33m⚠\x1b[0m Process {} is dead but state says running", pid);
                            }
                        }
                    }
                } else {
                    eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", name);
                }
            } else {
                visual::print_status(dag);
            }
        }
        ServiceCommand::List(args) => {
            load_services_if_empty(dag);
            if args.json {
                println!("[");
                let mut first = true;
                for (name, svc) in &dag.services {
                    let running = matches!(svc.state, ServiceState::Running);
                    if !args.all && !running && !matches!(svc.state, ServiceState::Failed) { continue; }
                    if args.failed && !matches!(svc.state, ServiceState::Failed) { continue; }
                    if args.running && !running { continue; }
                    if !first { println!(","); }
                    first = false;
                    print!("  {{\"name\":\"{}\",\"state\":\"{}\",\"pid\":{}}}", name, svc.state, svc.pid.map_or("null".to_string(), |p| p.to_string()));
                }
                println!("\n]");
            } else {
                for (name, svc) in &dag.services {
                    let running = matches!(svc.state, ServiceState::Running);
                    if !args.all && !running && !matches!(svc.state, ServiceState::Failed) { continue; }
                    if args.failed && !matches!(svc.state, ServiceState::Failed) { continue; }
                    if args.running && !running { continue; }
                    let icon = match svc.state {
                        ServiceState::Running => "\x1b[32m●\x1b[0m",
                        ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                        ServiceState::Starting => "\x1b[33m◌\x1b[0m",
                        ServiceState::Reloading => "\x1b[33m↻\x1b[0m",
                        ServiceState::Stopping => "\x1b[33m⏹\x1b[0m",
                        _ => "\x1b[90m○\x1b[0m",
                    };
                    let pid = svc.pid.map_or("-".to_string(), |p| format!("PID {}", p));
                    if args.minimal {
                        println!("{} {}", icon, name);
                    } else {
                        println!("{} {:<20} {:<12} {}", icon, name, format!("[{}]", svc.state), pid);
                    }
                }
            }
        }
        ServiceCommand::Inspect(args) => {
            load_services_if_empty(dag);
            if let Some(svc) = dag.services.get(&args.name) {
                if args.raw {
                    println!("Name:      {}", svc.config.name);
                    println!("Exec:      {}", svc.config.exec);
                    println!("State:     {}", svc.state);
                    println!("PID:       {}", svc.pid.map_or("-".to_string(), |p| p.to_string()));
                    println!("Restart:   {:?}", svc.config.restart);
                    println!("Socket:    {}", svc.config.socket.as_deref().unwrap_or("-"));
                    println!("Desc:      {}", svc.config.description.as_deref().unwrap_or("-"));
                    println!("Restarts:  {}", svc.restart_count);
                } else if args.deps {
                    println!("Dependencies for '{}':", args.name);
                    for dep in &svc.config.requires {
                        let dep_state = dag.get_service_state(dep).unwrap_or(ServiceState::Stopped);
                        let icon = match dep_state {
                            ServiceState::Running => "\x1b[32m●\x1b[0m",
                            ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                            _ => "\x1b[90m○\x1b[0m",
                        };
                        println!("  {} {} [{}]", icon, dep, dep_state);
                    }
                } else if args.pid {
                    match svc.pid {
                        Some(pid) => {
                            println!("PID:       {}", pid);
                            println!("State:     {}", svc.state);
                            let status = cprocess::check_process(pid);
                            println!("Process:   {}", match status {
                                cprocess::ProcessStatus::Alive => "\x1b[32malive\x1b[0m",
                                _ => "\x1b[31mdead\x1b[0m",
                            });
                        }
                        None => println!("No PID (not running)"),
                    }
                } else if args.json {
                    println!("{{\"name\":\"{}\",\"state\":\"{}\",\"exec\":\"{}\",\"pid\":{}}}",
                        args.name, svc.state, svc.config.exec,
                        svc.pid.map_or("null".to_string(), |p| p.to_string()));
                } else {
                    println!("Service: {}", args.name);
                    println!("State:   {}", svc.state);
                    println!("Exec:    {}", svc.config.exec);
                    println!("PID:     {}", svc.pid.map_or("-".to_string(), |p| p.to_string()));
                    println!("Deps:    {}", if svc.config.requires.is_empty() { "none".to_string() } else { svc.config.requires.join(", ") });
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name);
            }
        }
        ServiceCommand::Log(args) => {
            let log_content = logger.read_log();
            if let Some(ref name) = args.name {
                let mut in_section = false;
                let mut count = 0;
                for line in log_content.lines().rev() {
                    if line.starts_with(&format!("## [{}", name)) {
                        in_section = true;
                        println!("{}", line);
                        count += 1;
                    } else if line.starts_with("## [") {
                        in_section = false;
                    } else if in_section && count < args.lines {
                        println!("{}", line);
                    }
                }
            } else {
                let lines: Vec<&str> = log_content.lines().collect();
                let start = lines.len().saturating_sub(args.lines);
                for line in &lines[start..] {
                    println!("{}", line);
                }
            }
        }
        ServiceCommand::Cat(args) => {
            let user_path = format!("/etc/cesar/services/{}.ini", args.name);
            let sys_path = format!("/system/lib/cesar/services/{}.ini", args.name);
            if Path::new(&user_path).exists() {
                match fs::read_to_string(&user_path) {
                    Ok(c) => println!("{}\n(from {})", c, user_path),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to read {}: {}", user_path, e),
                }
            } else if Path::new(&sys_path).exists() {
                match fs::read_to_string(&sys_path) {
                    Ok(c) => println!("{}\n(from {})", c, sys_path),
                    Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to read {}: {}", sys_path, e),
                }
            } else {
                eprintln!("\x1b[31m✗\x1b[0m Config for '{}' not found", args.name);
            }
        }
        ServiceCommand::Edit(args) => {
            let path = format!("/etc/cesar/services/{}.ini", args.name);
            if !Path::new(&path).exists() {
                eprintln!("\x1b[31m✗\x1b[0m Config for '{}' not found at {}", args.name, path);
                return;
            }
            let editor = args.editor.unwrap_or_else(|| std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()));
            let status = process::Command::new(&editor).arg(&path).status();
            match status {
                Ok(s) if s.success() => println!("\x1b[32m✓\x1b[0m Config edited"),
                Ok(s) => eprintln!("\x1b[31m✗\x1b[0m Editor exited with {}", s),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed to launch {}: {}", editor, e),
            }
        }
        ServiceCommand::Diff(args) => {
            load_services_if_empty(dag);
            let user_path = format!("/etc/cesar/services/{}.ini", args.name);
            let sys_path = format!("/system/lib/cesar/services/{}.ini", args.name);
            let disk_content = fs::read_to_string(&user_path).or_else(|_| fs::read_to_string(&sys_path));
            match disk_content {
                Ok(disk) => {
                    if let Some(svc) = dag.services.get(&args.name) {
                        let env_str = svc.config.environment.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(" ");
                        let running_config = format!(
                            "[Service]\nName = {}\nExec = {}\nRequires = {}\nRestart = {:?}\nSocket = {}\nDescription = {}\nEnvironment = {}\nWorkingDirectory = {}\n",
                            svc.config.name, svc.config.exec, svc.config.requires.join(", "), svc.config.restart,
                            svc.config.socket.as_deref().unwrap_or(""),
                            svc.config.description.as_deref().unwrap_or(""),
                            env_str,
                            svc.config.working_directory.as_deref().unwrap_or("")
                        );
                        if disk.trim() == running_config.trim() {
                            println!("\x1b[32m✓\x1b[0m Running config matches on-disk config for '{}'", args.name);
                        } else {
                            println!("Difference for '{}' (running vs on-disk):", args.name);
                            let disk_lines: Vec<&str> = disk.lines().collect();
                            let running_lines: Vec<&str> = running_config.lines().collect();
                            let max = disk_lines.len().max(running_lines.len());
                            for i in 0..max {
                                let d = disk_lines.get(i).unwrap_or(&"");
                                let r = running_lines.get(i).unwrap_or(&"");
                                if d != r {
                                    println!("\x1b[31m- {}\x1b[0m (on-disk)", d);
                                    println!("\x1b[32m+ {}\x1b[0m (running)", r);
                                }
                            }
                        }
                    }
                }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Cannot read on-disk config: {}", e),
            }
        }
        ServiceCommand::Validate(_args) => {
            let dirs = ["/system/lib/cesar/services", "/etc/cesar/services"];
            let mut valid = 0;
            let mut invalid = 0;
            for dir in &dirs {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                            match config::parse_cesar_config(path.to_str().unwrap_or_default()) {
                                Ok(cfg) => {
                                    if !Path::new(&cfg.exec).exists() {
                                        println!("\x1b[33m⚠\x1b[0m {} — valid syntax, Exec '{}' not found", path.display(), cfg.exec);
                                    } else {
                                        println!("\x1b[32m✓\x1b[0m {}", path.display());
                                    }
                                    valid += 1;
                                }
                                Err(e) => {
                                    invalid += 1;
                                    eprintln!("\x1b[31m✗\x1b[0m {}: {}", path.display(), e);
                                }
                            }
                        }
                    }
                }
            }
            println!("\n{} valid, {} invalid", valid, invalid);
        }
        ServiceCommand::Create(args) => {
            let dir = "/etc/cesar/services";
            fs::create_dir_all(dir).ok();
            let path = format!("{}/{}.ini", dir, args.name);
            if Path::new(&path).exists() {
                eprintln!("\x1b[33m⚠\x1b[0m Service '{}' already exists at {}", args.name, path);
                return;
            }
            let requires = args.requires.unwrap_or_default();
            let restart = args.restart.unwrap_or_else(|| "never".to_string());
            let mut content = format!("[Service]\nName = {}\nExec = {}\nRestart = {}", args.name, args.exec, restart);
            if !requires.is_empty() { content.push_str(&format!("\nRequires = {}", requires)); }
            if let Some(ref desc) = args.description { content.push_str(&format!("\nDescription = {}", desc)); }
            if let Some(ref sock) = args.socket { content.push_str(&format!("\nSocket = {}", sock)); }
            content.push('\n');
            match fs::write(&path, &content) {
                Ok(()) => println!("\x1b[32m✓\x1b[0m Service '{}' created at {}", args.name, path),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Failed: {}", e),
            }
        }
        ServiceCommand::Rm(args) => {
            let path = format!("/etc/cesar/services/{}.ini", args.name);
            if !Path::new(&path).exists() {
                eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found at {}", args.name, path);
                return;
            }
            if !args.force
                && let Some(svc) = dag.services.get(&args.name)
                    && matches!(svc.state, ServiceState::Running) {
                        eprintln!("\x1b[31m✗\x1b[0m Service '{}' is running. Stop first or use --force", args.name);
                        return;
                    }
            match fs::remove_file(&path) {
                Ok(()) => {
                    println!("\x1b[32m✓\x1b[0m Service '{}' removed", args.name);
                    let enabled_path = format!("/etc/cesar/enabled/{}.ini", args.name);
                    fs::remove_file(enabled_path).ok();
                }
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m {}", e),
            }
        }
        ServiceCommand::Monitor(args) => {
            load_services_if_empty(dag);
            println!("Monitoring '{}' (Ctrl+C to stop, interval={}ms)...", args.name, args.interval);
            let mut consecutive_failures = 0u32;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(args.interval));
                let state = dag.services.get(&args.name).map(|svc| {
                    let process_alive = svc.pid.map(|pid| cprocess::check_process(pid) == cprocess::ProcessStatus::Alive).unwrap_or(false);
                    (svc.state.clone(), svc.pid, process_alive)
                });
                match state {
                    Some((ref svc_state, pid, alive)) => {
                        let ts = chrono::Local::now().format("%H:%M:%S");
                        let state_icon = match svc_state {
                            ServiceState::Running if alive => "\x1b[32m●\x1b[0m",
                            ServiceState::Running if !alive => "\x1b[31m✗\x1b[0m",
                            ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                            _ => "\x1b[33m◌\x1b[0m",
                        };
                        println!("[{}] {} {} [{}] PID={}", ts, state_icon, args.name, svc_state, pid.map_or("-".to_string(), |p| p.to_string()));
                        if matches!(svc_state, ServiceState::Running) && !alive {
                            consecutive_failures += 1;
                            if consecutive_failures >= args.threshold {
                                eprintln!("\x1b[31m⚠ ALERT:\x1b[0m '{}' dead for {} checks", args.name, consecutive_failures);
                            }
                        } else {
                            consecutive_failures = 0;
                        }
                    }
                    None => { eprintln!("\x1b[31m✗\x1b[0m Service '{}' not found", args.name); break; }
                }
            }
        }
        ServiceCommand::Watch(_args) => {
            println!("Watching log for events (Ctrl+C to stop)...");
            let log_path = "/var/log/cesar.md";
            let mut last_len = fs::metadata(log_path).map(|m| m.len() as usize).unwrap_or(0);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if let Ok(content) = fs::read_to_string(log_path)
                    && content.len() > last_len {
                        let new_content = &content[last_len..];
                        for line in new_content.lines() {
                            if line.contains("EVENT:") || line.contains("CRITICAL") || line.contains("WARNING") {
                                println!("{}", line.trim_start_matches('>').trim());
                            }
                        }
                        last_len = content.len();
                    }
            }
        }
        ServiceCommand::Tree(args) => {
            load_services_if_empty(dag);
            let _ = dag.build_dependency_graph();
            let order = dag.get_boot_order();
            if args.flat {
                for level in &order { for name in level { println!("{}", name); } }
            } else {
                println!("Cesar Service Dependency Tree");
                println!("{}", "─".repeat(40));
                for (level_idx, level) in order.iter().enumerate() {
                    for name in level {
                        let deps = &dag.services[name].config.requires;
                        let state = dag.get_service_state(name).unwrap_or(ServiceState::Stopped);
                        let icon = match state { ServiceState::Running => "\x1b[32m●\x1b[0m", ServiceState::Failed => "\x1b[31m✗\x1b[0m", _ => "\x1b[90m○\x1b[0m" };
                        if deps.is_empty() {
                            println!("  [{}] {} {} (no dependencies)", level_idx + 1, icon, name);
                        } else {
                            println!("  [{}] {} {} (requires: {})", level_idx + 1, icon, name, deps.join(", "));
                        }
                    }
                }
            }
        }
    }
}

fn handle_plugin(cmd: PluginCommand) {
    match cmd {
        PluginCommand::List => {
            let plugins = crate::python::plugin::PluginManager::list();
            if plugins.is_empty() {
                println!("No plugins installed.");
                println!("  Install: csr plugin install <path>");
                return;
            }
            for p in &plugins {
                println!("\x1b[32m{}\x1b[0m", p.name);
                println!("  Path:    {}", p.path);
                if !p.aliases.is_empty() {
                    println!("  Aliases:");
                    for (alias, cmd_str) in &p.aliases {
                        println!("    {}  →  {}", alias, cmd_str);
                    }
                }
                println!();
            }
        }
        PluginCommand::Run(args) => {
            let (entry, func) = match crate::python::plugin::PluginManager::by_alias(&args.alias) {
                Some(found) => found,
                None => {
                    eprintln!("\x1b[31m✗\x1b[0m No plugin alias '{}' found", args.alias);
                    return;
                }
            };
            match crate::python::plugin::PluginManager::run(&entry, &func, &args.args) {
                Ok(output) => println!("{}", output),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Plugin '{}' failed: {}", entry.name, e),
            }
        }
        PluginCommand::Install(args) => {
            let src = Path::new(&args.path);
            if !src.exists() {
                eprintln!("\x1b[31m✗\x1b[0m File not found: {}", args.path);
                return;
            }
            if !src.is_file() {
                eprintln!("\x1b[31m✗\x1b[0m Not a file: {}", args.path);
                return;
            }

            let name = args.name.unwrap_or_else(|| {
                src.file_stem().unwrap_or_default().to_string_lossy().to_string()
            });

            let plugins_dir = Path::new("/etc/cesar/plugins");
            if !plugins_dir.exists() {
                let _ = fs::create_dir_all(plugins_dir);
            }

            let dest = plugins_dir.join(src.file_name().unwrap_or_default());
            if dest.exists() && !args.force {
                eprintln!("\x1b[33m⚠\x1b[0m Plugin '{}' already exists at {}", name, dest.display());
                eprintln!("  Use --force to overwrite");
                return;
            }

            if let Err(e) = fs::copy(src, &dest) {
                eprintln!("\x1b[31m✗\x1b[0m Failed to copy plugin: {}", e);
                return;
            }

            let mut aliases: HashMap<String, String> = args.aliases.clone().into_iter().collect();
            if let Some(alias) = args.alias {
                aliases.insert(alias.clone(), format!("{} {{}}", dest.display()));
            }

            crate::python::plugin::PluginManager::register(&name, &dest, &aliases);

            println!("\x1b[32m✓\x1b[0m Plugin '{}' installed", name);
            println!("  Path: {}", dest.display());
            if !aliases.is_empty() {
                println!("  Aliases:");
                for (a, c) in &aliases {
                    println!("    {}  →  {}", a, c);
                }
            }
        }
        PluginCommand::Remove(args) => {
            let plugins = crate::python::plugin::PluginManager::list();
            let entry = match plugins.iter().find(|p| p.name == args.name) {
                Some(e) => e,
                None => {
                    eprintln!("\x1b[31m✗\x1b[0m Plugin '{}' not found", args.name);
                    return;
                }
            };
            if let Err(e) = fs::remove_file(&entry.path) {
                eprintln!("\x1b[33m⚠\x1b[0m Could not remove file: {}", e);
            }
            crate::python::plugin::PluginManager::unregister(&args.name);
            println!("\x1b[32m✓\x1b[0m Plugin '{}' removed", args.name);
        }
        PluginCommand::Info(args) => {
            match crate::python::plugin::PluginManager::by_name(&args.name) {
                Some(p) => {
                    println!("\x1b[32m{}\x1b[0m", p.name);
                    println!("  Path:    {}", p.path);
                    let path = Path::new(&p.path);
                    if path.exists() {
                        let meta = fs::metadata(path).ok();
                        if let Some(m) = meta {
                            println!("  Size:    {} bytes", m.len());
                        }
                    }
                    if !p.aliases.is_empty() {
                        println!("  Aliases:");
                        for (alias, cmd) in &p.aliases {
                            println!("    {}  →  {}", alias, cmd);
                        }
                    }
                }
                None => eprintln!("\x1b[31m✗\x1b[0m Plugin '{}' not found", args.name),
            }
        }
    }
}

fn handle_theme(cmd: ThemeCommand) {
    match cmd {
        ThemeCommand::List => {
            let themes = crate::python::theme::ThemeEngine::list();
            if themes.is_empty() {
                println!("No themes installed.");
                println!("  Install: csr theme install <path>");
                return;
            }
            for t in &themes {
                println!("\x1b[32m{}\x1b[0m", t.name);
                println!("  Path: {}", t.path);
                if !t.description.is_empty() {
                    println!("  Desc: {}", t.description);
                }
                println!();
            }
        }
        ThemeCommand::Apply(args) => {
            let theme = match crate::python::theme::ThemeEngine::by_name(&args.name) {
                Some(t) => t,
                None => {
                    eprintln!("\x1b[31m✗\x1b[0m Theme '{}' not found", args.name);
                    return;
                }
            };
            match crate::python::theme::ThemeEngine::apply(&theme) {
                Ok(output) => println!("{}", output),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m Theme '{}' failed: {}", theme.name, e),
            }
        }
        ThemeCommand::Install(args) => {
            let src = Path::new(&args.path);
            if !src.exists() {
                eprintln!("\x1b[31m✗\x1b[0m File not found: {}", args.path);
                return;
            }
            if !src.is_file() {
                eprintln!("\x1b[31m✗\x1b[0m Not a file: {}", args.path);
                return;
            }

            let name = args.name.unwrap_or_else(|| {
                src.file_stem().unwrap_or_default().to_string_lossy().to_string()
            });

            let themes_dir = Path::new("/etc/cesar/themes");
            if !themes_dir.exists() {
                let _ = fs::create_dir_all(themes_dir);
            }

            let dest = themes_dir.join(src.file_name().unwrap_or_default());
            if dest.exists() && !args.force {
                eprintln!("\x1b[33m⚠\x1b[0m Theme '{}' already exists at {}", name, dest.display());
                eprintln!("  Use --force to overwrite");
                return;
            }

            if let Err(e) = fs::copy(src, &dest) {
                eprintln!("\x1b[31m✗\x1b[0m Failed to copy theme: {}", e);
                return;
            }

            crate::python::theme::ThemeEngine::register(&name, &dest);

            println!("\x1b[32m✓\x1b[0m Theme '{}' installed", name);
            println!("  Path: {}", dest.display());
        }
        ThemeCommand::Remove(args) => {
            let themes = crate::python::theme::ThemeEngine::list();
            let entry = match themes.iter().find(|t| t.name == args.name) {
                Some(e) => e,
                None => {
                    eprintln!("\x1b[31m✗\x1b[0m Theme '{}' not found", args.name);
                    return;
                }
            };
            if let Err(e) = fs::remove_file(&entry.path) {
                eprintln!("\x1b[33m⚠\x1b[0m Could not remove file: {}", e);
            }
            crate::python::theme::ThemeEngine::unregister(&args.name);
            println!("\x1b[32m✓\x1b[0m Theme '{}' removed", args.name);
        }
        ThemeCommand::Info(args) => {
            match crate::python::theme::ThemeEngine::by_name(&args.name) {
                Some(t) => {
                    println!("\x1b[32m{}\x1b[0m", t.name);
                    println!("  Path: {}", t.path);
                    if !t.description.is_empty() {
                        println!("  Desc: {}", t.description);
                    }
                    let path = Path::new(&t.path);
                    if path.exists() {
                        let meta = fs::metadata(path).ok();
                        if let Some(m) = meta {
                            println!("  Size:    {} bytes", m.len());
                        }
                    }
                }
                None => eprintln!("\x1b[31m✗\x1b[0m Theme '{}' not found", args.name),
            }
        }
    }
}

fn handle_tui(cmd: TuiCommand) {
    match cmd {
        TuiCommand::List => {
            let tuis = crate::python::tui::TuiEngine::list();
            if tuis.is_empty() {
                println!("No TUIs installed.");
                println!("  Install: csr tui install <path>");
                return;
            }
            for t in &tuis {
                println!("\x1b[32m{}\x1b[0m", t.name);
                println!("  Path: {}", t.path);
                if !t.description.is_empty() {
                    println!("  Desc: {}", t.description);
                }
                println!();
            }
        }
        TuiCommand::Apply(args) => {
            let tui = match crate::python::tui::TuiEngine::by_name(&args.name) {
                Some(t) => t,
                None => {
                    eprintln!("\x1b[31m✗\x1b[0m TUI '{}' not found", args.name);
                    return;
                }
            };
            match crate::python::tui::TuiEngine::apply(&tui) {
                Ok(output) => println!("{}", output),
                Err(e) => eprintln!("\x1b[31m✗\x1b[0m TUI '{}' failed: {}", tui.name, e),
            }
        }
        TuiCommand::Install(args) => {
            let src = Path::new(&args.path);
            if !src.exists() {
                eprintln!("\x1b[31m✗\x1b[0m File not found: {}", args.path);
                return;
            }
            if !src.is_file() {
                eprintln!("\x1b[31m✗\x1b[0m Not a file: {}", args.path);
                return;
            }

            let name = args.name.unwrap_or_else(|| {
                src.file_stem().unwrap_or_default().to_string_lossy().to_string()
            });

            let tuis_dir = Path::new("/etc/cesar/tuis");
            if !tuis_dir.exists() {
                let _ = fs::create_dir_all(tuis_dir);
            }

            let dest = tuis_dir.join(src.file_name().unwrap_or_default());
            if dest.exists() && !args.force {
                eprintln!("\x1b[33m⚠\x1b[0m TUI '{}' already exists at {}", name, dest.display());
                eprintln!("  Use --force to overwrite");
                return;
            }

            if let Err(e) = fs::copy(src, &dest) {
                eprintln!("\x1b[31m✗\x1b[0m Failed to copy TUI: {}", e);
                return;
            }

            crate::python::tui::TuiEngine::register(&name, &dest);

            println!("\x1b[32m✓\x1b[0m TUI '{}' installed", name);
            println!("  Path: {}", dest.display());
        }
        TuiCommand::Remove(args) => {
            let tuis = crate::python::tui::TuiEngine::list();
            let entry = match tuis.iter().find(|t| t.name == args.name) {
                Some(e) => e,
                None => {
                    eprintln!("\x1b[31m✗\x1b[0m TUI '{}' not found", args.name);
                    return;
                }
            };
            if let Err(e) = fs::remove_file(&entry.path) {
                eprintln!("\x1b[33m⚠\x1b[0m Could not remove file: {}", e);
            }
            crate::python::tui::TuiEngine::unregister(&args.name);
            println!("\x1b[32m✓\x1b[0m TUI '{}' removed", args.name);
        }
        TuiCommand::Info(args) => {
            match crate::python::tui::TuiEngine::by_name(&args.name) {
                Some(t) => {
                    println!("\x1b[32m{}\x1b[0m", t.name);
                    println!("  Path: {}", t.path);
                    if !t.description.is_empty() {
                        println!("  Desc: {}", t.description);
                    }
                    let path = Path::new(&t.path);
                    if path.exists() {
                        let meta = fs::metadata(path).ok();
                        if let Some(m) = meta {
                            println!("  Size:    {} bytes", m.len());
                        }
                    }
                }
                None => eprintln!("\x1b[31m✗\x1b[0m TUI '{}' not found", args.name),
            }
        }
    }
}
