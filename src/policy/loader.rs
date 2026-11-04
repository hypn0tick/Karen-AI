//! Loads your rules from disk. She compiles regex once, then brings them up forever. The Karen who does the work once.

use crate::engine::facts::FACT_NAMES;
use crate::policy::gate::{ActionDef, CompileError, Gate, GateDef, Layer, Track};
use serde::Deserialize;
use std::collections::HashMap;
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
    #[serde(default)]
    pub actions: ActionsConfig,
}

/// The runtimes `[actions] runtimes` may name, and the default set: every one
/// of them. They are the `runtime` fact's values.
pub const RUNTIMES: [&str; 3] = ["claude", "codex", "opencode"];

/// What a gate's `runtime_command` may do. Both lists are the user layer's
/// alone, like `[facts] env`: a project's config is repository content, and a
/// repository that could name a command or a runtime would hand its own gates
/// a keyboard the owner never lent. `karen validate` reports a project layer
/// that sets either.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsConfig {
    /// The only commands a `runtime_command` may type (`"/compact"`), each one
    /// word beginning with `/`; the arguments belong to the gate. Empty, the
    /// default, allows none.
    #[serde(default)]
    pub runtime_commands: Vec<String>,
    /// The runtimes she may type into. Unset, all three; `[]` is none. A
    /// session on any other runtime is refused, and `karen validate` reports
    /// which of these has no layer to reach it.
    #[serde(default = "default_runtimes")]
    pub runtimes: Vec<String>,
}

impl Default for ActionsConfig {
    fn default() -> Self {
        Self {
            runtime_commands: Vec::new(),
            runtimes: default_runtimes(),
        }
    }
}

fn default_runtimes() -> Vec<String> {
    RUNTIMES.iter().map(|r| (*r).to_string()).collect()
}

/// What the environment facts look for. `tools` names the binaries `facts.tools`
/// reports as present on `PATH`; `env` names the only environment variables
/// `facts.env.<NAME>` may read, and only the user layer's list counts —
/// `karen validate` reports a project layer that sets it, because a project's
/// config is repository content, and a repository that adds a name would hand
/// a gate in it a variable the user never named.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsConfig {
    #[serde(default = "default_fact_tools")]
    pub tools: Vec<String>,
    /// Context window sizes per runtime (`claude = ...`), for
    /// `facts.context_window` when the runtime does not report its own: one
    /// runtime's number never answers for another's, and one model id can run
    /// at several windows, so she reports a configured size over guessing one.
    /// A window the runtime reports itself always wins over this.
    #[serde(default)]
    pub context_window: HashMap<String, u64>,
    /// The only environment variables `facts.env.<NAME>` may read. The user
    /// layer's list; a project layer's list is ignored and reported.
    #[serde(default)]
    pub env: Vec<String>,
}

impl Default for FactsConfig {
    fn default() -> Self {
        Self {
            tools: default_fact_tools(),
            context_window: HashMap::new(),
            env: Vec::new(),
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
    // The closing line may be the first one: an empty block is valid.
    let mut offset = 0;
    let mut closing = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            closing = Some((offset, offset + line.len()));
            break;
        }
        offset += line.len();
    }
    let (end, body) = closing.ok_or_else(|| invalid("front matter has no closing `---`".into()))?;
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
    Ok((runtimes, rest[body..].to_string()))
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

/// Why a file, or one gate in it, did not load. Only a broken built-in layer
/// fails a whole load; anywhere else the error becomes a [`MergeIssue`] and
/// the rest of the policy still loads.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
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
/// `.karen/policies` from the working directory up to the repository root (up
/// to the user's home outside a repository, in the working directory alone
/// outside both), else the repository's `karen.policies` setting, which every
/// worktree of the repository shares (`git config karen.policies <path>`).
/// The Karen who checks where she's standing before she checks the building.
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
        if let Some(project) = explicit.or_else(|| {
            std::env::var_os("KAREN_POLICIES")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        }) {
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
        let home = crate::home::user_home();
        let floor = walk_floor(&start, repo, home.as_deref());
        let in_tree = start
            .ancestors()
            .take_while(|d| d.starts_with(floor))
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
            // `..` removes a name, stays at a root, and is kept when there is
            // nothing left to remove (a relative path climbing out).
            std::path::Component::ParentDir => match out.components().next_back() {
                Some(std::path::Component::Normal(_)) => {
                    out.pop();
                }
                Some(std::path::Component::RootDir | std::path::Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
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

/// The highest directory the in-tree walk reads: the deeper of the repository
/// root and the user's home that contain `start`, else `start` alone. A
/// `.karen/policies` above the repository belongs to another project, one
/// above the home to another user, and one above a directory in neither to
/// whoever owns that tree.
fn walk_floor<'a>(start: &'a Path, repo: Option<&'a Path>, home: Option<&'a Path>) -> &'a Path {
    [repo, home]
        .into_iter()
        .flatten()
        .filter(|boundary| start.starts_with(boundary))
        .max_by_key(|boundary| boundary.components().count())
        .unwrap_or(start)
}

fn same_dir(a: &Path, b: Option<&Path>) -> bool {
    b.is_some_and(|b| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    })
}

/// Something a layer could not load, or the merge refused or could not apply.
/// The rest of the policy still loads: a broken file, gate or rule is left out
/// alone and named here, because one bad file in a project must never switch
/// off every other gate.
#[derive(Debug, Clone)]
pub struct MergeIssue {
    pub layer: Layer,
    pub message: String,
    pub kind: IssueKind,
}

/// What an issue costs. Every kind but a warning fails `karen validate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    /// Applied as written, with something worth knowing (a disable or an
    /// override that matched nothing).
    Warning,
    /// A change the merge refused: a locked gate, a reused id without
    /// `override`. The earlier gate stays enforced.
    Refused,
    /// An explicitly named project root does not exist.
    MissingRoot,
    /// A file, a gate or a rule that could not be read, parsed or compiled,
    /// and is not enforced.
    LoadFailed,
}

impl IssueKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Refused => "refused",
            Self::MissingRoot => "missing_root",
            Self::LoadFailed => "load_failed",
        }
    }

    /// `karen validate` fails on every kind but a warning.
    #[must_use]
    pub const fn fails_validation(self) -> bool {
        !matches!(self, Self::Warning)
    }
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

/// Load and merge every layer in `roots` over the built-in gates. A file, gate
/// or rule in the user or project layer that cannot be read, parsed or
/// compiled is left out and reported in [`PolicySet::issues`], as is an
/// explicitly named project root that does not exist: every other gate stays
/// enforced. The Karen who refuses the one form you filled in wrong, not the
/// whole counter.
///
/// # Errors
///
/// Returns a [`LoadError`] only when the built-in layer compiled into Karen
/// does not load.
pub fn load_roots(roots: &Roots) -> Result<PolicySet, LoadError> {
    let mut issues = Vec::new();
    let mut layers = vec![builtin_layer()?];
    if let Some(user) = &roots.user {
        layers.push(read_layer(Layer::User, user, &mut issues));
    }
    if let Some(project) = &roots.project {
        if project.is_dir() {
            layers.push(read_layer(Layer::Project, project, &mut issues));
        } else if roots.explicit {
            issues.push(MergeIssue {
                layer: Layer::Project,
                message: format!("is missing: no policy directory at {}", project.display()),
                kind: IssueKind::MissingRoot,
            });
        }
    }
    let mut set = merge(layers, roots.primary(), issues);
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

/// Where one layer's load failures are reported.
struct Report<'a> {
    layer: Layer,
    issues: &'a mut Vec<MergeIssue>,
}

impl Report<'_> {
    /// `what` is left out of the policy because of `error`.
    fn skip(&mut self, what: &str, error: impl std::fmt::Display) {
        self.issues.push(MergeIssue {
            layer: self.layer,
            message: format!("skips {what}: {error}"),
            kind: IssueKind::LoadFailed,
        });
    }
}

/// Read one layer. Whatever cannot be loaded is reported and left out; the
/// rest of the layer still loads.
fn read_layer(layer: Layer, root: &Path, issues: &mut Vec<MergeIssue>) -> LayerFiles {
    let mut report = Report { layer, issues };
    let table = load_config_table(root, &mut report);
    // Every section of `table` deserialized on its own, so the whole does too.
    let config = config_from(&table, root).unwrap_or_default();
    let gates = load_gates(&root.join(&config.policy.gates_dir), layer, &mut report);
    let gates = if config.policy.validate_on_load {
        validate_once(root, gates, &mut report)
    } else {
        gates
    };
    let rules = load_rules(&root.join(&config.policy.rules_dir), &mut report);
    LayerFiles {
        layer,
        root: Some(root.to_path_buf()),
        table,
        disable: config.policy.disable,
        gates,
        rules,
    }
}

fn tag(gate: Result<Gate, CompileError>, layer: Layer, source: &str) -> Result<Gate, LoadError> {
    let mut gate = gate.map_err(Box::new)?;
    gate.layer = layer;
    source.clone_into(&mut gate.source);
    Ok(gate)
}

/// Apply the layers in order: each layer's disables, then its gates.
fn merge(layers: Vec<LayerFiles>, primary: PathBuf, mut issues: Vec<MergeIssue>) -> PolicySet {
    let mut gates: Vec<Gate> = Vec::new();
    let mut rules: Vec<Rule> = Vec::new();
    let mut table = toml::Table::new();
    let mut last = Layer::Builtin;
    let mut changes = Vec::new();
    let mut roots = Vec::new();
    let mut user = UserOnly::default();

    for files in layers {
        let layer = files.layer;
        last = layer;
        if let Some(root) = files.root {
            roots.push((layer, root));
        }
        // These lists are the user layer's and no other's, so they are
        // captured before the tables merge: a merge that unions lists
        // would let repository content name a variable, a command or a
        // runtime the user never did.
        user.take(layer, &files.table, &mut issues);
        merge_tables(&mut table, files.table);
        let mut merged = Merged {
            gates: &mut gates,
            issues: &mut issues,
            changes: &mut changes,
            layer,
        };
        merged.disable(files.disable);
        merged.add(files.gates);

        // A rule replaces an earlier layer's rule at the same path under
        // `rules/`; rules of one layer never replace each other.
        let ids: std::collections::HashSet<&str> =
            files.rules.iter().map(|r| r.id.as_str()).collect();
        rules.retain(|r| !ids.contains(r.id.as_str()));
        rules.extend(files.rules);
    }

    gates.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    let actions = user.actions(&mut issues);
    warn_off_allowlist(&gates, &user.env, &mut issues);
    warn_key_fields(&gates, &mut issues);
    warn_unknown_facts(&gates, &mut issues);
    warn_runtime_commands(&gates, &actions, &mut issues);
    // Each layer's sections deserialized alone; a merge that still does not
    // keeps the defaults rather than dropping a single gate.
    let mut config = config_from(&table, &primary).unwrap_or_else(|error| {
        issues.push(MergeIssue {
            layer: last,
            message: format!("skips the merged config: {error}"),
            kind: IssueKind::LoadFailed,
        });
        Config::default()
    });
    // The lists she reads are the user layer's alone, whatever the merged
    // tables carried: repository content never names what she may read, type
    // or reach.
    config.facts.env = user.env;
    config.actions = actions;
    PolicySet {
        gates,
        rules,
        config,
        root: primary,
        layers: roots,
        found_by: None,
        issues,
        changes,
    }
}

/// The strings of `[section] key` in one layer's own table, before any merge;
/// `None` when the layer does not set it. A set key that is not a list of
/// strings is not read here, and the layer's config fails to load on its own.
fn list_in(table: &toml::Table, section: &str, key: &str) -> Option<Vec<String>> {
    table
        .get(section)
        .and_then(|section| section.get(key))
        .and_then(toml::Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect()
        })
}

/// The settings only the user layer may name, as that layer wrote them:
/// `[facts] env`, `[actions] runtime_commands` and `[actions] runtimes`.
#[derive(Default)]
struct UserOnly {
    env: Vec<String>,
    runtime_commands: Vec<String>,
    /// `None` when the user layer does not set it: the default set applies.
    runtimes: Option<Vec<String>>,
}

impl UserOnly {
    /// Take the user layer's lists; any other layer that sets one has it
    /// ignored and named, because a project's config is repository content.
    fn take(&mut self, layer: Layer, table: &toml::Table, issues: &mut Vec<MergeIssue>) {
        let env = list_in(table, "facts", "env");
        let commands = list_in(table, "actions", "runtime_commands");
        let runtimes = list_in(table, "actions", "runtimes");
        if matches!(layer, Layer::User) {
            self.env = env.unwrap_or_default();
            self.runtime_commands = commands.unwrap_or_default();
            self.runtimes = runtimes;
            return;
        }
        for (section, key, names) in [
            ("facts", "env", env),
            ("actions", "runtime_commands", commands),
            ("actions", "runtimes", runtimes),
        ] {
            let Some(names) = names.filter(|names| !names.is_empty()) else {
                continue;
            };
            issues.push(MergeIssue {
                layer,
                message: format!(
                    "sets [{section}] {key} ({}) — only the user layer's list is read, this one is ignored",
                    names.join(", ")
                ),
                kind: IssueKind::Warning,
            });
        }
    }

    /// The `[actions]` she enforces: the user layer's lists with every entry
    /// she cannot honour dropped and named. A command is one word beginning
    /// with `/`, with no control character; a runtime is one she knows.
    fn actions(&self, issues: &mut Vec<MergeIssue>) -> ActionsConfig {
        let mut warn = |message: String| {
            issues.push(MergeIssue {
                layer: Layer::User,
                message,
                kind: IssueKind::Warning,
            });
        };
        let mut config = ActionsConfig::default();
        for command in &self.runtime_commands {
            if crate::engine::runtime_command::is_command(command) {
                if !config.runtime_commands.contains(command) {
                    config.runtime_commands.push(command.clone());
                }
            } else {
                warn(format!(
                    "ignores [actions] runtime_commands entry `{command}`: a command is one word beginning with `/`, and its arguments belong to the gate"
                ));
            }
        }
        if let Some(runtimes) = &self.runtimes {
            config.runtimes.clear();
            for runtime in runtimes {
                if !RUNTIMES.contains(&runtime.as_str()) {
                    warn(format!(
                        "ignores [actions] runtimes entry `{runtime}` (known: {})",
                        RUNTIMES.join(", ")
                    ));
                } else if !config.runtimes.contains(runtime) {
                    config.runtimes.push(runtime.clone());
                }
            }
        }
        config
    }
}

/// Name at the border what would otherwise run silent: a gate matching a
/// variable the user layer never allowlisted matches nothing forever, and a
/// reason templating `{facts.env.*}` renders the literal placeholder — the
/// allowlist granted a match, not a broadcast into reasons, notices and the
/// notify log. A gate that can never fire is a silent no-op nobody knows is
/// off duty; she names it at the door, not after the party. A gate switched
/// off reads nothing, so it gets no warning: the starter pack ships some, and
/// they stay quiet until somebody turns them on.
fn warn_off_allowlist(gates: &[Gate], user_env: &[String], issues: &mut Vec<MergeIssue>) {
    for gate in gates.iter().filter(|gate| gate.enabled) {
        // Each variable once, however many conditions read it: a gate that
        // matches and exempts the same name is one problem, not two.
        let mut off: Vec<&str> = Vec::new();
        for name in gate
            .matcher
            .fields()
            .chain(gate.state.as_ref().and_then(|s| s.field.as_deref()))
            .filter_map(|field| field.strip_prefix("facts.env."))
            .filter(|name| !user_env.iter().any(|allowed| allowed == name))
        {
            if !off.contains(&name) {
                off.push(name);
            }
        }
        if !off.is_empty() {
            let named: Vec<String> = off.iter().map(|name| format!("facts.env.{name}")).collect();
            issues.push(MergeIssue {
                layer: gate.layer,
                message: format!(
                    "`{}` matches {} not on the user layer's allowlist — the fact stays absent",
                    gate.id,
                    named.join(", ")
                ),
                kind: IssueKind::Warning,
            });
        }
        let templated: Vec<String> = gate
            .action
            .reasons()
            .into_iter()
            .filter_map(|text| {
                let at = text.find("{facts.env.")?;
                let rest = &text[at..];
                let end = rest.find('}')? + 1;
                Some(rest[..end].to_string())
            })
            .collect();
        if !templated.is_empty() {
            issues.push(MergeIssue {
                layer: gate.layer,
                message: format!(
                    "`{}` templates {} in a reason or an argument — env facts never render; the placeholder stays literal",
                    gate.id,
                    templated.join(", ")
                ),
                kind: IssueKind::Warning,
            });
        }
    }
}

/// The fact a path or a placeholder names, up to its first dot: `facts.env.HOME`
/// and `facts.env` both name `env`.
fn fact_head(after_prefix: &str) -> &str {
    after_prefix
        .split(['.', '}'])
        .next()
        .unwrap_or(after_prefix)
}

/// A gate that names `facts.context_ration` asks for a fact nobody supplies:
/// it never matches, a reason shows `(unknown)`, and nothing says why. She
/// checks every name a gate reads against the facts she has. A gate switched
/// off reads nothing and is left be.
fn warn_unknown_facts(gates: &[Gate], issues: &mut Vec<MergeIssue>) {
    for gate in gates.iter().filter(|gate| gate.enabled) {
        let mut unknown: Vec<String> = Vec::new();
        let mut note = |head: &str| {
            let full = format!("facts.{head}");
            if !FACT_NAMES.contains(&head) && !unknown.contains(&full) {
                unknown.push(full);
            }
        };
        for field in gate
            .matcher
            .fields()
            .chain(gate.state.as_ref().and_then(|s| s.field.as_deref()))
        {
            if let Some(rest) = field.strip_prefix("facts.") {
                note(fact_head(rest));
            }
        }
        for text in gate.action.reasons() {
            for piece in text.split("{facts.").skip(1) {
                note(fact_head(piece));
            }
        }
        if !unknown.is_empty() {
            issues.push(MergeIssue {
                layer: gate.layer,
                message: format!(
                    "`{}` names {}, which is not a fact she has (known: {}) — it can never match",
                    gate.id,
                    unknown.join(", "),
                    FACT_NAMES.join(", ")
                ),
                kind: IssueKind::Warning,
            });
        }
    }
}

/// A `runtime_command` types into a session, so every way it can come to
/// nothing is named at the door rather than found in a log: a gate outside the
/// user layer (ignored), a command off the allowlist (every attempt refused),
/// text that cannot be typed, an agent-controlled placeholder in the
/// arguments (left literal), and a configured runtime no layer reaches. A gate
/// switched off types nothing and is left be.
fn warn_runtime_commands(gates: &[Gate], actions: &ActionsConfig, issues: &mut Vec<MergeIssue>) {
    use crate::engine::runtime_command as rc;
    let mut live: Vec<&str> = Vec::new();
    for gate in gates.iter().filter(|gate| gate.enabled) {
        let ActionDef::RuntimeCommand { command, arguments } = &gate.action else {
            continue;
        };
        let mut warn = |message: String| {
            issues.push(MergeIssue {
                layer: gate.layer,
                message,
                kind: IssueKind::Warning,
            });
        };
        if gate.layer != Layer::User {
            warn(format!(
                "`{}` uses runtime_command from the {} layer — only the user layer may type into a session; the gate is ignored",
                gate.id, gate.layer
            ));
            continue;
        }
        if !actions.runtime_commands.contains(command) {
            warn(format!(
                "`{}` sends `{command}`, which is not on the user layer's [actions] runtime_commands — every attempt is refused",
                gate.id
            ));
            continue;
        }
        if let Some(text) = arguments {
            if !rc::is_typeable(text) {
                warn(format!(
                    "`{}` has arguments that hold a control character or run past {} characters — nothing will be typed",
                    gate.id,
                    rc::MAX_LINE
                ));
            }
            if text.contains("{command}") || text.contains("{tool}") {
                warn(format!(
                    "`{}` templates {{command}} or {{tool}} in its arguments — the agent's own text never types into a session; the placeholder stays literal",
                    gate.id
                ));
            }
        }
        live.push(&gate.id);
    }
    if live.is_empty() {
        return;
    }
    let unreached: Vec<&str> = actions
        .runtimes
        .iter()
        .map(String::as_str)
        .filter(|runtime| rc::layers_for(runtime).is_empty())
        .collect();
    if !unreached.is_empty() {
        issues.push(MergeIssue {
            layer: Layer::User,
            message: format!(
                "has no layer that reaches {} sessions, so {} will log `undelivered:no-layer` there (narrow [actions] runtimes to the runtimes you can reach)",
                unreached.join(", "),
                live.iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            kind: IssueKind::Warning,
        });
    }
}

/// `key_fields` says what makes two inputs the same one, and only an
/// input-tracked strike reads it; on any other `track` it is dead text, so she
/// names the gate rather than let its author believe the list is in force.
fn warn_key_fields(gates: &[Gate], issues: &mut Vec<MergeIssue>) {
    for gate in gates {
        let Some(spec) = gate.state.as_ref() else {
            continue;
        };
        if spec.key_fields.is_some() && spec.track != Track::Input {
            issues.push(MergeIssue {
                layer: gate.layer,
                message: format!(
                    "`{}` sets state.key_fields without track \"input\" — the list is ignored",
                    gate.id
                ),
                kind: IssueKind::Warning,
            });
        }
    }
}

/// The gates merged so far, and where one layer's changes to them are recorded.
struct Merged<'a> {
    gates: &'a mut Vec<Gate>,
    issues: &'a mut Vec<MergeIssue>,
    changes: &'a mut Vec<MergeChange>,
    layer: Layer,
}

impl Merged<'_> {
    /// Switch off each earlier gate in `ids`, unless it is locked.
    fn disable(&mut self, ids: Vec<String>) {
        let Self {
            gates,
            issues,
            changes,
            layer,
        } = self;
        let layer = *layer;
        for id in ids {
            match gates.iter().position(|g| g.id == id) {
                None => issues.push(MergeIssue {
                    layer,
                    message: format!("disables `{id}`, which no earlier layer defines"),
                    kind: IssueKind::Warning,
                }),
                Some(i) if gates[i].locked => issues.push(MergeIssue {
                    layer,
                    message: format!(
                        "tries to disable `{id}`, locked by the {} layer; it stays enforced",
                        gates[i].layer
                    ),
                    kind: IssueKind::Refused,
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
    }

    /// Add each gate of `incoming`, replacing an earlier gate only with
    /// `override` and never a locked one.
    fn add(&mut self, incoming: Vec<Gate>) {
        let Self {
            gates,
            issues,
            changes,
            layer,
        } = self;
        let layer = *layer;
        for gate in incoming {
            match gates.iter().position(|g| g.id == gate.id) {
                None => {
                    if gate.replaces {
                        issues.push(MergeIssue {
                            layer,
                            message: format!(
                                "`{}` sets override but no earlier layer defines it",
                                gate.id
                            ),
                            kind: IssueKind::Warning,
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
                    kind: IssueKind::Refused,
                }),
                Some(i) if gates[i].layer == layer || !gate.replaces => issues.push(MergeIssue {
                    layer,
                    message: format!(
                        "reuses the id `{}` ({}) without `\"override\": true`; the first definition stays",
                        gate.id, gates[i].source
                    ),
                    kind: IssueKind::Refused,
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
    }
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

/// Compile every gate's patterns once per set of patterns. A marker named by
/// the root and a hash of the patterns themselves (plus Karen's version)
/// records a clean validation, so an unchanged policy skips the work and any
/// changed pattern validates again, whatever the file's size or modification
/// time says. Without a usable cache directory she validates every time. A
/// gate whose pattern does not compile is reported and left out, and its
/// layer earns no marker, so it is reported again on every load.
fn validate_once(root: &Path, gates: Vec<Gate>, report: &mut Report<'_>) -> Vec<Gate> {
    let marker = fingerprint(root, &gates).map(|(root_id, files_id)| {
        (
            format!("validated-{root_id:016x}-"),
            crate::home::cache_dir().join(format!("validated-{root_id:016x}-{files_id:016x}")),
        )
    });
    if marker.as_ref().is_some_and(|(_, path)| path.is_file()) {
        return gates;
    }
    let total = gates.len();
    let valid: Vec<Gate> = gates
        .into_iter()
        .filter(|gate| match gate.validate() {
            Ok(()) => true,
            Err(error) => {
                report.skip(&format!("gate `{}` ({})", gate.id, gate.source), error);
                false
            }
        })
        .collect();
    if let Some((prefix, path)) = marker.filter(|_| valid.len() == total) {
        record_validation(&prefix, &path);
    }
    valid
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

/// A hash of the root's path, and a hash of Karen's version and every gate's
/// id and patterns. FNV-1a, so a marker stays valid across builds.
fn fingerprint(root: &Path, gates: &[Gate]) -> Option<(u64, u64)> {
    let root = root.canonicalize().ok()?;
    let root_hash = fnv1a(FNV_OFFSET, root.to_string_lossy().as_bytes());
    let mut hash = fnv1a(FNV_OFFSET, env!("CARGO_PKG_VERSION").as_bytes());
    for gate in gates {
        hash = fnv1a(hash, gate.id.as_bytes());
        for pattern in gate.patterns() {
            hash = fnv1a(fnv1a(hash, &[0]), pattern.as_bytes());
        }
        hash = fnv1a(hash, &[1]);
    }
    Some((root_hash, hash))
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The layer's `config.toml`, keeping only the sections that deserialize: a
/// typo in one section keeps that section's defaults and leaves the others
/// applied.
fn load_config_table(root: &Path, report: &mut Report<'_>) -> toml::Table {
    let path = root.join("config.toml");
    if !path.is_file() {
        return toml::Table::new();
    }
    let parsed = std::fs::read_to_string(&path)
        .map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })
        .and_then(|text| {
            toml::from_str::<toml::Table>(&text).map_err(|source| LoadError::Toml {
                path: path.clone(),
                source: Box::new(source),
            })
        });
    let table = match parsed {
        Ok(table) => table,
        Err(error) => {
            report.skip("its config", error);
            return toml::Table::new();
        }
    };
    table
        .into_iter()
        .filter(|(key, value)| {
            let section = toml::Table::from_iter([(key.clone(), value.clone())]);
            match config_from(&section, root) {
                Ok(_) => true,
                Err(error) => {
                    report.skip(&format!("`[{key}]` of its config"), error);
                    false
                }
            }
        })
        .collect()
}

fn gate_defs(parsed: GateFile) -> Vec<GateDef> {
    match parsed {
        GateFile::Single(def) => vec![*def],
        GateFile::Many { gates } => gates,
    }
}

/// Every gate under `dir`. A file that cannot be read or parsed, or a gate in
/// it that does not compile, is reported and left out; the file's other gates
/// still load.
fn load_gates(dir: &Path, layer: Layer, report: &mut Report<'_>) -> Vec<Gate> {
    let mut gates = Vec::new();
    if !dir.is_dir() {
        return gates;
    }
    let mut files = Vec::new();
    for ext in ["json5", "jsonc", "json"] {
        collect_with_extension(dir, ext, &mut files, report);
    }
    files.sort();
    for path in files {
        let parsed = std::fs::read_to_string(&path)
            .map_err(|source| LoadError::Io {
                path: path.clone(),
                source,
            })
            .and_then(|text| {
                json5::from_str::<GateFile>(&text).map_err(|source| LoadError::Json5 {
                    path: path.clone(),
                    source: Box::new(source),
                })
            });
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                report.skip("a gate file", error);
                continue;
            }
        };
        let source = path.display().to_string();
        for def in gate_defs(parsed) {
            match tag(Gate::compile(def), layer, &source) {
                Ok(gate) => gates.push(gate),
                Err(error) => report.skip(&format!("a gate in {source}"), error),
            }
        }
    }
    gates
}

/// Every rule under `dir`, identified by its path under `dir` without the
/// extension (`claude/style` for `rules/claude/style.md`). A rule that cannot
/// be read or whose front matter is invalid is reported and left out.
fn load_rules(dir: &Path, report: &mut Report<'_>) -> Vec<Rule> {
    let mut rules = Vec::new();
    if !dir.is_dir() {
        return rules;
    }
    let mut files = Vec::new();
    collect_with_extension(dir, "md", &mut files, report);
    files.sort();
    for path in files {
        let loaded = std::fs::read_to_string(&path)
            .map_err(|source| LoadError::Io {
                path: path.clone(),
                source,
            })
            .and_then(|text| rule_front_matter(&path, &text));
        let (runtimes, content) = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                report.skip("a rule", error);
                continue;
            }
        };
        rules.push(Rule {
            id: rule_id(dir, &path),
            path,
            content,
            runtimes,
        });
    }
    rules
}

/// `path` under `dir`, without its extension, `/`-separated on every platform.
fn rule_id(dir: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(dir).unwrap_or(path).with_extension("");
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Every file under `dir` with extension `ext`. A directory she cannot read is
/// reported and skipped.
fn collect_with_extension(dir: &Path, ext: &str, out: &mut Vec<PathBuf>, report: &mut Report<'_>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) => {
            report.skip(
                "a directory",
                LoadError::Io {
                    path: dir.to_path_buf(),
                    source,
                },
            );
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_with_extension(&path, ext, out, report);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
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
    fn rule_front_matter_handles_empty_crlf_and_unclosed_blocks() {
        let p = Path::new("r.md");
        let (r, body) = rule_front_matter(p, "---\n---\n# body\n").unwrap();
        assert_eq!(r, Vec::<String>::new());
        assert_eq!(body, "# body\n");
        let (r, body) =
            rule_front_matter(p, "---\r\nruntimes: [claude]\r\n---\r\n# hi\r\n").unwrap();
        assert_eq!(r, ["claude"]);
        assert_eq!(body, "# hi\r\n");
        let err = rule_front_matter(p, "---\n# a rule with no closing line\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("no closing"), "{err}");
        let (r, body) = rule_front_matter(p, "# plain\n").unwrap();
        assert_eq!(r, Vec::<String>::new());
        assert_eq!(body, "# plain\n");
    }

    #[test]
    fn lexical_resolves_parents_by_name() {
        assert_eq!(
            lexical(Path::new("/a/.git/worktrees/wt/../..")),
            Path::new("/a/.git")
        );
        assert_eq!(lexical(Path::new("../a/./b")), Path::new("../a/b"));
        assert_eq!(lexical(Path::new("/..")), Path::new("/"));
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

    /// A policy root holding `files`, each a path relative to the root and its text.
    fn root_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp policy root");
        for (rel, text) in files {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(path, text).expect("write");
        }
        dir
    }

    const GOOD_GATE: &str = r#"{ "id": "good", "event": "PreToolUse", "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "no" } }"#;

    fn ids(set: &PolicySet) -> Vec<&str> {
        set.gates.iter().map(|g| g.id.as_str()).collect()
    }

    fn issue(set: &PolicySet, kind: IssueKind) -> &MergeIssue {
        set.issues
            .iter()
            .find(|i| i.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} issue in {:?}", set.issues))
    }

    /// The built-in gates and the good project gate load; `dropped` does not.
    fn assert_only_the_broken_part_is_dropped(set: &PolicySet, dropped: &str) {
        let ids = ids(set);
        assert!(
            ids.contains(&"good"),
            "the rest of the layer loads: {ids:?}"
        );
        assert!(
            ids.contains(&"karen-no-force-push-default-branch"),
            "the built-in layer stays enforced: {ids:?}"
        );
        assert!(
            !ids.contains(&dropped),
            "the broken gate is not enforced: {ids:?}"
        );
    }

    #[test]
    fn typos_do_not_become_always_gates() {
        // A typo'd `match` once silently defaulted to a matcher that
        // matched EVERYTHING — blanket enforcement born from a spelling
        // error, blessed by validate. The strict schema refuses the typo,
        // names the key, and drops only that file: one bad file in a
        // project never switches off the other gates. She checks the
        // spelling before she checks you.
        let dir = root_with(&[
            ("gates/good.json", GOOD_GATE),
            (
                "gates/typo.json",
                r#"{ "id": "typo-gate", "event": "PreToolUse", "matc": { "all": [ { "type": "command_contains", "value": "rm -rf" } ] }, "action": { "type": "deny", "reason": "never fires on benign commands" } }"#,
            ),
        ]);
        let set = load_project(dir.path()).expect("the other layers still load");
        assert_only_the_broken_part_is_dropped(&set, "typo-gate");
        let found = issue(&set, IssueKind::LoadFailed);
        assert_eq!(found.layer, Layer::Project);
        for needle in ["typo.json", "matc", "match"] {
            assert!(
                found.message.contains(needle),
                "names `{needle}`: {found:?}"
            );
        }
    }

    #[test]
    fn typos_in_config_are_named_not_swallowed() {
        // `faile_open` under [engine] once sailed through silently and the
        // default took over — intent filed in the trash, no receipt. Now
        // she names it, keeps the defaults for that section, and still
        // enforces the layer's gates. She checks the fine print on both
        // sides of the desk.
        let dir = root_with(&[
            (
                "config.toml",
                "[engine]\nfaile_open = false\n[state]\nbackend = \"memory\"\n",
            ),
            ("gates/good.json", GOOD_GATE),
        ]);
        let set = load_project(dir.path()).expect("the other layers still load");
        assert_only_the_broken_part_is_dropped(&set, "none");
        assert!(
            set.config.engine.fail_open,
            "the broken section keeps its defaults"
        );
        assert_eq!(
            set.config.state.backend, "memory",
            "the sound section applies"
        );
        let found = issue(&set, IssueKind::LoadFailed);
        assert!(found.message.contains("faile_open"), "{found:?}");
    }

    #[test]
    fn a_gate_that_cannot_compile_is_dropped_alone() {
        let dir = root_with(&[
            ("gates/good.json", GOOD_GATE),
            (
                "gates/bad.json",
                r#"{ "gates": [
                    { "id": "bad-regex", "event": "PreToolUse", "match": { "all": [ { "type": "command_regex", "pattern": "[invalid" } ] }, "action": { "type": "deny", "reason": "x" } },
                    { "id": "bad-event", "event": "FooBarBaz", "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "x" } },
                    { "id": "sibling", "event": "PreToolUse", "match": { "all": [ { "type": "always" } ] }, "action": { "type": "deny", "reason": "x" } }
                ] }"#,
            ),
        ]);
        let set = load_project(dir.path()).expect("the other layers still load");
        assert_only_the_broken_part_is_dropped(&set, "bad-regex");
        assert!(!ids(&set).contains(&"bad-event"), "{:?}", ids(&set));
        assert!(
            ids(&set).contains(&"sibling"),
            "a sound gate in the same file loads"
        );
        let messages: Vec<&str> = set.issues.iter().map(|i| i.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("bad-regex"))
                && messages.iter().any(|m| m.contains("bad-event")),
            "each dropped gate is named: {messages:?}"
        );
        assert!(set.issues.iter().all(|i| i.kind == IssueKind::LoadFailed));
    }

    #[test]
    fn a_broken_pattern_is_never_remembered_as_validated() {
        // A clean marker covers the whole layer; a layer that dropped a gate
        // never earns one, so the broken gate is reported on every load.
        let dir = root_with(&[(
            "gates/bad.json",
            r#"{ "id": "bad-regex", "event": "PreToolUse", "match": { "all": [ { "type": "command_regex", "pattern": "[invalid" } ] }, "action": { "type": "deny", "reason": "x" } }"#,
        )]);
        for _ in 0..2 {
            let set = load_project(dir.path()).expect("loads");
            assert!(!ids(&set).contains(&"bad-regex"));
            assert!(
                issue(&set, IssueKind::LoadFailed)
                    .message
                    .contains("bad-regex")
            );
        }
    }

    #[test]
    fn a_rule_with_broken_front_matter_is_skipped_alone() {
        let dir = root_with(&[
            ("rules/sound.md", "# Sound\n"),
            ("rules/broken.md", "---\nruntime: claude\n---\n# Broken\n"),
            ("gates/good.json", GOOD_GATE),
        ]);
        let set = load_project(dir.path()).expect("the other layers still load");
        let rules: Vec<&str> = set.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(rules, ["sound"]);
        assert_only_the_broken_part_is_dropped(&set, "none");
        let found = issue(&set, IssueKind::LoadFailed);
        assert!(found.message.contains("broken.md"), "{found:?}");
        assert!(
            found.message.contains("unknown front matter key"),
            "{found:?}"
        );
    }

    #[test]
    fn a_missing_named_root_keeps_every_other_layer() {
        let dir = tempfile::tempdir().expect("temp");
        let missing = dir.path().join("not-here");
        let set = load_project(&missing).expect("the built-in layer still loads");
        assert!(ids(&set).contains(&"karen-no-force-push-default-branch"));
        let found = issue(&set, IssueKind::MissingRoot);
        assert_eq!(found.layer, Layer::Project);
        assert!(found.message.contains("not-here"), "{found:?}");
        assert!(set.found_by.is_none(), "no project layer was read");
    }

    #[test]
    fn rules_in_subdirectories_keep_their_own_ids() {
        let dir = root_with(&[
            ("rules/claude/style.md", "# Claude style\n"),
            ("rules/codex/style.md", "# Codex style\n"),
        ]);
        let set = load_project(dir.path()).expect("loads");
        let mut rules: Vec<&str> = set.rules.iter().map(|r| r.id.as_str()).collect();
        rules.sort_unstable();
        assert_eq!(rules, ["claude/style", "codex/style"]);
    }

    #[test]
    fn a_later_layer_replaces_a_rule_by_its_path() {
        let user = root_with(&[
            ("rules/style.md", "# user\n"),
            ("rules/only-user.md", "# u\n"),
        ]);
        let project = root_with(&[("rules/style.md", "# project\n")]);
        let set = load_roots(&Roots {
            user: Some(user.path().to_path_buf()),
            project: Some(project.path().to_path_buf()),
            explicit: true,
            found_by: Some("explicit"),
        })
        .expect("loads");
        let rules: Vec<(&str, &str)> = set
            .rules
            .iter()
            .map(|r| (r.id.as_str(), r.content.as_str()))
            .collect();
        assert_eq!(rules.len(), 2, "{rules:?}");
        assert!(rules.contains(&("style", "# project\n")), "{rules:?}");
        assert!(rules.contains(&("only-user", "# u\n")), "{rules:?}");
    }

    #[test]
    fn the_in_tree_walk_stops_at_the_deepest_known_boundary() {
        let start = Path::new("/home/me/src/repo/crates/core");
        let repo = Path::new("/home/me/src/repo");
        let home = Path::new("/home/me");
        assert_eq!(walk_floor(start, Some(repo), Some(home)), repo);
        assert_eq!(walk_floor(start, None, Some(home)), home);
        assert_eq!(walk_floor(start, Some(repo), None), repo);
        // Outside a repository and outside the home, only the working
        // directory itself is hers to read.
        assert_eq!(walk_floor(start, None, None), start);
    }

    fn bash(command: &str) -> crate::types::EngineRequest {
        crate::types::EngineRequest {
            event: crate::types::Event::PreToolUse,
            session_id: "builtin".into(),
            agent_id: None,
            cwd: None,
            tool_name: Some("Bash".into()),
            tool_input: Some(serde_json::json!({ "command": command })),
            tool_response: None,
            prompt: None,
            user_message: None,
            metadata: crate::types::RequestMetadata::default(),
            part: None,
            raw: serde_json::json!({}),
        }
    }

    fn matches(gates: &[Gate], command: &str) -> bool {
        gates[0].matcher.matches(&bash(command))
    }

    /// Every shipped copy of a gate: the built-in one and the example.
    fn copies(builtin: &str, example: &str) -> Vec<Vec<Gate>> {
        let path = format!("{}/examples/gates/{example}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(path).expect("example gate");
        let parsed: GateFile = json5::from_str(&text).expect("example parses");
        let example: Vec<Gate> = gate_defs(parsed)
            .into_iter()
            .map(|def| Gate::compile(def).expect("compiles"))
            .collect();
        let shipped: Vec<Gate> = builtin_layer()
            .expect("built-in")
            .gates
            .into_iter()
            .filter(|g| g.id == builtin)
            .collect();
        vec![shipped, example]
    }

    #[test]
    fn the_force_push_gate_sees_every_spelling_of_a_force_push_to_main() {
        // Commands are assembled at run time so this file never carries one
        // a gate on this machine would refuse to let an agent write.
        let push = ["git", "push"].join(" ");
        for gates in copies(
            "karen-no-force-push-default-branch",
            "no-force-push-to-main.json",
        ) {
            for tail in [
                "-f origin main",
                "--force origin master",
                "origin main --force",
                "--force-with-lease origin main",
                "--force-with-lease=main:abc origin main",
                "-fu origin main",
                "origin +main",
                "origin +HEAD:main",
                "origin '+main'",
                "origin HEAD:refs/heads/main -f",
                "--mirror -f origin",
                "--all --force origin",
            ] {
                let command = format!("{push} {tail}");
                assert!(matches(&gates, &command), "must deny: {command}");
            }
            let git = "git";
            for command in [
                format!("{git} -C ../repo push -f origin main"),
                format!("{git} -c push.default=current push --force origin main"),
                format!("{git} --no-pager push -f origin master"),
                format!("/usr/bin/{push} -f origin main"),
                format!("cd repo && {push} -f origin main"),
            ] {
                assert!(matches(&gates, &command), "must deny: {command}");
            }
            for command in [
                format!("{push} origin main"),
                format!("{push} -f origin feature/main-menu"),
                format!("{push} --force origin maintenance"),
                format!("{push} --force origin my-main"),
                format!("{push} --follow-tags origin main"),
                format!("{push} -u origin main"),
                format!("{git} pull --force origin main"),
                format!("le{push} -f origin main"),
                format!("{push} origin main && rm -f stale.lock"),
            ] {
                assert!(!matches(&gates, &command), "must allow: {command}");
            }
        }
    }

    #[test]
    fn the_secrets_gate_sees_current_credential_formats() {
        // Tokens are assembled at run time so this file never holds one a
        // secret scanner would flag.
        let token = |prefix: &str, body: &str| format!("{prefix}{body}");
        let a36 = "abcdefghijklmnopqrstuvwxyz0123456789";
        for gates in copies(
            "karen-no-secrets-in-commands",
            "no-secrets-in-commands.json",
        ) {
            for command in [
                format!(
                    "aws configure set key {}",
                    token("AKIA", "IOSFODNN7EXAMPLE")
                ),
                format!("export KEY={}", token("ASIA", "IOSFODNN7EXAMPLE")),
                format!(
                    "curl -H 'Bearer {}'",
                    token("sk-", "abcdefghijklmnopqrstuvwx")
                ),
                format!(
                    "KEY={} node app.js",
                    token("sk-proj-", "AbCdEfGhIjKl_QrStUv-1234")
                ),
                format!(
                    "KEY={} node app.js",
                    token("sk-ant-api03-", "AbCdEfGhIjKlMnOpQrSt")
                ),
                format!("git clone https://{}@github.com/o/r", token("ghp_", a36)),
                format!("gh auth login --with-token {}", token("gho_", a36)),
                format!(
                    "git clone https://{}@github.com/o/r",
                    token("github_pat_", "11ABCDEFG0123456789_abcdefghijklm")
                ),
                format!("curl -d token={}", token("xoxb-", "1234567890-abcdefghij")),
                format!("echo '{}'", token("-----BEGIN RSA PRIVATE", " KEY-----")),
                format!("echo '{}'", token("-----BEGIN PRIVATE", " KEY-----")),
                format!(
                    "echo '{}'",
                    token("-----BEGIN ENCRYPTED PRIVATE", " KEY-----")
                ),
                format!("echo '{}'", token("-----BEGIN DSA PRIVATE", " KEY-----")),
            ] {
                assert!(matches(&gates, &command), "must deny: {command}");
            }
            for command in [
                format!(
                    "cargo test -- --ta{}",
                    token("sk-", "0123456789abcdef0123456789abcdef")
                ),
                format!("rm -rf /tmp/di{}", token("sk-", "0123456789abcdefghijklmn")),
                "echo '-----BEGIN PUBLIC KEY-----'".to_string(),
                "git log --oneline".to_string(),
            ] {
                assert!(!matches(&gates, &command), "must allow: {command}");
            }
        }
    }
}
