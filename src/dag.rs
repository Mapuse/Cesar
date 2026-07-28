use std::collections::{HashMap, VecDeque};

use crate::service::{Service, ServiceConfig, ServiceState};

pub struct DagEngine {
    pub services: HashMap<String, Service>,
    adjacency: HashMap<String, Vec<String>>,
}

impl Default for DagEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DagEngine {
    pub fn new() -> Self {
        DagEngine {
            services: HashMap::new(),
            adjacency: HashMap::new(),
        }
    }

    pub fn add_service(&mut self, config: ServiceConfig) {
        let name = config.name.clone();
        let service = Service::from_config(config);
        self.services.insert(name.clone(), service);
        self.adjacency.entry(name).or_default();
    }

    pub fn build_dependency_graph(&mut self) -> Result<(), String> {
        self.adjacency.clear();
        for name in self.services.keys() {
            self.adjacency.entry(name.clone()).or_default();
        }

        let names: Vec<String> = self.services.keys().cloned().collect();

        for name in &names {
            let deps = self.services[name].config.requires.clone();
            for dep in &deps {
                if !self.services.contains_key(dep) {
                    return Err(format!(
                        "Service '{}' depends on unknown service '{}'",
                        name, dep
                    ));
                }
                self.adjacency
                    .entry(dep.clone())
                    .or_default()
                    .push(name.clone());
            }
        }

        if let Some(cycle) = self.detect_cycle() {
            return Err(format!("Circular dependency detected: {}", cycle.join(" -> ")));
        }

        Ok(())
    }

    fn detect_cycle(&self) -> Option<Vec<String>> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();
        let mut path = Vec::new();

        for name in self.services.keys() {
            if !visited.contains(name)
                && let Some(cycle) = self.dfs_cycle(name, &mut visited, &mut stack, &mut path) {
                    return Some(cycle);
                }
        }
        None
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut std::collections::HashSet<String>,
        stack: &mut std::collections::HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = self.adjacency.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if let Some(cycle) = self.dfs_cycle(dep, visited, stack, path) {
                        return Some(cycle);
                    }
                } else if stack.contains(dep) {
                    let cycle_start = path.iter().position(|p| p == dep).unwrap();
                    let mut cycle = path[cycle_start..].to_vec();
                    cycle.push(dep.to_string());
                    return Some(cycle);
                }
            }
        }

        path.pop();
        stack.remove(node);
        None
    }

    pub fn get_boot_order(&self) -> Vec<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        for name in self.services.keys() {
            in_degree.entry(name.clone()).or_insert(0);
        }

        for deps in self.adjacency.values() {
            for dep in deps {
                *in_degree.entry(dep.clone()).or_insert(0) += 1;
            }
        }

        let mut levels = Vec::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        for (name, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(name.clone());
            }
        }

        while !queue.is_empty() {
            let mut current_level = Vec::new();
            let mut next_queue = VecDeque::new();

            while let Some(node) = queue.pop_front() {
                current_level.push(node.clone());
                if let Some(children) = self.adjacency.get(&node) {
                    for child in children {
                        let deg = in_degree.get_mut(child).unwrap();
                        *deg -= 1;
                        if *deg == 0 {
                            next_queue.push_back(child.clone());
                        }
                    }
                }
            }

            if !current_level.is_empty() {
                current_level.sort();
                levels.push(current_level);
            }
            queue = next_queue;
        }

        levels
    }

    pub fn mark_running(&mut self, name: &str, pid: u32) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Running;
            svc.pid = Some(pid);
        }
    }

    pub fn mark_failed(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Failed;
            svc.pid = None;
        }
    }

    pub fn mark_stopped(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Stopped;
            svc.pid = None;
        }
    }

    pub fn mark_starting(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Starting;
        }
    }

    pub fn mark_stopping(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Stopping;
        }
    }

    pub fn mark_reloading(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Reloading;
        }
    }

    pub fn all_deps_satisfied(&self, name: &str) -> bool {
        if let Some(svc) = self.services.get(name) {
            for dep in &svc.config.requires {
                match self.services.get(dep) {
                    Some(dep_svc) => {
                        if dep_svc.state != ServiceState::Running {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        } else {
            false
        }
    }

    pub fn get_service_state(&self, name: &str) -> Option<ServiceState> {
        self.services.get(name).map(|s| s.state.clone())
    }
}
