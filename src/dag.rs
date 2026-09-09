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

    /// Drop a service entirely (also strips edges pointing at it).
    /// Callers must stop a running service before removing it.
    pub fn remove_service(&mut self, name: &str) {
        self.services.remove(name);
        self.adjacency.remove(name);
        for deps in self.adjacency.values_mut() {
            deps.retain(|d| d != name);
        }
    }

    pub fn build_dependency_graph(&mut self) -> Result<(), String> {
        // Build the complete replacement adjacency first; only swap it in
        // when every dependency resolves, so a bad config can never leave
        // a partially-cleared live graph behind.
        let mut new_adjacency: HashMap<String, Vec<String>> = self
            .services
            .keys()
            .map(|k| (k.clone(), Vec::new()))
            .collect();

        let names: Vec<String> = self.services.keys().cloned().collect();

        for name in &names {
            let deps = self.services[name].config.requires.clone();
            for dep in &deps {
                if !self.services.contains_key(dep) {
                    // Keep the previous graph untouched and report.
                    return Err(format!(
                        "Service '{}' depends on unknown service '{}'",
                        name, dep
                    ));
                }
                new_adjacency
                    .get_mut(dep)
                    .expect("dependency seeded above")
                    .push(name.clone());
            }
        }

        if let Some(cycle) = Self::find_cycle(&new_adjacency, names.iter()) {
            return Err(format!(
                "Circular dependency detected: {}",
                cycle.join(" -> ")
            ));
        }

        self.adjacency = new_adjacency;
        Ok(())
    }

    fn find_cycle<'a, I>(adjacency: &HashMap<String, Vec<String>>, nodes: I) -> Option<Vec<String>>
    where
        I: Iterator<Item = &'a String>,
    {
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();
        let mut path = Vec::new();

        for name in nodes {
            if !visited.contains(name)
                && let Some(cycle) =
                    Self::dfs_cycle(adjacency, name, &mut visited, &mut stack, &mut path)
            {
                return Some(cycle);
            }
        }
        None
    }

    fn dfs_cycle(
        adjacency: &HashMap<String, Vec<String>>,
        node: &str,
        visited: &mut std::collections::HashSet<String>,
        stack: &mut std::collections::HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = adjacency.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if let Some(cycle) = Self::dfs_cycle(adjacency, dep, visited, stack, path) {
                        return Some(cycle);
                    }
                } else if stack.contains(dep) {
                    let cycle_start = path
                        .iter()
                        .position(|p| p == dep)
                        .expect("cycle start in path");
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
                        let deg = in_degree.get_mut(child).expect("child in in-degree map");
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

    /// Services that never appeared in any boot level: they are part of a
    /// cycle or depend (transitively) on one, so they would silently never
    /// start without this report.
    pub fn unresolved(&self) -> Vec<String> {
        let boot_order = self.get_boot_order();
        let scheduled: std::collections::HashSet<&String> = boot_order.iter().flatten().collect();
        let mut missing: Vec<String> = self
            .services
            .keys()
            .filter(|name| !scheduled.contains(name))
            .cloned()
            .collect();
        missing.sort();
        missing
    }

    pub fn mark_running(&mut self, name: &str, pid: u32) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Running;
            svc.pid = Some(pid);
            svc.started_at = Some(std::time::Instant::now());
        }
    }

    pub fn mark_failed(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Failed;
            svc.pid = None;
            svc.started_at = None;
        }
    }

    pub fn mark_stopped(&mut self, name: &str) {
        if let Some(svc) = self.services.get_mut(name) {
            svc.state = ServiceState::Stopped;
            svc.pid = None;
            svc.started_at = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ServiceConfig;

    fn cfg(name: &str, requires: &[&str]) -> ServiceConfig {
        let mut c = ServiceConfig::new(name, "/bin/true");
        c.requires = requires.iter().map(|s| s.to_string()).collect();
        c
    }

    #[test]
    fn boot_order_respects_dependencies() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("app", &["db", "cache"]));
        dag.add_service(cfg("db", &[]));
        dag.add_service(cfg("cache", &["db"]));
        dag.build_dependency_graph().expect("acyclic");

        let order = dag.get_boot_order();
        assert_eq!(order[0], vec!["db".to_string()]);
        assert_eq!(order[1], vec!["cache".to_string()]);
        assert_eq!(order[2], vec!["app".to_string()]);
    }

    #[test]
    fn cycle_detection_rejects_and_keeps_old_graph() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("a", &[]));
        dag.add_service(cfg("b", &["a"]));
        dag.build_dependency_graph().expect("initial graph ok");
        let good_order = dag.get_boot_order();

        // Introduce a cycle: b -> a becomes a -> b, a -> b.
        dag.services.get_mut("a").expect("a").config.requires = vec!["b".to_string()];
        let err = dag.build_dependency_graph().expect_err("cycle rejected");
        assert!(err.contains("Circular dependency"), "got: {}", err);

        // The previously-good graph must survive the failed rebuild.
        assert_eq!(dag.get_boot_order(), good_order);
    }

    #[test]
    fn unknown_dep_rejected_and_old_graph_kept() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("solo", &[]));
        dag.add_service(cfg("bad", &["ghost"]));
        let err = dag.build_dependency_graph().expect_err("unknown dep");
        assert!(err.contains("unknown service 'ghost'"), "got: {}", err);
        // Old (empty) adjacency intact: no edges, so all nodes land in one level.
        assert_eq!(
            dag.get_boot_order(),
            vec![vec!["bad".to_string(), "solo".to_string()]]
        );
    }

    #[test]
    fn remove_service_drops_edges() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("dep", &[]));
        dag.add_service(cfg("user", &["dep"]));
        dag.build_dependency_graph().expect("ok");

        dag.remove_service("dep");
        assert!(!dag.services.contains_key("dep"));
        // Rebuild must now fail for the dependent, and the edge is gone.
        let err = dag.build_dependency_graph().expect_err("dangling dep");
        assert!(err.contains("'user' depends on unknown service 'dep'"));
    }

    #[test]
    fn unresolved_reports_cycle_members() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("free", &[]));
        dag.add_service(cfg("p", &["q"]));
        dag.add_service(cfg("q", &["p"]));
        // Install a cyclic adjacency directly (private field, test crate).
        dag.adjacency.insert("free".to_string(), vec![]);
        dag.adjacency.insert("p".to_string(), vec!["q".to_string()]);
        dag.adjacency.insert("q".to_string(), vec!["p".to_string()]);

        assert_eq!(dag.unresolved(), vec!["p".to_string(), "q".to_string()]);

        // Breaking the cycle makes everyone schedulable again.
        dag.services.get_mut("q").expect("q").config.requires = vec![];
        dag.build_dependency_graph().expect("acyclic now");
        assert!(dag.unresolved().is_empty());
    }

    #[test]
    fn mark_transitions_track_state_pid_and_started_at() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("svc", &[]));

        dag.mark_starting("svc");
        assert_eq!(dag.get_service_state("svc"), Some(ServiceState::Starting));

        dag.mark_running("svc", 4242);
        assert_eq!(dag.get_service_state("svc"), Some(ServiceState::Running));
        assert_eq!(dag.services["svc"].pid, Some(4242));
        assert!(dag.services["svc"].started_at.is_some());

        dag.mark_failed("svc");
        assert_eq!(dag.get_service_state("svc"), Some(ServiceState::Failed));
        assert_eq!(dag.services["svc"].pid, None);
        assert!(dag.services["svc"].started_at.is_none());
    }

    #[test]
    fn all_deps_satisfied_requires_running_deps() {
        let mut dag = DagEngine::new();
        dag.add_service(cfg("base", &[]));
        dag.add_service(cfg("top", &["base"]));
        dag.build_dependency_graph().expect("ok");

        assert!(!dag.all_deps_satisfied("top"));
        dag.mark_running("base", 1);
        assert!(dag.all_deps_satisfied("top"));
        assert!(!dag.all_deps_satisfied("missing"));
    }
}
