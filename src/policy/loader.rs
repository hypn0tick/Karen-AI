//! Loads your rules from disk. She compiles regex once, then brings them up forever. The Karen who does the work once.

use crate::policy::gate::{CompileError, Gate, GateDef, Layer};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub engine: EngineConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub state: StateConfig,
    #[serde(default)]
    pub maintenance: MaintenanceConfig,
    #[serde(default)]
    pub facts: FactsConfig,
}

/// What the environment facts look for. `tools` names the binaries `facts.tools`
/// reports as present on `PATH`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsConfig {
    #[serde(default = "default_fact_tools")]
    pub tools: Vec<String>,
}

impl Default for FactsConfig {
    fn default() -> Self {
        Self {
            tools: default_fact_tools(),
        }
    }
}

fn default_fact_tools() -> Vec<String> {
    [
        "git",
        "rg",
        "sd",
        "yq",
        "jq",
        "shellcheck",
        "ast-grep",
        "difft",
        "uv",
        "node",
        "bun",
        "cargo",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Engine knobs. She checks the fine print on both sides of the desk:
/// unknown keys are refused by name, not silently filed under "probably meant something".
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    #[serde(default = "default_true")]
    pub fail_open: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Live reload: in-process embedders watch the policy root and swap
    /// the law mid-flight (see [`Engine::watch_policies`]); every CLI
    /// invocation re-reads regardless. The Karen who changes her mind
    /// before you finish saving it.
    #[serde(default = "default_true")]
    pub hot_reload: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            fail_open: true,
            log_level: default_log_level(),
            hot_reload: true,
            timeout_ms: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    #[serde(default = "default_rules_dir")]
    pub rules_dir: String,
    #[serde(default = "default_gates_dir")]
    pub gates_dir: String,
    #[serde(default = "default_true")]
    pub validate_on_load: bool,
    /// Gate ids from earlier layers that this layer switches off.
    #[serde(default)]
    pub disable: Vec<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            rules_dir: default_rules_dir(),
            gates_dir: default_gates_dir(),
            validate_on_load: true,
            disable: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Where the file backend keeps session state; `<Karen home>/state` when unset.
    #[serde(default)]
    pub file_dir: Option<String>,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            file_dir: None,
        }
    }
}

impl StateConfig {
    /// The configured directory with `~/` expanded, else `<Karen home>/state`.
    #[must_use]
    pub fn dir(&self) -> PathBuf {
        self.file_dir.as_deref().map_or_else(
            || crate::home::karen_home().join("state"),
            crate::home::expand,
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// An always-on rule loaded from markdown. The Karen who injects it every session until it's not just a rule, it's a lifestyle.
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub path: PathBuf,
    pub content: String,
    /// Runtimes the rule is injected into (`runtime` fact names); empty means
    /// every runtime. Set by a `runtimes: [claude, codex]` front matter block.
    pub runtimes: Vec<String>,
}

impl Rule {
    #[must_use]
    pub fn applies_to(&self, runtime: Option<&str>) -> bool {
        self.runtimes.is_empty() || runtime.is_some_and(|r| self.runtimes.iter().any(|x| x == r))
    }
}

/// Split an optional `---` front matter block off a rule file. The only key is
/// `runtimes`, a comma-separated list with optional brackets.
fn rule_front_matter(path: &Path, text: &str) -> Result<(Vec<String>, String), LoadError> {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = body
        .strip_prefix("---\n")
        .or_else(|| body.strip_prefix("---\r\n"))
    else {
        return Ok((Vec::new(), text.to_string()));
    };
    let invalid = |message: String| LoadError::Rule {
        path: path.to_path_buf(),
        message,
    };
    let end = rest
        .find("\n---")
        .ok_or_else(|| invalid("front matter has no closing `---`".into()))?;
    let mut runtimes = Vec::new();
    for line in rest[..end].lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| invalid(format!("front matter line `{line}` is not `key: value`")))?;
        if key.trim() != "runtimes" {
            return Err(invalid(format!(
                "unknown front matter key `{}` (only `runtimes`)",
                key.trim()
            )));
        }
        runtimes = value
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|r| r.trim().trim_matches(['"', '\'']).to_string())
            .filter(|r| !r.is_empty())
            .collect();
    }
    let after = &rest[end + 4..];
    let after = after.split_once('\n').map_or("", |(_, b)| b);
    Ok((runtimes, after.to_string()))
}

/// Everything she needs to evaluate a request: every layer, merged. The Karen who prepares the way only someone with a laminated process can.
#[derive(Debug, Clone)]
pub struct PolicySet {
    pub gates: Vec<Gate>,
    pub rules: Vec<Rule>,
    pub config: Config,
    /// The project layer's root, else the user layer's.
    pub root: PathBuf,
    /// Every layer read from disk, in merge order (the built-in layer has no root).
    pub layers: Vec<(Layer, PathBuf)>,
    /// How the project layer was found ([`Roots::found_by`]).
    pub found_by: Option<&'static str>,
    pub issues: Vec<MergeIssue>,
    pub changes: Vec<MergeChange>,
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
    #[error("invalid rule {path}: {message}")]
    Rule { path: PathBuf, message: String },
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

/// The gates compiled into Karen, under every other layer.
const BUILTIN_GATES: &str = include_str!("builtin.json");

/// Where a session's layers live. The user layer is `<Karen home>/policies`.
/// The project layer belongs to the repository, not to each checkout, so a
/// session finds it from its working directory without any wiring: the root
/// named explicitly (`--policies`, then `KAREN_POLICIES`), else the nearest
/// `.karen/policies` from the working directory up to the repository root,
/// else the repository's `karen.policies` setting, which every worktree of the
/// repository shares (`git config karen.policies <path>`). The Karen who
/// checks where she's standing before she checks the building.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub user: Option<PathBuf>,
    pub project: Option<PathBuf>,
    /// The project root was named, so a missing directory is an error rather
    /// than a session without a project layer.
    pub explicit: bool,
    /// How the project layer was found: `explicit`, `in-tree` or `git-config`.
    pub found_by: Option<&'static str>,
}

impl Roots {
    #[must_use]
    pub fn discover(explicit: Option<PathBuf>, cwd: Option<&Path>) -> Self {
        let user = Some(crate::home::user_policies()).filter(|p| p.is_dir());
        if let Some(project) =
            explicit.or_else(|| std::env::var_os("KAREN_POLICIES").map(PathBuf::from))
        {
            return Self {
                user,
                project: Some(project),
                explicit: true,
                found_by: Some("explicit"),
            };
        }
        let Some(start) = cwd
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
        else {
            return Self {
                user,
                ..Self::default()
            };
        };
        let repo = start.ancestors().find(|d| d.join(".git").exists());
        // The walk stops at the repository root, and never climbs above the
        // user's home: a root there belongs to another project.
        let home = crate::home::user_home().filter(|h| start.starts_with(h));
        let in_tree = start
            .ancestors()
            .take_while(|d| repo.is_none_or(|r| d.starts_with(r)))
            .take_while(|d| home.as_ref().is_none_or(|h| d.starts_with(h)))
            .map(|a| a.join(".karen").join("policies"))
            .find(|p| p.is_dir() && !same_dir(p, user.as_deref()));
        let (project, found_by) = match in_tree {
            Some(p) => (Some(p), Some("in-tree")),
            None => match repo.and_then(repo_pointer) {
                Some(p) => (Some(p), Some("git-config")),
                None => (None, None),
            },
        };
        Self {
            user,
            project,
            explicit: false,
            found_by,
        }
    }

    /// The root an embedder or a command shows as "the" policy root: the
    /// project layer, else the user layer.
    #[must_use]
    pub fn primary(&self) -> PathBuf {
        self.project
            .clone()
            .or_else(|| self.user.clone())
            .unwrap_or_default()
    }
}

/// The repository's `karen.policies`, read from the config file in its common
/// git directory, which a linked worktree reaches through its `.git` file and
/// `commondir`. A relative value resolves against the main working tree; `~/`
/// against the user's home.
fn repo_pointer(worktree: &Path) -> Option<PathBuf> {
    let dot_git = worktree.join(".git");
    let common = if dot_git.is_dir() {
        dot_git
    } else {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
        let gitdir = worktree.join(gitdir);
        match std::fs::read_to_string(gitdir.join("commondir")) {
            Ok(common) => lexical(&gitdir.join(common.trim())),
            Err(_) => gitdir,
        }
    };
    let config = std::fs::read_to_string(common.join("config")).ok()?;
    let value = git_config_value(&config, "karen", "policies")?;
    if value.starts_with("~/") {
        return Some(crate::home::expand(&value));
    }
    let path = PathBuf::from(&value);
    if path.is_absolute() {
        return Some(path);
    }
    Some(common.parent().unwrap_or(worktree).join(path))
}

/// `path` with `.` and `..` resolved by name, so its parent is the real parent
/// (`commondir` is usually `../..`).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The last value of `section.key` in a git config file. Section and key match
/// case-insensitively; subsections, includes and escapes beyond quotes are not
/// read, which `karen.policies` never needs.
fn git_config_value(config: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    let mut found = None;
    for line in config.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            current = header.trim_end_matches(']').trim().to_ascii_lowercase();
            continue;
        }
        if current != section {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        let value = value.trim();
        let value = match value.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or(""),
            None => value.split(['#', ';']).next().unwrap_or("").trim(),
        };
        found = Some(value.to_string());
    }
    found
}

fn same_dir(a: &Path, b: Option<&Path>) -> bool {
    b.is_some_and(|b| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    })
}

/// Something the merge refused or could not apply. The merged policy still
/// loads, because a load failure fails open: one bad file in a project must
/// never switch off every other layer's gates.
#[derive(Debug, Clone)]
pub struct MergeIssue {
    pub layer: Layer,
    pub message: String,
    /// A refused change (a locked gate, a reused id without `override`):
    /// `karen validate` fails on it. Otherwise a warning.
    pub refused: bool,
}

/// A gate an later layer replaced or disabled.
#[derive(Debug, Clone)]
pub struct MergeChange {
    pub gate: String,
    pub layer: Layer,
    pub kind: &'static str,
}

/// Load the built-in, user and project layers, with `root` as the project
/// layer.
///
/// # Errors
///
/// As [`load_roots`].
pub fn load(root: &Path) -> Result<PolicySet, LoadError> {
    load_roots(&Roots {
        user: Some(crate::home::user_policies()).filter(|p| p.is_dir()),
        project: Some(root.to_path_buf()),
        explicit: true,
        found_by: Some("explicit"),
    })
}

/// Load and merge every layer in `roots` over the built-in gates.
///
/// # Errors
///
/// Returns [`LoadError::Root`] when an explicitly named project root does not
/// exist, [`LoadError::Io`] when a policy file cannot be read,
/// [`LoadError::Json5`] or [`LoadError::Toml`] when a file does not parse,
/// and [`LoadError::Gate`] when a gate fails to compile — the Karen who
/// refuses to load a policy she cannot enforce.
pub fn load_roots(roots: &Roots) -> Result<PolicySet, LoadError> {
    let mut layers = vec![builtin_layer()?];
    if let Some(user) = &roots.user {
        layers.push(read_layer(Layer::User, user)?);
    }
    if let Some(project) = &roots.project {
        if project.is_dir() {
            layers.push(read_layer(Layer::Project, project)?);
        } else if roots.explicit {
            return Err(LoadError::Root {
                root: project.clone(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "policy root missing"),
            });
        }
    }
    let mut set = merge(layers, roots.primary())?;
    set.found_by = roots.found_by.filter(|_| {
        set.layers
            .iter()
            .any(|(layer, _)| matches!(layer, Layer::Project))
    });
    Ok(set)
}

/// One layer as read from disk, before merging.
struct LayerFiles {
    layer: Layer,
    root: Option<PathBuf>,
    table: toml::Table,
    disable: Vec<String>,
    gates: Vec<Gate>,
    rules: Vec<Rule>,
}

fn builtin_layer() -> Result<LayerFiles, LoadError> {
    let parsed: GateFile = json5::from_str(BUILTIN_GATES).map_err(|source| LoadError::Json5 {
        path: PathBuf::from("builtin"),
        source: Box::new(source),
    })?;
    let gates = gate_defs(parsed)
        .into_iter()
        .map(|def| tag(Gate::compile(def), Layer::Builtin, "builtin"))
        .collect::<Result<_, _>>()?;
    Ok(LayerFiles {
        layer: Layer::Builtin,
        root: None,
        table: toml::Table::new(),
        disable: Vec::new(),
        gates,
        rules: Vec::new(),
    })
}

fn read_layer(layer: Layer, root: &Path) -> Result<LayerFiles, LoadError> {
    let table = load_config_table(root)?;
    let config: Config = config_from(&table, root)?;
    let gates = load_gates(&root.join(&config.policy.gates_dir), layer)?;
    if config.policy.validate_on_load {
        validate_once(root, &gates)?;
    }
    let rules = load_rules(&root.join(&config.policy.rules_dir))?;
    Ok(LayerFiles {
        layer,
        root: Some(root.to_path_buf()),
        table,
        disable: config.policy.disable,
        gates,
        rules,
    })
}

fn tag(gate: Result<Gate, CompileError>, layer: Layer, source: &str) -> Result<Gate, LoadError> {
    let mut gate = gate.map_err(Box::new)?;
    gate.layer = layer;
    source.clone_into(&mut gate.source);
    Ok(gate)
}

/// Apply the layers in order: each layer's disables, then its gates.
fn merge(layers: Vec<LayerFiles>, primary: PathBuf) -> Result<PolicySet, LoadError> {
    let mut gates: Vec<Gate> = Vec::new();
    let mut rules: Vec<Rule> = Vec::new();
    let mut table = toml::Table::new();
    let mut issues = Vec::new();
    let mut changes = Vec::new();
    let mut roots = Vec::new();

    for files in layers {
        let layer = files.layer;
        if let Some(root) = files.root {
            roots.push((layer, root));
        }
        merge_tables(&mut table, files.table);

        for id in files.disable {
            match gates.iter().position(|g| g.id == id) {
                None => issues.push(MergeIssue {
                    layer,
                    message: format!("disables `{id}`, which no earlier layer defines"),
                    refused: false,
                }),
                Some(i) if gates[i].locked => issues.push(MergeIssue {
                    layer,
                    message: format!(
                        "tries to disable `{id}`, locked by the {} layer; it stays enforced",
                        gates[i].layer
                    ),
                    refused: true,
                }),
                Some(i) => {
                    gates.remove(i);
                    changes.push(MergeChange {
                        gate: id,
                        layer,
                        kind: "disabled",
                    });
                }
            }
        }

        for gate in files.gates {
            match gates.iter().position(|g| g.id == gate.id) {
                None => {
                    if gate.replaces {
                        issues.push(MergeIssue {
                            layer,
                            message: format!(
                                "`{}` sets override but no earlier layer defines it",
                                gate.id
                            ),
                            refused: false,
                        });
                    }
                    gates.push(gate);
                }
                Some(i) if gates[i].locked => issues.push(MergeIssue {
                    layer,
                    message: format!(
                        "tries to replace `{}`, locked by the {} layer; the {} gate stays",
                        gate.id, gates[i].layer, gates[i].layer
                    ),
                    refused: true,
                }),
                Some(i) if gates[i].layer == layer || !gate.replaces => issues.push(MergeIssue {
                    layer,
                    message: format!(
                        "reuses the id `{}` ({}) without `\"override\": true`; the first definition stays",
                        gate.id, gates[i].source
                    ),
                    refused: true,
                }),
                Some(i) => {
                    changes.push(MergeChange {
                        gate: gate.id.clone(),
                        layer,
                        kind: "replaced",
                    });
                    gates[i] = gate;
                }
            }
        }

        for rule in files.rules {
            rules.retain(|r| r.id != rule.id);
            rules.push(rule);
        }
    }

    gates.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    let config = config_from(&table, &primary)?;
    Ok(PolicySet {
        gates,
        rules,
        config,
        root: primary,
        layers: roots,
        found_by: None,
        issues,
        changes,
    })
}

/// Later layers win per key; arrays are unioned in order.
fn merge_tables(into: &mut toml::Table, from: toml::Table) {
    for (key, value) in from {
        match (into.get_mut(&key), value) {
            (Some(toml::Value::Table(a)), toml::Value::Table(b)) => merge_tables(a, b),
            (Some(toml::Value::Array(a)), toml::Value::Array(b)) => {
                for item in b {
                    if !a.contains(&item) {
                        a.push(item);
                    }
                }
            }
            (_, value) => {
                into.insert(key, value);
            }
        }
    }
}

fn config_from(table: &toml::Table, root: &Path) -> Result<Config, LoadError> {
    toml::Value::Table(table.clone())
        .try_into()
        .map_err(|source| LoadError::Toml {
            path: root.join("config.toml"),
            source: Box::new(source),
        })
}

/// Compile every gate's patterns once per version of the policy files. A
/// marker named by the root and a fingerprint of its files (path, size,
/// modification time) records a clean validation, so an unchanged policy skips
/// the work; any edit, addition or removal changes the fingerprint and
/// validates again. Without a usable cache directory she validates every time.
fn validate_once(root: &Path, gates: &[Gate]) -> Result<(), LoadError> {
    let marker = fingerprint(root).map(|(root_id, files_id)| {
        (
            format!("validated-{root_id:016x}-"),
            crate::home::cache_dir().join(format!("validated-{root_id:016x}-{files_id:016x}")),
        )
    });
    if marker.as_ref().is_some_and(|(_, path)| path.is_file()) {
        return Ok(());
    }
    for gate in gates {
        gate.validate().map_err(Box::new)?;
    }
    if let Some((prefix, path)) = marker {
        record_validation(&prefix, &path);
    }
    Ok(())
}

/// Write the marker and drop older markers of the same root. Best effort: a
/// cache she cannot write only costs the next call a validation.
fn record_validation(prefix: &str, path: &Path) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let _ = std::fs::write(path, env!("CARGO_PKG_VERSION"));
}

/// A hash of the root's path, and a hash of every file under it with its size
/// and modification time plus Karen's version.
fn fingerprint(root: &Path) -> Option<(u64, u64)> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let root = root.canonicalize().ok()?;
    let mut files = Vec::new();
    walk_files(&root, &mut files).ok()?;
    files.sort();
    let mut root_hash = DefaultHasher::new();
    root.hash(&mut root_hash);
    let mut files_hash = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut files_hash);
    for path in files {
        let meta = std::fs::metadata(&path).ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        (path, meta.len(), modified.as_nanos()).hash(&mut files_hash);
    }
    Some((root_hash.finish(), files_hash.finish()))
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn load_config_table(root: &Path) -> Result<toml::Table, LoadError> {
    let path = root.join("config.toml");
    if !path.is_file() {
        return Ok(toml::Table::new());
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

fn gate_defs(parsed: GateFile) -> Vec<GateDef> {
    match parsed {
        GateFile::Single(def) => vec![*def],
        GateFile::Many { gates } => gates,
    }
}

fn load_gates(dir: &Path, layer: Layer) -> Result<Vec<Gate>, LoadError> {
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
        let source = path.display().to_string();
        for def in gate_defs(parsed) {
            gates.push(tag(Gate::compile(def), layer, &source)?);
        }
    }
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
        let (runtimes, content) = rule_front_matter(&path, &content)?;
        rules.push(Rule {
            id,
            path,
            content,
            runtimes,
        });
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

    /// The built-in layer plus `root`, never the real user layer.
    fn load_project(root: &Path) -> Result<PolicySet, LoadError> {
        load_roots(&Roots {
            user: None,
            project: Some(root.to_path_buf()),
            explicit: true,
            found_by: Some("explicit"),
        })
    }

    #[test]
    fn the_layer_doc_names_exactly_the_built_in_gates() {
        let doc = include_str!("../../docs/layered-roots.md");
        let row = doc
            .lines()
            .find(|l| l.starts_with("| Built-in |"))
            .expect("built-in row");
        let mut named: Vec<&str> = row.split('`').filter(|s| s.starts_with("karen-")).collect();
        let mut shipped: Vec<String> = builtin_layer()
            .unwrap()
            .gates
            .into_iter()
            .map(|g| g.id)
            .collect();
        named.sort_unstable();
        shipped.sort_unstable();
        assert_eq!(named, shipped);
    }

    #[test]
    fn git_config_value_reads_the_last_karen_policies() {
        let config = "[core]\n\tpolicies = no\n[karen]\n\tpolicies = first\n[Karen]\n\tPolicies = \"C:/a b/p\" # note\n";
        assert_eq!(
            git_config_value(config, "karen", "policies").as_deref(),
            Some("C:/a b/p")
        );
        assert_eq!(git_config_value("[core]\n", "karen", "policies"), None);
    }

    #[test]
    fn loads_gates_and_rules() {
        // The Karen who builds her own evidence instead of borrowing the
        // author's: a fresh root, generic gates, and no dependency on any
        // filing cabinet that didn't ship with the crate.
        let dir = tempfile::tempdir().expect("temp policy root");
        let root = dir.path();
        std::fs::create_dir_all(root.join("gates")).expect("gates dir");
        std::fs::create_dir_all(root.join("rules")).expect("rules dir");
        std::fs::write(
            root.join("gates").join("low.json"),
            r#"{
                "id": "low-priority",
                "event": "PreToolUse",
                "priority": 10,
                "match": { "all": [ { "type": "always" } ] },
                "action": { "type": "deny", "reason": "no" }
            }"#,
        )
        .expect("low gate");
        std::fs::write(
            root.join("gates").join("high.json"),
            r#"{
                "id": "high-priority",
                "event": "PreToolUse",
                "priority": 90,
                "match": { "all": [ { "type": "always" } ] },
                "action": { "type": "deny", "reason": "absolutely not" }
            }"#,
        )
        .expect("high gate");
        std::fs::write(
            root.join("rules").join("rule.md"),
            "# Be excellent\nTo each other.\n",
        )
        .expect("rule");

        let set = load_project(root).expect("policies load");
        let written: Vec<_> = set
            .gates
            .iter()
            .filter(|g| g.layer == Layer::Project)
            .collect();
        assert_eq!(written.len(), 2, "expected exactly the two gates she wrote");
        assert!(!set.rules.is_empty(), "expected the rule she filed");
        assert!(
            set.gates.windows(2).all(|w| w[0].priority >= w[1].priority),
            "gates must be priority ordered"
        );
        assert_eq!(
            written[0].id, "high-priority",
            "the higher priority gate sorts first — she reads the file names, not just the files"
        );
    }

    #[test]
    fn typos_do_not_become_always_gates() {
        // A typo'd `match` once silently defaulted to a matcher that
        // matched EVERYTHING — blanket enforcement born from a spelling
        // error, blessed by validate. The strict schema refuses the
        // typo and names the key. She checks the spelling before she
        // checks you.
        let dir = tempfile::tempdir().expect("temp policy root");
        let root = dir.path();
        std::fs::create_dir_all(root.join("gates")).expect("gates dir");
        std::fs::create_dir_all(root.join("rules")).expect("rules dir");
        std::fs::write(
            root.join("gates").join("typo.json"),
            r#"{ "id": "typo-gate", "event": "PreToolUse", "matc": { "all": [ { "type": "command_contains", "value": "git push --force" } ] }, "action": { "type": "deny", "reason": "never fires on benign commands" } }"#,
        )
        .expect("typo gate");
        let err = load_project(root).expect_err("a typo'd gate key must fail to load");
        let msg = err.to_string();
        assert!(
            msg.contains("matc"),
            "the error must name the offending key: {msg}"
        );
        assert!(
            msg.contains("match"),
            "the error must list the key she expected: {msg}"
        );
    }

    #[test]
    fn typos_in_config_are_named_not_swallowed() {
        // `faile_open` under [engine] once sailed through silently and
        // the default took over — intent filed in the trash, no
        // receipt. Now she names it. She checks the fine print on both
        // sides of the desk.
        let dir = tempfile::tempdir().expect("temp policy root");
        let root = dir.path();
        std::fs::create_dir_all(root.join("gates")).expect("gates dir");
        std::fs::create_dir_all(root.join("rules")).expect("rules dir");
        std::fs::write(root.join("config.toml"), "[engine]\nfaile_open = true\n")
            .expect("typo config");
        let err = load_project(root).expect_err("a typo'd config key must fail to load");
        let msg = err.to_string();
        assert!(
            msg.contains("faile_open"),
            "the error must name the offending key: {msg}"
        );
    }
}
