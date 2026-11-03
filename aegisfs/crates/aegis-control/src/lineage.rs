use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

/// PID plus kernel start time prevents accidental PID-reuse attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ProcessKey {
    pub pid: u32,
    pub start_time_ticks: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessNode {
    pub key: ProcessKey,
    pub parent: Option<ProcessKey>,
    pub executable: String,
    pub uid: u32,
    pub ragid: String,
}

#[derive(Debug, Clone, Default)]
pub struct Lineage {
    nodes: Vec<ProcessNode>,
}

impl Lineage {
    /// Best-effort `/proc` snapshot. Processes can exit during scanning; those
    /// entries are skipped. This is telemetry, not a synchronous kernel gate.
    pub fn scan(procfs: &Path) -> Result<Self> {
        #[derive(Clone)]
        struct Raw {
            key: ProcessKey,
            parent_pid: u32,
            executable: String,
            uid: u32,
        }
        let mut raw = Vec::new();
        for entry in fs::read_dir(procfs).with_context(|| format!("read {}", procfs.display()))? {
            let entry = entry?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let path = entry.path();
            let Ok(stat) = fs::read_to_string(path.join("stat")) else {
                continue;
            };
            let Some((parent_pid, start_time_ticks)) = parse_stat(&stat) else {
                continue;
            };
            let executable = fs::read_link(path.join("exe"))
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "unknown".to_owned());
            let uid = fs::metadata(&path)
                .map(|metadata| {
                    use std::os::unix::fs::MetadataExt;
                    metadata.uid()
                })
                .unwrap_or(0);
            raw.push(Raw {
                key: ProcessKey {
                    pid,
                    start_time_ticks,
                },
                parent_pid,
                executable,
                uid,
            });
        }
        let by_pid: HashMap<u32, Raw> = raw.iter().cloned().map(|node| (node.key.pid, node)).collect();
        let nodes = raw
            .into_iter()
            .map(|node| {
                let mut root = node.key;
                let mut parent = by_pid.get(&node.parent_pid).map(|parent| parent.key);
                let mut cursor = node.parent_pid;
                let mut visited = HashSet::new();
                while let Some(candidate) = by_pid.get(&cursor) {
                    if !visited.insert(candidate.key.pid) {
                        break;
                    }
                    root = candidate.key;
                    cursor = candidate.parent_pid;
                }
                // If the parent vanished from the snapshot, this node is the
                // visible root rather than being attached to a reused PID.
                if parent.is_some_and(|key| !by_pid.contains_key(&key.pid)) {
                    parent = None;
                    root = node.key;
                }
                ProcessNode {
                    key: node.key,
                    parent,
                    executable: node.executable,
                    uid: node.uid,
                    ragid: ragid(root),
                }
            })
            .collect();
        Ok(Self { nodes })
    }

    pub fn from_nodes(mut nodes: Vec<ProcessNode>) -> Self {
        let by_pid: HashMap<u32, ProcessNode> = nodes
            .iter()
            .cloned()
            .map(|node| (node.key.pid, node))
            .collect();
        for node in &mut nodes {
            let mut root = node.key;
            let mut cursor = node.parent;
            let mut visited = HashSet::new();
            while let Some(parent) = cursor.and_then(|key| by_pid.get(&key.pid)) {
                if parent.key.start_time_ticks != cursor.map(|key| key.start_time_ticks).unwrap_or(0)
                    || !visited.insert(parent.key.pid)
                {
                    break;
                }
                root = parent.key;
                cursor = parent.parent;
            }
            node.ragid = ragid(root);
        }
        Self { nodes }
    }

    pub fn nodes(&self) -> &[ProcessNode] {
        &self.nodes
    }

    pub fn tree(&self, root: ProcessKey) -> Vec<ProcessNode> {
        self.nodes
            .iter()
            .filter(|node| {
                node.key == root
                    || self
                        .nodes
                        .iter()
                        .any(|candidate| candidate.key == root && candidate.ragid == node.ragid)
            })
            .cloned()
            .collect()
    }
}

fn ragid(root: ProcessKey) -> String {
    let input = format!("{}:{}", root.pid, root.start_time_ticks);
    let hash = blake3::hash(input.as_bytes()).to_hex().to_string();
    format!("proc-{}", &hash[..16])
}

/// Returns PPID and starttime from Linux `/proc/<pid>/stat`. The executable
/// name can contain spaces and parentheses, so split after the final `)`.
fn parse_stat(stat: &str) -> Option<(u32, u64)> {
    let close = stat.rfind(')')?;
    let fields: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();
    // fields[0] is field 3 (state); starttime is field 22, hence index 19.
    Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(pid: u32, start: u64, parent: Option<ProcessKey>) -> ProcessNode {
        ProcessNode {
            key: ProcessKey {
                pid,
                start_time_ticks: start,
            },
            parent,
            executable: format!("/bin/worker-{pid}"),
            uid: 1000,
            ragid: String::new(),
        }
    }

    #[test]
    fn workers_share_the_root_ancestor_group() {
        let root = ProcessKey {
            pid: 10,
            start_time_ticks: 100,
        };
        let child = ProcessKey {
            pid: 11,
            start_time_ticks: 200,
        };
        let lineage = Lineage::from_nodes(vec![
            node(10, 100, None),
            node(11, 200, Some(root)),
            node(12, 300, Some(child)),
            node(99, 50, None),
        ]);
        let ragid = lineage.nodes()[0].ragid.clone();
        assert!(lineage.nodes()[1..3]
            .iter()
            .all(|node| node.ragid == ragid));
        assert_ne!(lineage.nodes()[3].ragid, ragid);
        assert_eq!(lineage.tree(root).len(), 3);
    }

    #[test]
    fn stat_parser_handles_parentheses_in_executable_name() {
        let mut fields = vec!["S", "7"];
        fields.extend(std::iter::repeat("0").take(17));
        fields.push("9876");
        let stat = format!("42 (a ) worker) {}", fields.join(" "));
        assert_eq!(parse_stat(&stat), Some((7, 9876)));
    }
}
