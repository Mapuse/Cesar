use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, Once, OnceLock};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use crate::settings::schema::PythonConfig;
use super::expand_tilde;

#[derive(serde::Deserialize)]
struct PluginDescConfig {
    #[serde(rename = "plugin")]
    plugins: HashMap<String, PluginDescEntry>,
}

#[derive(serde::Deserialize)]
struct PluginDescEntry {
    name: String,
    path: String,
    #[serde(default)]
    aliases: HashMap<String, String>,
}

pub struct PluginManager {
    plugins: Vec<LoadedPlugin>,
}

struct LoadedPlugin {
    name: String,
    module: PyObject,
    hooks: Vec<String>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: vec![] }
    }

    pub fn load_all(&mut self, cfg: &PythonConfig) {
        if cfg.plugins.is_empty() { return; }
        let plugins_result: PyResult<Vec<(String, PyObject, Vec<String>)>> = Python::with_gil(|py| {
            let mut loaded = Vec::new();
            let sys_path = py.import("sys")?.getattr("path")?;
            for plugin_path in &cfg.plugins {
                let path = expand_tilde(plugin_path);
                let std_path = std::path::PathBuf::from(&path);
                if !std_path.exists() {
                    eprintln!("[Warning] :: plugin not found: {}", path);
                    continue;
                }
                let parent = match std_path.parent() {
                    Some(p) => p.to_str().unwrap_or(".").to_string(),
                    None => { eprintln!("[Warning] :: cannot determine parent of {}", path); continue; }
                };
                let file_stem = match std_path.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => { eprintln!("[Warning] :: cannot determine name of {}", path); continue; }
                };
                let _ = sys_path.call_method1("insert", (0, &parent));
                match load_one_plugin(py, &file_stem) {
                    Some((module, hooks)) => {
                        eprintln!("[Done] :: loaded plugin: {} (hooks: {})", file_stem, hooks.join(", "));
                        loaded.push((file_stem, module, hooks));
                    }
                    None => {
                        eprintln!("[Warning] :: plugin {} has no hooks, skipping", file_stem);
                    }
                }
            }
            Ok(loaded)
        });
        for (name, module, hooks) in plugins_result.unwrap_or_default() {
            self.plugins.push(LoadedPlugin { name, module, hooks });
        }
    }

    pub fn fire(&self, event: &str, data: &std::collections::HashMap<String, String>) {
        let _ = Python::with_gil(|py| -> PyResult<()> {
            for plugin in &self.plugins {
                if plugin.hooks.contains(&event.to_string()) {
                    let kwargs = PyDict::new(py);
                    for (k, v) in data {
                        kwargs.set_item(k.as_str(), v.as_str())?;
                    }
                    let _ = plugin.module.call_method(py, event, (), Some(&kwargs));
                }
            }
            Ok(())
        });
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    pub fn names(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.name.clone()).collect()
    }
}

fn load_one_plugin(py: Python, file_stem: &str) -> Option<(PyObject, Vec<String>)> {
    let module = py.import(file_stem).ok()?;
    let dir = module.dir().ok()?;
    let mut hooks = Vec::new();
    let builtins = py.import("builtins").ok()?;
    let callable = builtins.getattr("callable").ok()?;
    for item in dir.iter() {
        if let Ok(name) = item.extract::<String>() {
            if name.starts_with('_') { continue; }
            if let Ok(attr) = module.getattr(name.as_str())
                && callable.call1((attr,)).and_then(|r| r.extract::<bool>()).unwrap_or(false) {
                    hooks.push(name);
                }
        }
    }
    if hooks.is_empty() { return None; }
    Some((module.into(), hooks))
}

// --- p.desc descriptor loading ---

fn plugin_desc_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(std::path::PathBuf::from(home).join(".config/cesar/p.desc"));
    }
    candidates.push(std::path::PathBuf::from("/etc/cesar/p.desc"));
    candidates.push(std::path::PathBuf::from("./p.desc"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("p.desc"));
    }
    candidates
}

fn ensure_pdesc_loaded() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for path in plugin_desc_candidates() {
            if path.exists()
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<PluginDescConfig>(&content)
            {
                for (id, entry) in config.plugins {
                    let expanded = expand_tilde(&entry.path);
                    let dest = Path::new(&expanded);
                    let aliases = entry.aliases.clone();
                    PluginManager::register_desc(&id, &entry.name, dest, &aliases);
                }
                eprintln!("[Done] :: loaded plugins from p.desc: {}", path.display());
                return;
            }
        }
    });
}

// --- backward-compatible associated functions ---

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub name: String,
    pub path: String,
    pub aliases: HashMap<String, String>,
}

fn plugin_registry() -> &'static Mutex<HashMap<String, PluginEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, PluginEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

impl PluginManager {
    pub fn list() -> Vec<PluginEntry> {
        ensure_pdesc_loaded();
        plugin_registry().lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
    }

    pub fn by_alias(alias: &str) -> Option<(PluginEntry, String)> {
        ensure_pdesc_loaded();
        let registry = plugin_registry().lock().unwrap_or_else(|e| e.into_inner());
        for entry in registry.values() {
            if let Some(cmd) = entry.aliases.get(alias) {
                return Some((entry.clone(), cmd.clone()));
            }
        }
        None
    }

    pub fn run(entry: &PluginEntry, func: &str, args: &[String]) -> Result<String, String> {
        ensure_pdesc_loaded();
        let path = expand_tilde(&entry.path);
        let std_path = std::path::PathBuf::from(&path);
        if !std_path.exists() {
            return Err(format!("plugin file not found: {}", path));
        }
        let mut command = func.to_string();
        let joined = args.join(" ");
        if command.contains("{}") {
            command = command.replace("{}", &joined);
        } else if !joined.is_empty() {
            command = format!("{} {}", command, joined);
        }
        let parent = std_path.parent().unwrap_or_else(|| Path::new("."));
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(parent)
            .output()
            .map_err(|e| format!("failed to run plugin: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    pub fn register(name: &str, dest: &Path, aliases: &HashMap<String, String>) {
        let mut registry = plugin_registry().lock().unwrap_or_else(|e| e.into_inner());
        registry.insert(name.to_string(), PluginEntry {
            name: name.to_string(),
            path: dest.to_string_lossy().to_string(),
            aliases: aliases.clone(),
        });
    }

    pub fn register_desc(name: &str, display_name: &str, dest: &Path, aliases: &HashMap<String, String>) {
        let mut registry = plugin_registry().lock().unwrap_or_else(|e| e.into_inner());
        registry.insert(name.to_string(), PluginEntry {
            name: display_name.to_string(),
            path: dest.to_string_lossy().to_string(),
            aliases: aliases.clone(),
        });
    }

    pub fn unregister(name: &str) {
        let mut registry = plugin_registry().lock().unwrap_or_else(|e| e.into_inner());
        registry.remove(name);
        registry.retain(|_, e| e.name != name);
    }

    pub fn by_name(name: &str) -> Option<PluginEntry> {
        ensure_pdesc_loaded();
        let registry = plugin_registry().lock().unwrap_or_else(|e| e.into_inner());
        registry.get(name).cloned()
            .or_else(|| registry.values().find(|e| e.name == name).cloned())
    }
}
