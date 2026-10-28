use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::generate;
use karen::adapters::{AdapterError, RuntimeAdapter};
use karen::engine::{Engine, maintenance};
use karen::policy::{self, ActionDef, LoadError, PolicySet};
use karen::state::{FileStore, MemoryStore, ReadOnlyStore, StateStore};
use karen::types::{Action, AdapterType, EngineRequest, EngineResponse, Event, InjectionKind};
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const EXIT_OK: i32 = 0;
const EXIT_INPUT: i32 = 2;
const EXIT_NOT_FOUND: i32 = 3;
const EXIT_CONFIG: i32 = 78;

const EXAMPLES: &str = "\
EXAMPLES:
  karen hook --adapter claude-classic < payload.json
  karen validate
  karen validate --json
  karen evaluate --event PreToolUse --tool Bash --command 'git commit -m \"x\"'
  karen schema hook
  karen completion bash > /usr/local/share/bash-completion/completions/karen
  karen completion zsh > /usr/local/share/zsh/site-functions/_karen
  karen completion fish > ~/.config/fish/completions/karen.fish
  karen setup claude
  karen setup codex
  karen setup opencode
  karen setup --dry-run claude

EXIT CODES:
  0   success
  2   usage or invalid input
  3   requested file not found
  78  invalid or missing policies

`hook` prints the runtime protocol object on stdout and nothing else.
`evaluate` is a dry run: it never writes state, so rehearsing the apology can't cost you your only warning.
`setup` with --dry-run previews changes without writing files.";

#[derive(Parser)]
#[command(name = "karen", version, about, after_help = EXAMPLES)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run as an external hook: JSON in on stdin, protocol JSON out on stdout, and nothing else — not even her opinions, of which there are several. The Karen who respects the protocol, and audits yours.
    Hook {
        #[arg(long, value_enum, default_value_t = Adapter::ClaudeClassic)]
        adapter: Adapter,
        #[arg(long)]
        policies: Option<PathBuf>,
        /// Deliver one rule-injection part (1-based). She chunks rules so the harness can't truncate her mid-lecture. The Karen who plans ahead — surprises are for parties she wasn't invited to.
        #[arg(long)]
        part: Option<usize>,
    },
    /// Validate policy files and print a summary. The Karen who checks first, because "it looked fine" is not a validation result — and she'll be quoting you on it.
    Validate(ValidateArgs),
    /// Report policy health, including every gate and rule — all of them, by name. The Karen who leaves no stone unturned, then documents the stones.
    Check(ValidateArgs),
    /// Evaluate one request without touching runtime state (dry run). The Karen who runs rehearsals that don't go on your record, and wants that noted.
    Evaluate(EvaluateArgs),
    /// Describe commands and policy vocabulary as JSON. The Karen who documents herself, reads yours with a pen, and has already written the review.
    Schema(SchemaArgs),
    /// Generate shell completions. The Karen who finishes your sentences, because waiting for you was never on the table.
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Install karen hooks for AI runtimes. Global by default; use --project for project-level. The Karen who sets up for you, because she doesn't want to have to call the manager already.
    Setup(SetupArgs),
}

#[derive(Args)]
struct ValidateArgs {
    #[arg(long)]
    policies: Option<PathBuf>,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct EvaluateArgs {
    #[arg(long, value_enum, default_value_t = Adapter::ClaudeClassic)]
    adapter: Adapter,
    #[arg(long)]
    policies: Option<PathBuf>,
    /// Raw runtime payload from a file, or `-` for stdin. The Karen who reads stdin the way other people read contracts — thoroughly, and out loud.
    #[arg(long, value_name = "FILE|-")]
    request: Option<String>,
    /// Event name, e.g. `PreToolUse`, `UserPromptSubmit`, `SessionStart`. The Karen who classifies first and asks questions later, in writing.
    #[arg(long, value_name = "EVENT")]
    event: Option<String>,
    #[arg(long)]
    tool: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    command: Option<String>,
    #[arg(long)]
    file_path: Option<String>,
    #[arg(long)]
    prompt: Option<String>,
    #[arg(long, default_value = "evaluate")]
    session: String,
    #[arg(long)]
    cwd: Option<String>,
    /// Include the full injected text in JSON output. The Karen who shows you the whole document, under protest, and points at the parts you clearly didn't read.
    #[arg(long)]
    include_content: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SchemaArgs {
    /// Limit the response to one command, e.g. `hook` or `evaluate`. The Karen who focuses on one thing at a time, unlike some agents she could name.
    topic: Option<String>,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(Args)]
struct SetupArgs {
    /// Runtime to install hooks for. The Karen who doesn't play favorites — she distrusts all runtimes equally.
    #[arg(value_enum)]
    runtime: Option<SetupRuntime>,
    /// Install hooks project-local (under .claude/, .codex/, .opencode/) instead of global. The Karen who scopes, because sometimes you only want her watching THIS repo.
    #[arg(long)]
    project: bool,
    /// Preview changes without writing files. The Karen who plays it safe, because she's seen what happens when you don't measure twice.
    #[arg(long)]
    dry_run: bool,
    #[command(flatten)]
    out: OutputArgs,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
enum SetupRuntime {
    /// Install for Claude Code (global: ~/.claude/, project: .claude/). The Karen who speaks Classic and has the receipts.
    #[clap(name = "claude")]
    Claude,
    /// Install for Codex (global: ~/.codex/, project: .codex/). The Karen who codes, because she's reviewed every PR and knows where the bodies are buried.
    #[clap(name = "codex")]
    Codex,
    /// Install for `OpenCode` (global: ~/.config/opencode/, project: .opencode/). The Karen who opens, because closed doors don't stop her from finding the mess.
    #[clap(name = "opencode")]
    OpenCode,
}

#[derive(Args, Clone)]
struct OutputArgs {
    /// Machine-readable JSON on stdout (same as `--output json`). The Karen who speaks JSON natively — and yes, she'll correct yours.
    #[arg(long)]
    json: bool,
    #[arg(long, value_enum)]
    output: Option<OutputFormat>,
    /// Keep only these top-level fields in JSON output. The Karen who filters — she knows exactly what she wants, and *will* ask for the manager if she doesn't get it.
    #[arg(long, value_delimiter = ',')]
    fields: Vec<String>,
    /// Suppress human-readable success output. The Karen who can be silenced, and will be bringing it up later.
    #[arg(long)]
    quiet: bool,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

impl OutputArgs {
    fn wants_json(&self) -> bool {
        self.json || self.output == Some(OutputFormat::Json)
    }
}

#[derive(ValueEnum, Clone, Copy)]
enum Adapter {
    /// Claude Code runtime with modifiedInput support (default). The Karen who took the upgrade, and brings it up unprompted.
    #[clap(name = "claude")]
    Claude,
    /// Claude Code runtime without modifiedInput support. The Karen who tells you what to fix instead of fixing it herself — it's a boundaries thing.
    #[clap(name = "claude-classic")]
    ClaudeClassic,
}

fn main() {
    init_logging();
    let cli = Cli::parse();

    let code = match cli.command {
        Command::Hook {
            adapter,
            policies,
            part,
        } => run_hook(adapter, policies, part),
        Command::Validate(args) => run_validate(args, false),
        Command::Check(args) => run_validate(args, true),
        Command::Evaluate(args) => run_evaluate(&args),
        Command::Schema(args) => run_schema(&args),
        Command::Completion { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "karen", &mut std::io::stdout());
            EXIT_OK
        }
        Command::Setup(args) => run_setup(&args),
    };

    std::process::exit(code);
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .without_time()
        .try_init();
}

fn policy_root(override_path: Option<PathBuf>) -> PathBuf {
    override_path.unwrap_or_else(policy::locate_policies)
}

fn normalize(adapter: Adapter, raw: Value) -> Result<EngineRequest, AdapterError> {
    match adapter {
        Adapter::Claude => karen::adapters::Claude.normalize(raw),
        Adapter::ClaudeClassic => karen::adapters::ClaudeClassicAdapter.normalize(raw),
    }
}

fn adapter_name(adapter: Adapter) -> &'static str {
    match adapter {
        Adapter::Claude => "claude",
        Adapter::ClaudeClassic => "claude-classic",
    }
}

fn run_hook(adapter: Adapter, policies: Option<PathBuf>, part: Option<usize>) -> i32 {
    let dev = std::env::var_os("KAREN_DEV").is_some();

    let mut input = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut input) {
        fail_open(&format!("cannot read stdin: {error}"), dev);
    }

    let raw: Value = match serde_json::from_str(&input) {
        Ok(value) => value,
        Err(error) => fail_open(&format!("invalid JSON input: {error}"), dev),
    };

    let mut request = match normalize(adapter, raw) {
        Ok(request) => request,
        Err(error) => fail_open(&format!("cannot normalize request: {error}"), dev),
    };
    request.part = part;

    let root = policy_root(policies);
    let set = match policy::load(&root) {
        Ok(set) => set,
        Err(error) => {
            tracing::error!(%error, root = %root.display(), "policy load failed");
            println!("{{}}");
            return EXIT_OK;
        }
    };

    if matches!(request.event, Event::SessionStart) && part == Some(1) {
        let config = &set.config.maintenance;
        maintenance::prune_once(
            &request.session_id,
            &config.stale_temp_prefixes,
            Duration::from_secs(config.stale_temp_age_hours.saturating_mul(3_600)),
        );
    }

    let state = state_for(&set);
    let engine = Engine::new(set, state);
    let response = engine.dispatch(&request);

    let output = match adapter {
        Adapter::Claude => karen::adapters::Claude.denormalize(&request, response),
        Adapter::ClaudeClassic => {
            karen::adapters::ClaudeClassicAdapter.denormalize(&request, response)
        }
    };
    match serde_json::to_string(&output) {
        Ok(text) => println!("{text}"),
        Err(error) => fail_open(&format!("cannot serialize response: {error}"), dev),
    }
    EXIT_OK
}

fn state_for(set: &PolicySet) -> Box<dyn StateStore> {
    if set.config.state.backend == "memory" {
        Box::new(MemoryStore::new())
    } else {
        Box::new(FileStore::new(expand_home(&set.config.state.file_dir)))
    }
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn fail_open(message: &str, dev: bool) -> ! {
    tracing::error!("{message}");
    if dev {
        std::process::exit(1);
    }
    println!("{{}}");
    std::process::exit(EXIT_OK);
}

fn run_validate(args: ValidateArgs, verbose: bool) -> i32 {
    let root = policy_root(args.policies);
    let set = match load_policies(&root, &args.out) {
        Ok(set) => set,
        Err(code) => return code,
    };

    let data = policy_json(&set);
    if args.out.wants_json() {
        return print_json(data, &args.out);
    }
    if args.out.quiet {
        return EXIT_OK;
    }

    println!("policies: {}", set.root.display());
    println!("gates: {}  rules: {}", set.gates.len(), set.rules.len());
    if verbose {
        for gate in &set.gates {
            println!("  - {gate}");
        }
        for rule in &set.rules {
            println!("  - rule {}", rule.path.display());
        }
    }
    println!();
    println!("Next steps:");
    println!("  Inspect as JSON:  karen check --json");
    println!(
        "  Try a call:       karen evaluate --event PreToolUse --tool Bash --command \"git commit -m x\""
    );
    EXIT_OK
}

fn run_evaluate(args: &EvaluateArgs) -> i32 {
    if let Err((code, message)) = validate_evaluate_input(args) {
        return fail(
            &args.out,
            "INVALID_INPUT",
            &message,
            "Run `karen schema evaluate` for accepted flags.",
            code,
        );
    }

    let raw = match build_raw(args, &args.out) {
        Ok(raw) => raw,
        Err(code) => return code,
    };

    let request = match normalize(args.adapter, raw) {
        Ok(request) => request,
        Err(error) => {
            return fail(
                &args.out,
                "INVALID_REQUEST",
                &format!("cannot normalize request: {error}"),
                "Run `karen schema evaluate` for the accepted event names.",
                EXIT_INPUT,
            );
        }
    };

    let root = policy_root(args.policies.clone());
    let set = match load_policies(&root, &args.out) {
        Ok(set) => set,
        Err(code) => return code,
    };
    let policy_root = set.root.display().to_string();
    // Dry run means dry: she reads real strike state so the preview is honest,
    // but writes nothing, so rehearsing can never eat your only warning.
    // The Karen who lets you practice your apology without filing it.
    let state = Box::new(ReadOnlyStore::new(state_for(&set)));

    let engine = Engine::new(set, state);
    let response = engine.dispatch(&request);

    let data = evaluation_json(&response, args.adapter, &policy_root, args.include_content);
    if args.out.wants_json() {
        return print_json(data, &args.out);
    }
    if args.out.quiet {
        return EXIT_OK;
    }

    println!("action: {}", response.action);
    if let Some(reason) = action_reason(&response.action) {
        println!("reason: {reason}");
    }
    if response.fired_policies.is_empty() {
        println!("fired: (none)");
    } else {
        println!("fired: {}", response.fired_policies.join(", "));
    }
    println!("injections: {}", response.injections.len());
    println!();
    println!("Dry run only; session state was not modified.");
    println!("Next: karen check --json --fields gates");
    EXIT_OK
}

fn validate_evaluate_input(args: &EvaluateArgs) -> Result<(), (i32, String)> {
    let flag_mode = args.event.is_some()
        || args.tool.is_some()
        || args.command.is_some()
        || args.file_path.is_some()
        || args.prompt.is_some()
        || args.cwd.is_some();

    if args.request.is_some() && flag_mode {
        return Err((
            EXIT_INPUT,
            "use either --request or the individual request flags, not both".into(),
        ));
    }
    if args.request.is_none() && args.event.is_none() {
        return Err((EXIT_INPUT, "provide --request or at least --event".into()));
    }

    let text_fields = [
        ("--event", args.event.as_deref()),
        ("--tool", args.tool.as_deref()),
        ("--command", args.command.as_deref()),
        ("--file-path", args.file_path.as_deref()),
        ("--prompt", args.prompt.as_deref()),
        ("--session", Some(args.session.as_str())),
        ("--cwd", args.cwd.as_deref()),
    ];
    for (name, value) in text_fields {
        if let Some(value) = value
            && has_forbidden_controls(value)
        {
            return Err((EXIT_INPUT, format!("{name} contains control characters")));
        }
    }
    Ok(())
}

/// Reject control characters except tab, newline, and carriage return. The Karen who checks every character's paperwork; three have visas, the rest are being escorted out.
fn has_forbidden_controls(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

fn build_raw(args: &EvaluateArgs, out: &OutputArgs) -> Result<Value, i32> {
    if let Some(request) = &args.request {
        let text = if request == "-" {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| {
                    fail(
                        out,
                        "INPUT_READ_FAILED",
                        &format!("cannot read stdin: {error}"),
                        "Pipe a JSON payload or pass --request <file>.",
                        EXIT_INPUT,
                    )
                })?;
            buffer
        } else {
            match std::fs::read_to_string(request) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(fail(
                        out,
                        "REQUEST_NOT_FOUND",
                        &format!("request file not found: {request}"),
                        "Pass an existing file or use --request - to read stdin.",
                        EXIT_NOT_FOUND,
                    ));
                }
                Err(error) => {
                    return Err(fail(
                        out,
                        "INPUT_READ_FAILED",
                        &format!("cannot read {request}: {error}"),
                        "Check the file path and permissions.",
                        EXIT_INPUT,
                    ));
                }
            }
        };

        return serde_json::from_str(&text).map_err(|error| {
            fail(
                out,
                "INVALID_JSON",
                &format!("request is not valid JSON: {error}"),
                "Pass the raw runtime hook payload as JSON.",
                EXIT_INPUT,
            )
        });
    }

    let mut tool_input = serde_json::Map::new();
    if let Some(command) = &args.command {
        tool_input.insert("command".into(), Value::String(command.clone()));
    }
    if let Some(file_path) = &args.file_path {
        tool_input.insert("file_path".into(), Value::String(file_path.clone()));
    }

    Ok(json!({
        "hook_event_name": args.event,
        "session_id": args.session,
        "cwd": args.cwd,
        "tool_name": args.tool,
        "tool_input": Value::Object(tool_input),
        "prompt": args.prompt,
    }))
}

fn run_schema(args: &SchemaArgs) -> i32 {
    let data = schema_json(args.topic.as_deref());
    let data = match data {
        Ok(data) => data,
        Err(message) => {
            return fail(
                &args.out,
                "UNKNOWN_SCHEMA_TOPIC",
                &message,
                "Run `karen schema` to list every command.",
                EXIT_INPUT,
            );
        }
    };

    if args.out.wants_json() {
        return print_json(data, &args.out);
    }
    match serde_json::to_string_pretty(&data) {
        Ok(text) => {
            println!("{text}");
            EXIT_OK
        }
        Err(error) => fail(
            &args.out,
            "SERIALIZE_FAILED",
            &format!("cannot serialize schema: {error}"),
            "Report this as a bug.",
            EXIT_INPUT,
        ),
    }
}

fn load_policies(root: &std::path::Path, out: &OutputArgs) -> Result<PolicySet, i32> {
    policy::load(root).map_err(|error| {
        let (code, fix) = match &error {
            LoadError::Root { .. } => (
                "POLICY_NOT_FOUND",
                "Point --policies at a directory containing config.toml, rules/, and gates/.",
            ),
            _ => (
                "POLICY_INVALID",
                "Run `karen validate` after fixing the reported file.",
            ),
        };
        fail(out, code, &error.to_string(), fix, EXIT_CONFIG)
    })
}

fn policy_json(set: &PolicySet) -> Value {
    json!({
        "policy_root": set.root.display().to_string(),
        "gate_count": set.gates.len(),
        "rule_count": set.rules.len(),
        "gates": set
            .gates
            .iter()
            .map(|gate| {
                json!({
                    "id": gate.id,
                    "event": gate.event,
                    "tool": gate.tool,
                    "priority": gate.priority,
                    "action": action_name(&gate.action),
                    "enabled": gate.enabled,
                    "tags": gate.tags,
                })
            })
            .collect::<Vec<_>>(),
        "rules": set
            .rules
            .iter()
            .map(|rule| {
                json!({
                    "id": rule.id,
                    "path": rule.path.display().to_string(),
                    "chars": rule.content.chars().count(),
                })
            })
            .collect::<Vec<_>>(),
        "config": {
            "fail_open": set.config.engine.fail_open,
            "state_backend": set.config.state.backend,
            "state_dir": set.config.state.file_dir,
        },
    })
}

fn evaluation_json(
    response: &EngineResponse,
    adapter: Adapter,
    policy_root: &str,
    include_content: bool,
) -> Value {
    let injections = response
        .injections
        .iter()
        .map(|injection| {
            let mut entry = json!({
                "kind": injection_kind_name(injection.kind),
                "chars": injection.content.chars().count(),
                "source": injection.source,
            });
            if include_content {
                entry["content"] = Value::String(injection.content.clone());
            }
            entry
        })
        .collect::<Vec<_>>();

    let injection_chars: usize = response
        .injections
        .iter()
        .map(|injection| injection.content.chars().count())
        .sum();

    json!({
        "action": response.action.to_string(),
        "reason": action_reason(&response.action),
        "fired_policies": response.fired_policies,
        "injections": injections,
        "injection_chars": injection_chars,
        "modified_input": response.modified_input,
        "adapter": adapter_name(adapter),
        "policy_root": policy_root,
        "dry_run": true,
    })
}

fn action_reason(action: &Action) -> Option<&str> {
    match action {
        Action::Allow => None,
        Action::Deny { reason } | Action::Ask { reason } | Action::Modify { reason, .. } => {
            Some(reason)
        }
    }
}

fn action_name(action: &ActionDef) -> &'static str {
    match action {
        ActionDef::Allow => "allow",
        ActionDef::Deny { .. } => "deny",
        ActionDef::Ask { .. } => "ask",
        ActionDef::Modify { .. } => "modify",
        ActionDef::DenyOnceThenWarn { .. } => "deny_once_then_warn",
    }
}

fn injection_kind_name(kind: InjectionKind) -> &'static str {
    match kind {
        InjectionKind::Rule => "rule",
        InjectionKind::Skill => "skill",
        InjectionKind::Redirect => "redirect",
        InjectionKind::Notice => "notice",
    }
}

fn print_json(mut data: Value, out: &OutputArgs) -> i32 {
    if let Err(message) = filter_fields(&mut data, &out.fields) {
        return fail(
            out,
            "INVALID_FIELDS",
            &message,
            "Run the command without --fields to see available fields.",
            EXIT_INPUT,
        );
    }

    let envelope = json!({
        "status": "ok",
        "data": data,
        "warnings": Vec::<String>::new(),
    });
    match serde_json::to_string(&envelope) {
        Ok(text) => {
            println!("{text}");
            EXIT_OK
        }
        Err(error) => fail(
            out,
            "SERIALIZE_FAILED",
            &format!("cannot serialize output: {error}"),
            "Report this as a bug.",
            EXIT_INPUT,
        ),
    }
}

fn filter_fields(data: &mut Value, fields: &[String]) -> Result<(), String> {
    if fields.is_empty() {
        return Ok(());
    }
    let Some(object) = data.as_object_mut() else {
        return Ok(());
    };
    let unknown = fields
        .iter()
        .filter(|field| !object.contains_key(field.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!("unknown field(s): {}", unknown.join(", ")));
    }
    object.retain(|key, _| fields.iter().any(|field| field == key));
    Ok(())
}

/// Emit a structured error on stdout when JSON was requested, a hint on
/// stderr always, and return the exit code. The Karen who errors gracefully.
fn fail(out: &OutputArgs, code: &str, message: &str, fix: &str, exit: i32) -> i32 {
    if out.wants_json() {
        let envelope = json!({
            "status": "error",
            "error": {
                "code": code,
                "message": message,
                "fix": fix,
                "transient": false,
            }
        });
        if let Ok(text) = serde_json::to_string(&envelope) {
            println!("{text}");
        }
    }
    eprintln!("Error: {message}");
    if !fix.is_empty() {
        eprintln!("Fix: {fix}");
    }
    exit
}

fn schema_json(topic: Option<&str>) -> Result<Value, String> {
    let commands = vec![
        json!({
            "name": "hook",
            "summary": "Run as an external hook. Reads a runtime payload on stdin and writes the runtime protocol object on stdout.",
            "agent_protocol": true,
            "params": {
                "adapter": {"type": "string", "enum": ["claude", "claude-classic"], "default": "claude"},
                "policies": {"type": "path", "required": false},
                "part": {"type": "integer", "minimum": 1, "required": false, "description": "Deliver one rule-injection part."},
            },
            "stdout": "runtime protocol object; `{}` when allowing or failing open",
        }),
        json!({
            "name": "validate",
            "summary": "Validate policy files and print a summary.",
            "params": output_params(json!({"policies": {"type": "path", "required": false}})),
        }),
        json!({
            "name": "check",
            "summary": "Validate policies and list every gate and rule.",
            "params": output_params(json!({"policies": {"type": "path", "required": false}})),
        }),
        json!({
            "name": "evaluate",
            "summary": "Evaluate one request as a dry run. Never writes runtime state.",
            "params": output_params(json!({
                "adapter": {"type": "string", "enum": ["claude", "claude-classic"], "default": "claude"},
                "policies": {"type": "path", "required": false},
                "request": {"type": "string", "description": "Raw runtime payload file, or `-` for stdin."},
                "event": {"type": "string", "enum": event_names()},
                "tool": {"type": "string"},
                "command": {"type": "string"},
                "file_path": {"type": "string"},
                "prompt": {"type": "string"},
                "session": {"type": "string", "default": "evaluate"},
                "cwd": {"type": "string"},
                "include_content": {"type": "boolean", "default": false},
            })),
        }),
        json!({
            "name": "schema",
            "summary": "Describe commands and policy vocabulary.",
            "params": output_params(json!({"topic": {"type": "string", "required": false}})),
        }),
        json!({
            "name": "setup",
            "summary": "Install karen hooks for AI runtimes. Global by default; use --project for project-level. Supports --dry-run to preview.",
            "params": output_params(json!({
                "runtime": {"type": "string", "enum": ["claude", "codex", "opencode"], "required": false},
                "project": {"type": "boolean", "default": false},
                "dry_run": {"type": "boolean", "default": false},
            })),
        }),
        json!({
            "name": "completion",
            "summary": "Generate shell completions for bash, zsh, or fish.",
            "params": {
                "shell": {"type": "string", "enum": ["bash", "zsh", "fish"], "required": true},
            },
        }),
    ];

    let commands = if let Some(topic) = topic {
        commands
            .into_iter()
            .filter(|command| command.get("name").and_then(Value::as_str) == Some(topic))
            .collect::<Vec<_>>()
    } else {
        commands
    };

    if commands.is_empty() {
        return Err(format!("unknown schema topic: {topic:?}"));
    }

    Ok(json!({
        "name": "karen",
        "version": env!("CARGO_PKG_VERSION"),
        "commands": commands,
        "types": {
            "events": event_names(),
            "adapters": ["claude", "claude-classic"],
            "conditions": [
                "command_regex", "command_contains", "file_regex", "path_regex",
                "prompt_regex", "content_regex", "tool_regex", "field_equals", "field_regex", "always"
            ],
            "actions": ["allow", "deny", "ask", "modify", "deny_once_then_warn"],
            "exit_codes": {"0": "success", "2": "usage or invalid input", "3": "not found", "78": "invalid policies"},
        },
        "invariants": [
            "stdout carries data only; diagnostics and logs go to stderr",
            "hook output is the runtime protocol object, not the CLI JSON envelope",
            "evaluate is read-only and always exits 0 on a successful evaluation",
            "policies are loaded from --policies, KAREN_POLICIES, or CLAUDE_PLUGIN_ROOT/policies",
        ],
    }))
}

fn output_params(mut params: Value) -> Value {
    if let Some(object) = params.as_object_mut() {
        object.insert("json".into(), json!({"type": "boolean", "default": false}));
        object.insert(
            "output".into(),
            json!({"type": "string", "enum": ["text", "json"]}),
        );
        object.insert(
            "fields".into(),
            json!({"type": "array", "items": {"type": "string"}, "description": "Top-level fields to keep."}),
        );
        object.insert("quiet".into(), json!({"type": "boolean", "default": false}));
    }
    params
}

fn event_names() -> Value {
    json!([
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PreCompact",
        "PostCompact",
        "SubagentStart",
        "SubagentStop",
        "Stop",
        "Notification",
    ])
}

#[allow(dead_code)]
fn adapter_type_name(adapter: AdapterType) -> &'static str {
    match adapter {
        AdapterType::Claude => "claude",
        AdapterType::ClaudeClassic => "claude-classic",
        AdapterType::Codex => "codex",
        AdapterType::OpenCodeV1 => "opencode-v1",
        AdapterType::OpenCodeV2 => "opencode-v2",
    }
}

fn run_setup(args: &SetupArgs) -> i32 {
    let runtimes = match args.runtime {
        Some(r) => vec![r],
        None => vec![
            SetupRuntime::Claude,
            SetupRuntime::Codex,
            SetupRuntime::OpenCode,
        ],
    };

    let actions = runtimes
        .iter()
        .map(|runtime| plan_setup_action(*runtime, args.project))
        .collect::<Vec<_>>();

    if args.dry_run {
        return preview_setup(&actions, &args.out);
    }

    for action in &actions {
        if let Err((code, message)) = install_setup_action(action) {
            return fail(
                &args.out,
                "SETUP_FAILED",
                &message,
                "Check permissions or run with --dry-run first.",
                code,
            );
        }
    }

    report_setup(&actions, &args.out)
}

/// Where this runtime's hooks belong and what goes in them. The Karen who
/// plans every install like a seating chart: right runtime, right
/// directory, no surprises at the table.
fn plan_setup_action(runtime: SetupRuntime, project: bool) -> SetupAction {
    let (global_dir, project_dir, hook_file_name, hook_content) = match runtime {
        SetupRuntime::Claude => {
            let global =
                dirs::home_dir().map_or_else(|| PathBuf::from(".claude"), |h| h.join(".claude"));
            (
                global,
                PathBuf::from(".claude"),
                "settings.json",
                claude_hooks_json(),
            )
        }
        SetupRuntime::Codex => {
            let global =
                dirs::home_dir().map_or_else(|| PathBuf::from(".codex"), |h| h.join(".codex"));
            (
                global,
                PathBuf::from(".codex"),
                "hooks.json",
                codex_hooks_json(),
            )
        }
        SetupRuntime::OpenCode => {
            let global = dirs::home_dir().map_or_else(
                || PathBuf::from(".config/opencode"),
                |h| h.join(".config").join("opencode"),
            );
            (
                global,
                PathBuf::from(".opencode"),
                "plugins/karen-ai.js",
                OPENCODE_PLUGIN_JS.to_string(),
            )
        }
    };

    let target_dir = if project { project_dir } else { global_dir };
    // Claude Code and OpenCode never read a hooks.json, so one written by an
    // earlier `karen setup` is hers to clear. Codex keeps its hooks.json.
    let legacy = (!matches!(runtime, SetupRuntime::Codex)).then(|| target_dir.join("hooks.json"));
    SetupAction {
        runtime: format!("{runtime:?}").to_lowercase(),
        scope: if project { "project" } else { "global" }.into(),
        path: target_dir.join(hook_file_name),
        merge: matches!(runtime, SetupRuntime::Claude),
        legacy,
        content: hook_content,
    }
}

/// Preview the install plan without touching disk. The Karen who shows you
/// the receipt before she spends your money.
fn preview_setup(actions: &[SetupAction], out: &OutputArgs) -> i32 {
    if out.wants_json() {
        let data = json!({
            "actions": actions.iter().map(|a| json!({
                "runtime": a.runtime,
                "scope": a.scope,
                "path": a.path.display().to_string(),
                "would_write": true,
                "mode": if a.merge { "merge" } else { "replace" },
            })).collect::<Vec<_>>(),
        });
        return print_json(data, out);
    }
    if !out.quiet {
        println!("Would install hooks (dry run):");
        for action in actions {
            println!(
                "  [{}] {} -> {}",
                action.runtime,
                action.scope,
                action.path.display()
            );
        }
        println!("\nRun without --dry-run to apply.");
    }
    EXIT_OK
}

/// Write one hooks.json atomically: temp file first, then rename, so a
/// crash can never leave a half-written config behind. The Karen who
/// commits or rolls back — no half-written files on her watch, and no
/// phone calls to the manager about files that only half exist.
fn install_setup_action(action: &SetupAction) -> Result<(), (i32, String)> {
    if let Some(parent) = action.path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err((
                EXIT_CONFIG,
                format!("cannot create directory {}: {e}", parent.display()),
            ));
        }
    }
    let content = if action.merge {
        merged_claude_settings(&action.path, &action.content)?
    } else {
        action.content.clone()
    };
    let temp_path = action.path.with_extension("json.tmp");
    if let Err(e) = fs::write(&temp_path, &content) {
        return Err((
            EXIT_CONFIG,
            format!("cannot write temp file {}: {e}", temp_path.display()),
        ));
    }
    if let Err(e) = fs::rename(&temp_path, &action.path) {
        let _ = fs::remove_file(&temp_path);
        return Err((
            EXIT_CONFIG,
            format!("cannot atomically install {}: {e}", action.path.display()),
        ));
    }
    if let Some(legacy) = action
        .legacy
        .as_deref()
        .filter(|p| is_karen_legacy_hooks(p))
    {
        if let Err(e) = fs::remove_file(legacy) {
            return Err((
                EXIT_CONFIG,
                format!("cannot remove stale {}: {e}", legacy.display()),
            ));
        }
    }
    Ok(())
}

/// A `hooks.json` is hers only if every command in it is a `karen hook`.
/// Anyone else's file stays put, however dead — she clears her own desk,
/// not the neighbours'.
fn is_karen_legacy_hooks(path: &Path) -> bool {
    let Some(doc) = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return false;
    };
    let entries: Vec<&Value> = doc
        .get("hooks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|events| events.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .collect();
    !entries.is_empty() && entries.iter().all(|entry| is_karen_hook(entry))
}

/// Report what was installed — quietly if asked, loudly by default. The
/// Karen who hands you the summary whether you asked for it or not.
fn report_setup(actions: &[SetupAction], out: &OutputArgs) -> i32 {
    if out.wants_json() {
        let data = json!({
            "installed": actions.iter().map(|a| json!({
                "runtime": a.runtime,
                "scope": a.scope,
                "path": a.path.display().to_string(),
            })).collect::<Vec<_>>(),
        });
        return print_json(data, out);
    }
    if !out.quiet {
        println!("Installed Karen-AI hooks:");
        for action in actions {
            println!(
                "  [{}] {} -> {}",
                action.runtime,
                action.scope,
                action.path.display()
            );
        }
        println!("\nNext steps:");
        println!("  Verify: karen validate --json");
        println!(
            "  Test:   karen evaluate --event PreToolUse --tool Bash --command 'git commit -m x'"
        );
        println!(
            "\nNote: Hooks use the karen binary from PATH. Install karen first (cargo install karen-ai)."
        );
    }
    EXIT_OK
}

struct SetupAction {
    runtime: String,
    scope: String,
    path: PathBuf,
    merge: bool,
    legacy: Option<PathBuf>,
    content: String,
}

/// Claude Code reads hooks only from its settings files (or a plugin's
/// hooks/hooks.json), so she moves in rather than leaving a note on the
/// porch. Everything that isn't hers stays exactly where it was; her own
/// previous entries — stale ones included — are cleared out first, because
/// a duplicate Karen is the one thing Karen cannot abide. Unreadable JSON is
/// refused, never overwritten: she does not "fix" other people's files.
fn merged_claude_settings(path: &Path, karen_registration: &str) -> Result<String, (i32, String)> {
    let mut settings = match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Value>(&text).map_err(|e| {
            (
                EXIT_CONFIG,
                format!(
                    "{} is not valid JSON ({e}); refusing to overwrite it",
                    path.display()
                ),
            )
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err((EXIT_CONFIG, format!("cannot read {}: {e}", path.display()))),
    };
    let not_object = |what: &str| {
        (
            EXIT_CONFIG,
            format!(
                "{what} in {} is not a JSON object; refusing to overwrite it",
                path.display()
            ),
        )
    };
    let karen_hooks = serde_json::from_str::<Value>(karen_registration)
        .ok()
        .and_then(|v| v.get("hooks").cloned())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let root = settings
        .as_object_mut()
        .ok_or_else(|| not_object("the top level"))?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| not_object("`hooks`"))?;

    let mut emptied = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let before = groups.len();
        for group in groups.iter_mut() {
            if let Some(entries) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                entries.retain(|entry| !is_karen_hook(entry));
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|entries| !entries.is_empty())
        });
        if groups.is_empty() && before > 0 {
            emptied.push(event.clone());
        }
    }
    for event in emptied {
        hooks.remove(&event);
    }

    for (event, groups) in karen_hooks {
        let slot = hooks.entry(event).or_insert_with(|| json!([]));
        if let (Some(slot), Some(groups)) = (slot.as_array_mut(), groups.as_array()) {
            slot.extend(groups.iter().cloned());
        }
    }

    serde_json::to_string_pretty(&settings)
        .map(|s| s + "\n")
        .map_err(|e| (EXIT_CONFIG, format!("cannot serialize settings: {e}")))
}

/// Her signature: a command that runs `karen hook` (or `karen.exe hook`),
/// by bare name or by path.
fn is_karen_hook(entry: &Value) -> bool {
    static SIGNATURE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?:^|[\s"'/\\])karen(?:\.exe)?["']?\s+hook\b"#)
            .expect("the signature regex is a literal and compiles")
    });
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| SIGNATURE.is_match(command))
}

/// `OpenCode` runs no command hooks — only JS/TS plugins (plugin API v1:
/// `tool.execute.before` throws to block, `tool.execute.after` amends the
/// output, `experimental.chat.system.transform` carries the rules). So she
/// ships a bridge that speaks Claude Classic to the engine and plugin to the
/// runtime. The Karen who learned the local customs and kept her own rules.
const OPENCODE_PLUGIN_JS: &str = r#"// Karen-AI bridge for OpenCode (plugin API v1). Installed by `karen setup opencode`.
// OpenCode has no command hooks, so this plugin forwards events to `karen hook`
// in the Claude Classic dialect. KAREN_BIN overrides the binary (default: karen on PATH).
import { spawnSync } from "node:child_process";

const KAREN = process.env.KAREN_BIN || "karen";
const TOOL_NAMES = { list: "LS", webfetch: "WebFetch", todowrite: "TodoWrite", todoread: "TodoRead", multiedit: "MultiEdit" };
const toolName = (tool) => TOOL_NAMES[tool] ?? (tool ? tool[0].toUpperCase() + tool.slice(1) : tool);

function karen(payload, part) {
  const args = ["hook", "--adapter", "claude-classic"];
  if (part) args.push("--part", String(part));
  const run = spawnSync(KAREN, args, { input: JSON.stringify(payload), encoding: "utf8", timeout: 10000, windowsHide: true });
  if (run.error || run.status !== 0) return {};
  try { return JSON.parse(run.stdout || "{}"); } catch { return {}; }
}

export const KarenAI = async ({ directory }) => {
  const rules = new Map();
  const notices = new Map();
  return {
    "experimental.chat.system.transform": async (input, output) => {
      const sid = input.sessionID ?? "unknown";
      if (!rules.has(sid)) {
        const parts = [];
        for (let part = 1; part <= 64; part++) {
          const out = karen({ hook_event_name: "SessionStart", session_id: `opencode-rules:${process.pid}:${sid}`, cwd: directory }, part);
          const context = out?.hookSpecificOutput?.additionalContext;
          if (!context) break;
          parts.push(context);
        }
        rules.set(sid, parts.join("\n\n"));
      }
      if (rules.get(sid)) output.system.push(rules.get(sid));
    },
    "tool.execute.before": async (input, output) => {
      const out = karen({ hook_event_name: "PreToolUse", session_id: input.sessionID, cwd: directory, tool_name: toolName(input.tool), tool_input: output.args });
      if (out.modifiedInput && typeof out.modifiedInput === "object") {
        Object.assign(output.args, out.modifiedInput);
      } else {
        const decision = out.hookSpecificOutput?.permissionDecision;
        const reason = out.hookSpecificOutput?.permissionDecisionReason ?? "blocked by policy";
        if (decision === "deny") throw new Error(`Karen-AI: ${reason}`);
        if (decision === "ask") throw new Error(`Karen-AI: needs human approval, and OpenCode plugins cannot ask: ${reason}`);
      }
      if (out.systemMessage) notices.set(input.callID, out.systemMessage);
    },
    "tool.execute.after": async (input, output) => {
      const out = karen({ hook_event_name: "PostToolUse", session_id: input.sessionID, cwd: directory, tool_name: toolName(input.tool), tool_input: input.args, tool_response: output.output });
      const notes = [notices.get(input.callID), out.decision === "block" ? out.reason : null, out.systemMessage].filter(Boolean);
      notices.delete(input.callID);
      if (notes.length) output.output = `${output.output ?? ""}\n\n[Karen-AI] ${notes.join("\n")}`;
    },
  };
};
"#;

fn claude_hooks_json() -> String {
    let karen = if cfg!(windows) { "karen.exe" } else { "karen" };
    format!(
        "{{
  \"_comment\": \"Claude Code hook registration for Karen-AI. Rule injection (karen hook --part N) and gate enforcement (karen hook) both use the Rust binary, which evaluates JSON/JSONC/JSON5 gates. This file is installed by `karen setup claude` — no manual editing of settings.json required.\",
  \"hooks\": {{
    \"SessionStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 1\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 2\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 3\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 4\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 5\", \"timeout\": 10}}] }}
    ],
    \"UserPromptSubmit\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"PreToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"PostToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"Stop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"SessionEnd\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"PreCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"PostCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"SubagentStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"SubagentStop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ]
  }}
}}")
}

fn codex_hooks_json() -> String {
    let karen = if cfg!(windows) { "karen.exe" } else { "karen" };
    format!(
        "{{
  \"_comment\": \"Codex hook registration for Karen-AI. Rule injection (karen hook --part N) and gate enforcement (karen hook) both use the Rust binary, which evaluates JSON/JSONC/JSON5 gates. This file is installed by `karen setup codex` — no manual editing of Codex config required. Codex auto-discovers hooks from .codex-plugin/plugin.json.\",
  \"hooks\": {{
    \"SessionStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 1\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 2\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 3\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 4\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --part 5\", \"timeout\": 10}}] }}
    ],
    \"UserPromptSubmit\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"PreToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"PostToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"Stop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"PreCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"PostCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ],
    \"SubagentStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 10}}] }}
    ],
    \"SubagentStop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic\", \"timeout\": 5}}] }}
    ]
  }}
}}")
}
