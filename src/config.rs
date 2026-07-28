use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::service::{RestartPolicy, ServiceConfig};

const SYSTEM_SERVICE_DIR: &str = "/system/lib/cesar/services";
const USER_SERVICE_DIR: &str = "/etc/cesar/services";

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

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

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
    }

    if name.is_empty() || exec.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Missing required Name or Exec in {}", source),
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

    order.iter().filter_map(|k| services_by_name.remove(k)).collect()
}
