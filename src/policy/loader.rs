//! Loads your rules from disk. She compiles regex once, then brings them up forever. The Karen who does the work once.

use crate::policy::gate::{CompileError, Gate, GateDef};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub engine: EngineConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub state: StateConfig,
    #[serde(default)]
    pub maintenance: MaintenanceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineConfig {
    #[serde(default = "default_true")]
    pub fail_open: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            fail_open: true,
            log_level: default_log_level(),
            timeout_ms: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyConfig {
    #[serde(default = "default_rules_dir")]
    pub rules_dir: String,
    #[serde(default = "default_gates_dir")]
    pub gates_dir: String,
    #[serde(default = "default_true")]
    pub validate_on_load: bool,
    #[serde(default = "default_true")]
    pub watch_for_changes: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            rules_dir: default_rules_dir(),
            gates_dir: default_gates_dir(),
            validate_on_load: true,
            watch_for_changes: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_state_dir")]
    pub file_dir: String,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            file_dir: default_state_dir(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaintenanceConfig {
    /// Temp-file prefixes she prunes on `SessionStart`; empty disables pruning. The Karen who knows exactly what she threw out, why she threw it out, and will describe the smell if you ask. Or don't ask. She's describing it either way.
    #[serde(default)]
    pub stale_temp_prefixes: Vec<String>,
    #[serde(default = "default_stale_hours")]
    pub stale_temp_age_hours: u64,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            stale_temp_prefixes: Vec::new(),
            stale_temp_age_hours: default_stale_hours(),
        }
    }
}

fn default_true() -> bool {
    true
}
const fn default_stale_hours() -> u64 {
    24
}
fn default_log_level() -> String {
    "info".into()
}
const fn default_timeout() -> u64 {
    5000
}
fn default_rules_dir() -> String {
    "rules".into()
}
fn default_gates_dir() -> String {
    "gates".into()
}
fn default_backend() -> String {
    "auto".into()
}
fn default_state_dir() -> String {
    "~/.cache/karen".into()
}

/// An always-on rule loaded from markdown. The Karen who injects it every session until it's not just a rule, it's a lifestyle.
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub path: PathBuf,
    pub content: String,
}

/// Everything she needs to evaluate a request. The Karen who prepares the way only someone with a laminated process can.
#[derive(Debug, Clone)]
pub struct PolicySet {
    pub gates: Vec<Gate>,
    pub rules: Vec<Rule>,
    pub config: Config,
    pub root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read policy root {root}: {source}")]
    Root {
        root: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON5 in {path}: {source}")]
    Json5 {
        path: PathBuf,
        #[source]
        source: Box<json5::Error>,
    },
    #[error("invalid TOML in {path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    // The Karen who boxes her grievances so they don't take up the whole
    // hallway — an error this size should wait politely by the door.
    #[error(transparent)]
    Gate(#[from] Box<CompileError>),
}

/// A gate file may hold one gate or a `gates:` list. The Karen who parses —
/// and boxes the singleton so it doesn't hog the whole hallway.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum GateFile {
    Single(Box<GateDef>),
    Many { gates: Vec<GateDef> },
}

#[must_use]
pub fn locate_policies() -> PathBuf {
    if let Ok(path) = std::env::var("KAREN_POLICIES") {
        return PathBuf::from(path);
    }
    if let Ok(root) = std::env::var("CLAUDE_PLUGIN_ROOT") {
        let candidate = PathBuf::from(root).join("policies");
        if candidate.is_dir() {
            return candidate;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().skip(1) {
            let candidate = ancestor.join("policies");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    PathBuf::from("policies")
}

/// Load the full policy set from `root`.
///
/// # Errors
///
/// Returns [`LoadError::Root`] when the policy root directory does not exist,
/// [`LoadError::Io`] when a policy file cannot be read, [`LoadError::Json5`]
/// or [`LoadError::Toml`] when `config.toml` or a gate file does not parse,
/// and [`LoadError::Gate`] when a gate fails to compile — the Karen who
/// refuses to load a policy she cannot enforce.
pub fn load(root: &Path) -> Result<PolicySet, LoadError> {
    if !root.is_dir() {
        return Err(LoadError::Root {
            root: root.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "policy root missing"),
        });
    }

    let config = load_config(root)?;
    let gates = load_gates(&root.join(&config.policy.gates_dir))?;
    let rules = load_rules(&root.join(&config.policy.rules_dir))?;

    Ok(PolicySet {
        gates,
        rules,
        config,
        root: root.to_path_buf(),
    })
}

fn load_config(root: &Path) -> Result<Config, LoadError> {
    let path = root.join("config.toml");
    if !path.is_file() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|source| LoadError::Io {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| LoadError::Toml {
        path,
        source: Box::new(source),
    })
}

fn load_gates(dir: &Path) -> Result<Vec<Gate>, LoadError> {
    let mut gates = Vec::new();
    if !dir.is_dir() {
        return Ok(gates);
    }
    let mut files = json5_files(dir)?;
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        let parsed: GateFile = json5::from_str(&text).map_err(|source| LoadError::Json5 {
            path: path.clone(),
            source: Box::new(source),
        })?;
        let defs = match parsed {
            GateFile::Single(def) => vec![*def],
            GateFile::Many { gates } => gates,
        };
        for def in defs {
            gates.push(Gate::compile(def).map_err(Box::new)?);
        }
    }
    gates.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    Ok(gates)
}

fn load_rules(dir: &Path) -> Result<Vec<Rule>, LoadError> {
    let mut rules = Vec::new();
    if !dir.is_dir() {
        return Ok(rules);
    }
    let mut files = Vec::new();
    collect_with_extension(dir, "md", &mut files)?;
    files.sort();
    for path in files {
        let content = std::fs::read_to_string(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rule")
            .to_string();
        rules.push(Rule { id, path, content });
    }
    Ok(rules)
}

fn json5_files(dir: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let mut files = Vec::new();
    for ext in ["json5", "jsonc", "json"] {
        collect_with_extension(dir, ext, &mut files)?;
    }
    Ok(files)
}

fn collect_with_extension(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<(), LoadError> {
    let entries = std::fs::read_dir(dir).map_err(|source| LoadError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_with_extension(&path, ext, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_gates_and_rules() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("policies");
        let set = load(&root).expect("policies load");
        assert!(!set.gates.is_empty(), "expected at least one gate");
        assert!(!set.rules.is_empty(), "expected at least one rule");
        assert!(
            set.gates.windows(2).all(|w| w[0].priority >= w[1].priority),
            "gates must be priority ordered"
        );
    }
}
