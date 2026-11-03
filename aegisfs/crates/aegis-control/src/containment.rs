use crate::lineage::{Lineage, ProcessKey};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ContainmentAction {
    PlannedStop,
    Stopped,
    PlannedKill,
    Killed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainmentReport {
    pub root: ProcessKey,
    pub action: ContainmentAction,
    pub dry_run: bool,
    pub pids: Vec<u32>,
    pub errors: Vec<String>,
}

/// Explicitly scoped Linux signal containment. Merely constructing this type
/// cannot signal anything: callers must register a root and disable dry-run.
pub struct Containment {
    dry_run: bool,
    self_pid: u32,
    allowed_roots: HashSet<ProcessKey>,
}

impl Containment {
    pub fn dry_run() -> Self {
        Self {
            dry_run: true,
            self_pid: std::process::id(),
            allowed_roots: HashSet::new(),
        }
    }

    /// Live mode still requires explicit registration of every allowed tree.
    pub fn live() -> Self {
        Self {
            dry_run: false,
            self_pid: std::process::id(),
            allowed_roots: HashSet::new(),
        }
    }

    pub fn register_tree(&mut self, root: ProcessKey) {
        if root.pid > 1 && root.pid != self.self_pid {
            self.allowed_roots.insert(root);
        }
    }

    pub fn freeze(&self, root: ProcessKey, lineage: &Lineage) -> ContainmentReport {
        self.apply(root, lineage, false)
    }

    /// Termination is deliberately separate from freezing so a frozen process
    /// can first be inspected or its executable captured by the caller.
    pub fn terminate(&self, root: ProcessKey, lineage: &Lineage) -> ContainmentReport {
        self.apply(root, lineage, true)
    }

    fn apply(&self, root: ProcessKey, lineage: &Lineage, kill: bool) -> ContainmentReport {
        let mut report = ContainmentReport {
            root,
            action: match (self.dry_run, kill) {
                (true, false) => ContainmentAction::PlannedStop,
                (false, false) => ContainmentAction::Stopped,
                (true, true) => ContainmentAction::PlannedKill,
                (false, true) => ContainmentAction::Killed,
            },
            dry_run: self.dry_run,
            pids: Vec::new(),
            errors: Vec::new(),
        };
        if !self.allowed_roots.contains(&root) {
            report
                .errors
                .push("process tree was not explicitly registered".to_owned());
            return report;
        }
        let members = lineage.tree(root);
        if members.is_empty() {
            report
                .errors
                .push("process tree is no longer visible".to_owned());
            return report;
        }
        for member in members {
            let pid = member.key.pid;
            if pid <= 1 || pid == self.self_pid {
                report
                    .errors
                    .push(format!("refusing to signal protected PID {pid}"));
                continue;
            }
            report.pids.push(pid);
            if !self.dry_run {
                let signal = if kill { libc::SIGKILL } else { libc::SIGSTOP };
                // SAFETY: kill does not dereference pointers. The PID came from
                // a fresh procfs snapshot and PID 1/self are excluded above.
                if unsafe { libc::kill(pid as libc::pid_t, signal) } != 0 {
                    report.errors.push(format!(
                        "signal PID {pid}: {}",
                        std::io::Error::last_os_error()
                    ));
                }
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineage::ProcessNode;

    #[test]
    fn unregistered_and_dry_run_trees_are_never_signalled() {
        let root = ProcessKey {
            pid: 4000000,
            start_time_ticks: 1,
        };
        let lineage = Lineage::from_nodes(vec![ProcessNode {
            key: root,
            parent: None,
            executable: "lab-simulator".into(),
            uid: 1000,
            ragid: String::new(),
        }]);
        let containment = Containment::dry_run();
        let denied = containment.freeze(root, &lineage);
        assert!(!denied.errors.is_empty());
        assert!(denied.pids.is_empty());

        let mut containment = Containment::dry_run();
        containment.register_tree(root);
        let planned = containment.freeze(root, &lineage);
        assert!(planned.errors.is_empty());
        assert_eq!(planned.pids, vec![root.pid]);
        assert_eq!(planned.action, ContainmentAction::PlannedStop);
    }
}
