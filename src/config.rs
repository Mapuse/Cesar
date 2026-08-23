use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;

use crate::service::{RestartPolicy, ServiceConfig};

const SYSTEM_SERVICE_DIR: &str = "/system/lib/cesar/services";
const USER_SERVICE_DIR: &str = "/etc/cesar/services";
const ENABLED_SERVICE_DIR: &str = "/etc/cesar/enabled";

pub fn parse_cesar_config(file_path: &str) -> io::Result<ServiceConfig> {
    let path = Path::new(file_path);
    let content = fs::read_to_string(path)?;
    parse_config_content(&content, file_path)
}

fn parse_config_content(content: &str, source: &str) -> io::Result<ServiceConfig> {
    let mut name = String::new();
    let mut exec = String::new();
    let mut requires = Vec::new();
    let mut restart = String::new();
    let mut socket: Option<String> = None;
    let mut description: Option<String> = None;
    let mut environment: Vec<(String, String)> = Vec::new();
    let mut working_directory: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') || trimmed.starts_with('[') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = unquote(value.trim());

            match key {
                "Name" => name = value.to_string(),
                "Exec" => exec = value.to_string(),
                "Requires" => {
                    requires = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "Restart" => restart = value.to_string(),
                "Socket" => socket = Some(value.to_string()),
                "Description" => description = Some(value.to_string()),
                "Environment" => {
                    for part in value.split_whitespace() {
                        if let Some((k, v)) = part.split_once('=') {
                            environment.push((k.to_string(), v.to_string()));
                        }
                    }
                }
                "WorkingDirectory" => working_directory = Some(value.to_string()),
                _ => {
                    eprintln!("[Warning] :: Unknown key '{}' in {} ignored.", key, source);
                }
            }
        }
        // Malformed lines without '=' are ignored; section headers and
        // comments carry no keys.
    }

    if name.is_empty() || exec.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Missing required Name or Exec in {}", source),
        ));
    }
    if let Err(problem) = validate_service_name(&name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid Name '{}' in {}: {}", name, source, problem),
        ));
    }

    Ok(ServiceConfig {
        name,
        exec,
        requires,
        restart: RestartPolicy::parse(&restart),
        socket,
        description,
        environment,
        working_directory,
    })
}

/// Strip one layer of matching single or double quotes from a config value.
fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Service names become file names, log sections and socket paths: keep
/// them to shell-safe characters. Returns the first problem found.
pub fn validate_service_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    for c in name.chars() {
        if c.is_whitespace() {
            return Err(format!("whitespace character {:?} not allowed", c));
        }
        if c.is_control() {
            return Err(format!("control character {:?} not allowed", c));
        }
        if matches!(c, '/' | '\\' | '\0' | '$' | '`' | ';' | '&' | '|' | '>' | '<') {
            return Err(format!("character {:?} not allowed", c));
        }
    }
    Ok(())
}

pub fn load_all_services() -> Vec<ServiceConfig> {
    let mut services_by_name: HashMap<String, ServiceConfig> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    if let Ok(entries) = fs::read_dir(SYSTEM_SERVICE_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                match parse_cesar_config(path.to_str().unwrap_or_default()) {
                    Ok(cfg) => {
                        let key = cfg.name.clone();
                        order.push(key.clone());
                        services_by_name.insert(key, cfg);
                    }
                    Err(e) => eprintln!("[Error] :: Failed to parse {:?}: {}", path, e),
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(USER_SERVICE_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ini") {
                match parse_cesar_config(path.to_str().unwrap_or_default()) {
                    Ok(cfg) => {
                        let key = cfg.name.clone();
                        if !services_by_name.contains_key(&key) {
                            order.push(key.clone());
                        }
                        services_by_name.insert(key, cfg);
                    }
                    Err(e) => eprintln!("[Error] :: Failed to parse {:?}: {}", path, e),
                }
            }
        }
    }

    let enabled: Option<HashSet<String>> = fs::read_dir(ENABLED_SERVICE_DIR)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    file_name.strip_suffix(".ini").map(|s| s.to_string())
                })
                .collect()
        })
        // An existing-but-empty enabled directory carries no allowlist:
        // treat it as absent so every discovered service still loads.
        // (Creating the directory must never silently disable all boot services.)
        .filter(|set: &HashSet<String>| !set.is_empty());

    order
        .into_iter()
        .filter(|k| match &enabled {
            Some(set) => set.contains(k),
            None => true,
        })
        .filter_map(|k| services_by_name.remove(&k))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(content: &str) -> io::Result<ServiceConfig> {
        parse_config_content(content, "test.ini")
    }

    #[test]
    fn parses_basic_service() {
        let cfg = parse("[Service]\nName = web\nExec = /usr/bin/nginx\n").expect("parses");
        assert_eq!(cfg.name, "web");
        assert_eq!(cfg.exec, "/usr/bin/nginx");
        assert_eq!(cfg.restart, RestartPolicy::Never);
        assert!(cfg.requires.is_empty());
    }

    #[test]
    fn comments_and_sections_are_skipped() {
        let content = "\
# leading comment
[Service]
; ini-style comment too
Name = svc
Exec = /bin/true
[Unit]
NotAKeyWeKnow = whatever
";
        let cfg = parse(content).expect("parses");
        assert_eq!(cfg.name, "svc");
    }

    #[test]
    fn quoted_values_are_unquoted_once() {
        let cfg = parse("Name = \"web\"\nExec = '/usr/bin/ng inx'\n").expect("parses");
        assert_eq!(cfg.name, "web");
        assert_eq!(cfg.exec, "/usr/bin/ng inx");
    }

    #[test]
    fn malformed_lines_without_equals_are_ignored() {
        let cfg = parse("Name = svc\nExec = /bin/true\nthis line is broken\n").expect("parses");
        assert_eq!(cfg.name, "svc");
    }

    #[test]
    fn missing_name_or_exec_is_rejected() {
        assert!(parse("Name = only-name\n").is_err());
        assert!(parse("Exec = /bin/true\n").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn requires_list_splits_on_commas() {
        let cfg = parse("Name = app\nExec = x\nRequires = db, cache ,, net\n").expect("parses");
        assert_eq!(cfg.requires, vec!["db", "cache", "net"]);
    }

    #[test]
    fn environment_pairs_are_keyed() {
        let cfg = parse("Name = e\nExec = x\nEnvironment = A=1 B=two\n").expect("parses");
        assert_eq!(
            cfg.environment,
            vec![
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "two".to_string()),
            ]
        );
    }

    #[test]
    fn service_names_with_whitespace_or_control_chars_rejected() {
        assert!(validate_service_name("my service").is_err());
        assert!(validate_service_name("svc\ttab").is_err());
        assert!(validate_service_name("bad\nname").is_err());
        assert!(validate_service_name("a/b").is_err());
        assert!(validate_service_name("sub;shell").is_err());
        assert!(validate_service_name("").is_err());
        assert!(validate_service_name("ok.name-1_2").is_ok());
        let parsed = parse("Name = bad name\nExec = x\n");
        assert!(parsed.is_err(), "parser must enforce the same rule");
    }

    #[test]
    fn working_directory_and_socket_captured() {
        let cfg = parse(
            "Name = s\nExec = x\nWorkingDirectory = /srv\nSocket = unix:/run/s.sock\n",
        )
        .expect("parses");
        assert_eq!(cfg.working_directory.as_deref(), Some("/srv"));
        assert_eq!(cfg.socket.as_deref(), Some("unix:/run/s.sock"));
    }
}
