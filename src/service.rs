use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Failed,
    Reloading,
    Stopping,
}

impl fmt::Display for ServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceState::Stopped => write!(f, "STOPPED"),
            ServiceState::Starting => write!(f, "STARTING"),
            ServiceState::Running => write!(f, "RUNNING"),
            ServiceState::Failed => write!(f, "FAILED"),
            ServiceState::Reloading => write!(f, "RELOADING"),
            ServiceState::Stopping => write!(f, "STOPPING"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RestartPolicy {
    Never,
    OnFailure,
    Always,
}

impl fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestartPolicy::Never => write!(f, "never"),
            RestartPolicy::OnFailure => write!(f, "on-failure"),
            RestartPolicy::Always => write!(f, "always"),
        }
    }
}

impl RestartPolicy {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "on-failure" => RestartPolicy::OnFailure,
            "always" => RestartPolicy::Always,
            _ => RestartPolicy::Never,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub exec: String,
    pub requires: Vec<String>,
    pub restart: RestartPolicy,
    pub socket: Option<String>,
    pub description: Option<String>,
    pub environment: Vec<(String, String)>,
    pub working_directory: Option<String>,
}

impl ServiceConfig {
    pub fn new(name: &str, exec: &str) -> Self {
        ServiceConfig {
            name: name.to_string(),
            exec: exec.to_string(),
            requires: Vec::new(),
            restart: RestartPolicy::Never,
            socket: None,
            description: None,
            environment: Vec::new(),
            working_directory: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Service {
    pub config: ServiceConfig,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub restart_count: u32,
    /// When the service last transitioned to Running; used to reset
    /// the restart counter after it proves stable.
    pub started_at: Option<std::time::Instant>,
}

impl Service {
    pub fn from_config(config: ServiceConfig) -> Self {
        Service {
            config,
            state: ServiceState::Stopped,
            pid: None,
            restart_count: 0,
            started_at: None,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.state, ServiceState::Running | ServiceState::Failed | ServiceState::Stopped)
    }
}
