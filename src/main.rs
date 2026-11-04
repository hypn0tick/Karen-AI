use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::generate;
use karen::adapters::{AdapterError, RuntimeAdapter};
use karen::engine::{Engine, maintenance};
use karen::policy::{self, ActionDef, IssueKind, PolicySet};
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
        /// Read the hook payload from this file instead of stdin. For a caller whose runtime cannot hand a child a reliable stdin: the `OpenCode` bridge writes the payload to a file and gives her no stdin at all.
        #[arg(long)]
        input: Option<PathBuf>,
        /// The runtime that runs this hook (`claude`, `codex`, `opencode`), as the `runtime` fact. `karen setup` writes it into every hook command; a `facts.runtime` in the payload still wins. Without either she reads it from the transcript path.
        #[arg(long)]
        runtime: Option<String>,
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
    /// The runtime to rehearse as (`claude`, `codex`, `opencode`), as `karen hook --runtime` does.
    #[arg(long)]
    runtime: Option<String>,
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
    /// Install for Claude Code (global: `CLAUDE_CONFIG_DIR`, else `~/.claude/`; project: .claude/). The Karen who speaks Classic and has the receipts.
    #[clap(name = "claude")]
    Claude,
    /// Install for Codex (global: `CODEX_HOME`, else `~/.codex/`; project: .codex/). The Karen who codes, because she's reviewed every PR and knows where the bodies are buried.
    #[clap(name = "codex")]
    Codex,
    /// Install for `OpenCode` (global: `OPENCODE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/opencode/`, else `~/.config/opencode/`; project: .opencode/). The Karen who opens, because closed doors don't stop her from finding the mess.
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
            input,
            runtime,
        } => run_hook(
            adapter,
            policies,
            part,
            input.as_deref(),
            runtime.as_deref(),
        ),
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

fn policy_roots(explicit: Option<PathBuf>, cwd: Option<&std::path::Path>) -> policy::Roots {
    policy::Roots::discover(explicit, cwd)
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

fn run_hook(
    adapter: Adapter,
    policies: Option<PathBuf>,
    part: Option<usize>,
    input: Option<&std::path::Path>,
    runtime: Option<&str>,
) -> i32 {
    let dev = std::env::var_os("KAREN_DEV").is_some();

    // The payload is one JSON value; she stops reading when it is complete,
    // never waiting for end of input. A runtime that is slow to close the
    // pipe after a small write costs her nothing.
    let first_value = |reader: &mut dyn std::io::Read| {
        serde_json::Deserializer::from_reader(reader)
            .into_iter::<Value>()
            .next()
    };
    let parsed = match input {
        Some(path) => match std::fs::File::open(path) {
            Ok(file) => first_value(&mut std::io::BufReader::new(file)),
            Err(error) => fail_open(&format!("cannot read {}: {error}", path.display()), dev),
        },
        None => first_value(&mut std::io::BufReader::new(std::io::stdin().lock())),
    };
    let mut raw: Value = match parsed {
        Some(Ok(value)) => value,
        Some(Err(error)) => fail_open(&format!("invalid JSON input: {error}"), dev),
        None => fail_open("invalid JSON input: empty input", dev),
    };
    name_runtime(&mut raw, runtime);

    let mut request = match normalize(adapter, raw) {
        Ok(request) => request,
        Err(error) => fail_open(&format!("cannot normalize request: {error}"), dev),
    };
    request.part = part;

    let roots = policy_roots(policies, request.cwd.as_deref().map(std::path::Path::new));
    let set = match policy::load_roots(&roots) {
        Ok(set) => set,
        Err(error) => {
            // Only the built-in layer fails a whole load; a broken user or
            // project file is reported in the merge notice instead. Still
            // fail open, so a broken build never bricks the agent, but out
            // loud: a guard nobody knows is off duty is worse than none.
            tracing::error!(%error, root = %roots.primary().display(), "policy load failed");
            let message = format!(
                "Karen-AI is not enforcing any policy: {}. Set KAREN_POLICIES or pass --policies.",
                one_line(&error.to_string())
            );
            println!("{}", serde_json::json!({ "systemMessage": message }));
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
    let merge_notice = merge_notice(&set, state.as_ref(), &request.session_id);
    karen::engine::facts::enrich(&set, state.as_ref(), &mut request);
    let engine = Engine::new(set, state);
    let response = engine.dispatch(&request);

    let mut output = match adapter {
        Adapter::Claude => karen::adapters::Claude.denormalize(&request, response),
        Adapter::ClaudeClassic => {
            karen::adapters::ClaudeClassicAdapter.denormalize(&request, response)
        }
    };
    if let Some(notice) = merge_notice {
        add_system_message(&mut output, &notice);
    }
    match serde_json::to_string(&output) {
        Ok(text) => println!("{text}"),
        Err(error) => fail_open(&format!("cannot serialize response: {error}"), dev),
    }
    EXIT_OK
}

/// What a layer could not load, or the merge refused or could not apply, told
/// once per session and again whenever it changes: every other gate still
/// loads, but nobody should find out late that a file is not enforced or that
/// a project tried to switch off a locked gate.
fn merge_notice(set: &PolicySet, state: &dyn StateStore, session: &str) -> Option<String> {
    if set.issues.is_empty() {
        return None;
    }
    let text = set
        .issues
        .iter()
        .map(|i| format!("the {} layer {}", i.layer, one_line(&i.message)))
        .collect::<Vec<_>>()
        .join("; ");
    let key = "karen.merge_issues";
    if state.get(session, key).as_ref().and_then(Value::as_str) == Some(text.as_str()) {
        return None;
    }
    state.set(session, key, Value::from(text.clone()));
    Some(format!(
        "Karen-AI policy merge: {text}. Run `karen validate` for details."
    ))
}

/// `text` on one line: every run of whitespace that spans a line break becomes
/// one space. A notice is one line of a `systemMessage`, so a bridge can tell a
/// status line from a gate's warning without parsing the text.
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `runtime` as the payload's `facts.runtime`, unless the payload already
/// names one: a bridge that says where it runs knows best.
fn name_runtime(raw: &mut Value, runtime: Option<&str>) {
    let (Some(runtime), Some(object)) = (runtime, raw.as_object_mut()) else {
        return;
    };
    let facts = object
        .entry("facts")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(facts) = facts.as_object_mut() {
        facts
            .entry("runtime")
            .or_insert_with(|| Value::from(runtime));
    }
}

fn add_system_message(output: &mut Value, message: &str) {
    let Some(object) = output.as_object_mut() else {
        return;
    };
    let joined = match object.get("systemMessage") {
        Some(Value::String(existing)) => format!(
            "{existing}
{message}"
        ),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .chain(std::iter::once(message))
            .collect::<Vec<_>>()
            .join(
                "
",
            ),
        _ => message.to_string(),
    };
    object.insert("systemMessage".into(), Value::from(joined));
}

fn state_for(set: &PolicySet) -> Box<dyn StateStore> {
    if set.config.state.backend == "memory" {
        Box::new(MemoryStore::new())
    } else {
        Box::new(FileStore::new(set.config.state.dir()))
    }
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
    let roots = policy_roots(args.policies, None);
    let set = match load_policies(&roots, &args.out) {
        Ok(set) => set,
        Err(code) => return code,
    };
    // An explicit validation never trusts the remembered one: every pattern compiles now.
    if let Err(error) = set.gates.iter().try_for_each(karen::policy::Gate::validate) {
        return fail(
            &args.out,
            "POLICY_INVALID",
            &error.to_string(),
            "Run `karen validate` after fixing the reported file.",
            EXIT_CONFIG,
        );
    }
    // At hook time a file that does not load, a missing named root and a
    // refused change are left out and the rest is enforced; here each is an
    // error to fix, and every one of them is named.
    let failing: Vec<&policy::MergeIssue> = set
        .issues
        .iter()
        .filter(|i| i.kind.fails_validation())
        .collect();
    let worst = [
        IssueKind::MissingRoot,
        IssueKind::LoadFailed,
        IssueKind::Refused,
    ]
    .into_iter()
    .find(|kind| failing.iter().any(|i| i.kind == *kind));
    if let Some(kind) = worst {
        let (code, fix) = match kind {
            IssueKind::MissingRoot => (
                "POLICY_NOT_FOUND",
                "Point --policies at a directory containing config.toml, rules/, and gates/.",
            ),
            IssueKind::Refused => (
                "POLICY_INVALID",
                "Remove the refused change, or add `\"override\": true` where a replacement is meant.",
            ),
            IssueKind::LoadFailed | IssueKind::Warning => (
                "POLICY_INVALID",
                "Run `karen validate` after fixing the reported file.",
            ),
        };
        let message = failing
            .iter()
            .map(|i| format!("the {} layer {}", i.layer, i.message))
            .collect::<Vec<_>>()
            .join("; ");
        return fail(&args.out, code, &message, fix, EXIT_CONFIG);
    }

    let data = policy_json(&set);
    if args.out.wants_json() {
        return print_json(data, &args.out);
    }
    if args.out.quiet {
        return EXIT_OK;
    }

    for (layer, root) in &set.layers {
        match (layer, set.found_by) {
            (karen::policy::Layer::Project, Some(how)) => {
                println!(
                    "{} layer: {} (found by {how})",
                    layer.name(),
                    root.display()
                );
            }
            _ => println!("{} layer: {}", layer.name(), root.display()),
        }
    }
    println!("gates: {}  rules: {}", set.gates.len(), set.rules.len());
    for issue in &set.issues {
        println!(
            "{}: the {} layer {}",
            issue.kind.name(),
            issue.layer,
            issue.message
        );
    }
    print_runtime_layers(&set);
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

    let mut raw = match build_raw(args, &args.out) {
        Ok(raw) => raw,
        Err(code) => return code,
    };
    name_runtime(&mut raw, args.runtime.as_deref());

    let mut request = match normalize(args.adapter, raw) {
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

    let roots = policy_roots(
        args.policies.clone(),
        request.cwd.as_deref().map(std::path::Path::new),
    );
    let set = match load_policies(&roots, &args.out) {
        Ok(set) => set,
        Err(code) => return code,
    };
    let policy_root = set.root.display().to_string();
    let issues = issues_json(&set);
    let issue_lines: Vec<String> = set
        .issues
        .iter()
        .map(|i| format!("{}: the {} layer {}", i.kind.name(), i.layer, i.message))
        .collect();
    // Dry run means dry: she reads real strike state so the preview is honest,
    // but writes nothing, so rehearsing can never eat your only warning.
    // The Karen who lets you practice your apology without filing it.
    let state = Box::new(ReadOnlyStore::new(state_for(&set)));
    karen::engine::facts::enrich(&set, state.as_ref(), &mut request);
    let layer_of: std::collections::HashMap<String, &'static str> = set
        .gates
        .iter()
        .map(|g| (g.id.clone(), g.layer.name()))
        .collect();
    let layer = |id: &str| layer_of.get(id).copied().unwrap_or("unknown");

    let engine = Engine::new(set, state);
    let response = engine.dispatch(&request);

    let mut data = evaluation_json(&response, args.adapter, &policy_root, args.include_content);
    data["fired_layers"] = response
        .fired_policies
        .iter()
        .map(|id| json!({ "id": id, "layer": layer(id) }))
        .collect();
    data["issues"] = issues;
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
        let fired: Vec<String> = response
            .fired_policies
            .iter()
            .map(|id| format!("{id} ({})", layer(id)))
            .collect();
        println!("fired: {}", fired.join(", "));
    }
    println!("injections: {}", response.injections.len());
    for line in &issue_lines {
        println!("{line}");
    }
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

/// Every layer in `roots`. A broken user or project file is left out and
/// reported in the set's issues; only the built-in layer fails the load.
fn load_policies(roots: &policy::Roots, out: &OutputArgs) -> Result<PolicySet, i32> {
    policy::load_roots(roots).map_err(|error| {
        fail(
            out,
            "POLICY_INVALID",
            &error.to_string(),
            "The built-in layer ships inside Karen: report this as a bug.",
            EXIT_CONFIG,
        )
    })
}

fn issues_json(set: &PolicySet) -> Value {
    set.issues
        .iter()
        .map(|i| json!({ "layer": i.layer.name(), "message": i.message, "kind": i.kind.name() }))
        .collect()
}

/// Which layers can carry a `runtime_command` into each runtime the user layer
/// configured, so a runtime nothing reaches is a line in the report rather than
/// a log entry found later.
fn runtime_layers(set: &PolicySet) -> Value {
    set.config
        .actions
        .runtimes
        .iter()
        .map(|runtime| {
            json!({
                "runtime": runtime,
                "layers": karen::engine::runtime_command::layers_for(runtime),
            })
        })
        .collect()
}

/// The layer table in `karen validate`'s text, printed only when an enabled
/// gate would use it: a policy with no `runtime_command` has no use for the
/// list, and a report that always carried it would be noise.
fn print_runtime_layers(set: &PolicySet) {
    if !set
        .gates
        .iter()
        .any(|gate| gate.enabled && matches!(gate.action, ActionDef::RuntimeCommand { .. }))
    {
        return;
    }
    println!("runtime_command layers:");
    for entry in runtime_layers(set).as_array().into_iter().flatten() {
        let layers: Vec<&str> = entry["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let reach = if layers.is_empty() {
            "none".to_string()
        } else {
            layers.join(", ")
        };
        println!("  {}: {reach}", entry["runtime"].as_str().unwrap_or(""));
    }
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
                    "layer": gate.layer.name(),
                    "source": gate.source,
                    "locked": gate.locked,
                })
            })
            .collect::<Vec<_>>(),
        "layers": set
            .layers
            .iter()
            .map(|(layer, root)| json!({ "layer": layer.name(), "root": root.display().to_string() }))
            .collect::<Vec<_>>(),
        "project_found_by": set.found_by,
        "runtime_layers": runtime_layers(set),
        "changes": set
            .changes
            .iter()
            .map(|c| json!({ "gate": c.gate, "layer": c.layer.name(), "kind": c.kind }))
            .collect::<Vec<_>>(),
        "issues": issues_json(set),
        "rules": set
            .rules
            .iter()
            .map(|rule| {
                json!({
                    "id": rule.id,
                    "path": rule.path.display().to_string(),
                    "chars": rule.content.chars().count(),
                    "runtimes": rule.runtimes,
                })
            })
            .collect::<Vec<_>>(),
        "config": {
            "fail_open": set.config.engine.fail_open,
            "state_backend": set.config.state.backend,
            "state_dir": set.config.state.dir().display().to_string(),
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
        "runtime_commands": response.runtime_commands,
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
        ActionDef::Notify { .. } => "notify",
        ActionDef::RuntimeCommand { .. } => "runtime_command",
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
                "input": {"type": "path", "required": false, "description": "Read the payload from this file instead of stdin."},
                "runtime": {"type": "string", "required": false, "description": "The runtime running the hook, as the `runtime` fact; a `facts.runtime` in the payload wins."},
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
                "runtime": {"type": "string", "required": false, "description": "The runtime to rehearse as, as `hook --runtime` does."},
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
                "prompt_regex", "content_regex", "tool_regex", "field_equals", "field_regex", "field_json_regex", "field_gte", "field_lte", "file_older_than", "always"
            ],
            "actions": ["allow", "deny", "ask", "modify", "deny_once_then_warn", "notify", "runtime_command"],
            "exit_codes": {"0": "success", "2": "usage or invalid input", "3": "not found", "78": "invalid policies"},
        },
        "invariants": [
            "stdout carries data only; diagnostics and logs go to stderr",
            "hook output is the runtime protocol object, not the CLI JSON envelope",
            "evaluate is read-only and always exits 0 on a successful evaluation",
            "policies merge three layers: built-in, the user layer <Karen home>/policies, then the project layer (--policies, else KAREN_POLICIES, else the nearest .karen/policies up to the repository root, else git config karen.policies)",
            "a file, gate or rule that does not load is left out and reported; every other gate stays enforced",
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
    json!(karen::policy::gate::EVENT_NAMES)
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
                karen::home::claude_config_dir().unwrap_or_else(|| PathBuf::from(".claude"));
            (
                global,
                PathBuf::from(".claude"),
                "settings.json",
                claude_hooks_json(),
            )
        }
        SetupRuntime::Codex => {
            let global = karen::home::codex_home().unwrap_or_else(|| PathBuf::from(".codex"));
            (
                global,
                PathBuf::from(".codex"),
                "hooks.json",
                codex_hooks_json(),
            )
        }
        SetupRuntime::OpenCode => {
            let global = karen::home::opencode_config_dir()
                .unwrap_or_else(|| PathBuf::from(".config/opencode"));
            (
                global,
                PathBuf::from(".opencode"),
                "plugins/karen-ai.js",
                match pinned_binary() {
                    Some(path) => OPENCODE_PLUGIN_JS.replacen(
                        "process.env.KAREN_BIN || \"karen\"",
                        &format!("process.env.KAREN_BIN || {}", Value::from(path)),
                        1,
                    ),
                    None => OPENCODE_PLUGIN_JS.to_string(),
                },
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
        // The note names the binary the hooks were just written to call, so it never tells a
        // home install (whose hooks carry her pinned path) to go and put karen on PATH.
        match pinned_binary() {
            Some(path) if Path::new(&path).is_file() => {
                println!("\nNote: Hooks call {path}; replacing that file upgrades every hook.");
            }
            Some(path) => println!(
                "\nNote: karen is not on PATH, so the hooks call {path}, Karen's default home binary. Install karen there, or put it on PATH and run setup again."
            ),
            None => println!(
                "\nNote: Hooks use the karen binary from PATH. Install karen first (cargo install karen-ai)."
            ),
        }
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
// Occupancy facts ride the payload too: the session's context tokens (kept current by
// `message.updated`) and its model's context window (resolved once per session), so a
// gate on `facts.context_ratio` runs on OpenCode without reading anything itself. The
// provider's own failures (`session.error`) ride the Stop review the same way. A prompt is
// reviewed as a UserPromptSubmit through the `chat.message` hook, and a compaction
// (`session.compacted`) is her PostCompact, so `reset_on_compact` works here too.
// A gate's `runtime_command` comes back from Karen as `karenRuntimeCommands`; this bridge types it
// through OpenCode's own server API once the session is idle (`/compact` as `session.summarize`, a
// custom command as `session.command`, a TUI built-in as `tui.executeCommand`) and files what
// became of it in `karen-runtime.jsonl`, the ledger Karen writes herself.
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { appendFileSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

const KAREN = process.env.KAREN_BIN || "karen";
const KAREN_HOME = process.env.KAREN_HOME || join(homedir(), ".karen");
// Every failure below fails open, so each one leaves a line here (KAREN_LOG overrides the path):
// a stop that was not held can then be explained from evidence instead of guessed.
const LOG = process.env.KAREN_LOG || join(KAREN_HOME, "logs", "karen-opencode.log");
// A payload holds tool input and the session's own words: it is written where only this user
// can read it, under a name nobody can predict, and never through a file placed there first.
const RUN = join(KAREN_HOME, "run");
const fail = (stage, sid, detail) => {
  try {
    mkdirSync(dirname(LOG), { recursive: true });
    appendFileSync(LOG, `${new Date().toISOString()} pid=${process.pid} session=${sid ?? "-"} ${stage}: ${String(detail?.message ?? detail ?? "").trim().slice(0, 500)}\n`);
  } catch {}
};
const TOOL_NAMES = { list: "LS", webfetch: "WebFetch", todowrite: "TodoWrite", todoread: "TodoRead", multiedit: "MultiEdit" };
const toolName = (tool) => TOOL_NAMES[tool] ?? (tool ? tool[0].toUpperCase() + tool.slice(1) : tool);

// The layers this bridge runs itself. Karen hands a `runtime_command` back only for a layer the
// payload names, so a bridge from before the layer existed never receives one it would drop.
const LAYERS = ["opencode-native"];
const RUNTIME_LOG = join(KAREN_HOME, "logs", "karen-runtime.jsonl");
// What became of a command after Karen handed it over, in the ledger she writes herself. Never
// the arguments: they are rendered from the session's facts, and the ledger keeps none of that.
const filed = (sid, cmd, route, status) => {
  try {
    mkdirSync(dirname(RUNTIME_LOG), { recursive: true });
    appendFileSync(RUNTIME_LOG, JSON.stringify({ time: Date.now(), session: sid, gate: cmd.gate, runtime: "opencode", command: cmd.command, layer: "opencode-native", route, status }) + "\n");
  } catch {}
};
// Set by KarenAI, which owns the client: takes the commands Karen handed back for a session.
let handOff = () => {};

// An async spawn, never spawnSync: under OpenCode's embedded Bun on Windows, spawnSync of the
// same binary timed out (ETIMEDOUT) on every call even with no stdin, while an async spawn
// answers in ~30 ms; a synchronous call would also stall OpenCode's event loop. Every call
// site awaits the promise, which resolves to `{}` on any failure (fail-open, logged).
function karen(payload, part) {
  const args = ["hook", "--adapter", "claude-classic"];
  if (part) args.push("--part", String(part));
  // The runtime is a fact only the caller knows; Karen reads the rest (MCP servers, tools, git) herself.
  const facts = { runtime: "opencode", ...(payload.facts ?? {}) };
  // The payload reaches Karen as a file named by --input and she gets no stdin:
  // OpenCode's runtime left a piped payload undelivered on some calls, and a file
  // descriptor handed over as stdin on every call, until each timed out.
  const file = join(RUN, `opencode-${process.pid}-${randomUUID()}.json`);
  args.push("--input", file);
  const sid = payload.session_id;
  const event = payload.hook_event_name;
  return new Promise((resolve) => {
    let settled = false;
    const done = (value) => {
      if (settled) return;
      settled = true;
      try { unlinkSync(file); } catch {}
      resolve(value);
    };
    let stdout = "";
    let stderr = "";
    let child;
    try {
      mkdirSync(RUN, { recursive: true, mode: 0o700 });
      // A rules fetch (`part`) runs under a made-up session id: nothing typed there could reach a session.
      writeFileSync(file, JSON.stringify({ ...payload, facts, ...(part ? {} : { runtime_layers: LAYERS }) }), { mode: 0o600, flag: "wx" });
      child = spawn(KAREN, args, { stdio: ["ignore", "pipe", "pipe"], windowsHide: true });
    } catch (e) {
      fail(`spawn ${KAREN} on ${event}`, sid, e);
      return done({});
    }
    child.stdout.on("data", (d) => { stdout += d; });
    child.stderr.on("data", (d) => { stderr += d; });
    const timer = setTimeout(() => {
      try { child.kill(); } catch {}
      fail(`timeout after 10000 ms on ${event}`, sid, stderr || "(no stderr)");
      done({});
    }, 10000);
    child.on("error", (e) => { clearTimeout(timer); fail(`spawn ${KAREN} on ${event}`, sid, e); done({}); });
    child.on("close", (status) => {
      clearTimeout(timer);
      if (status !== 0) { fail(`exit ${status} on ${event}`, sid, stderr || "(no stderr)"); return done({}); }
      let value;
      try { value = JSON.parse(stdout || "{}"); } catch (e) { fail(`unreadable output on ${event}`, sid, e); return done({}); }
      // The commands leave the answer here: the hook sites below read verdicts and never see them.
      const commands = Array.isArray(value?.karenRuntimeCommands) ? value.karenRuntimeCommands : [];
      if (value && typeof value === "object") delete value.karenRuntimeCommands;
      done(value);
      if (commands.length) handOff(sid, commands);
    });
  });
}

// OpenCode has no Stop hook: an idle session is reviewed as a Stop, and a block is
// sent back into the session as a prompt, which is how OpenCode keeps a turn going.
const lastAssistant = (messages) => {
  const m = [...(messages ?? [])].reverse().find((x) => x?.info?.role === "assistant");
  if (!m) return null;
  const t = m.info.tokens;
  return {
    id: m.info.id,
    text: (m.parts ?? []).filter((p) => p.type === "text").map((p) => p.text).join("\n"),
    // Context tokens as Karen counts them: input plus both cache counts. A compaction's own
    // summary message counted the context it replaced, so it measures nothing of the new one.
    tokens: t && !m.info.summary ? (t.input ?? 0) + (t.cache?.read ?? 0) + (t.cache?.write ?? 0) : 0,
    // The model this message ran on, which decides the window its tokens fill.
    model: m.info.providerID && m.info.modelID ? { providerID: m.info.providerID, modelID: m.info.modelID } : undefined,
  };
};

export const KarenAI = async ({ directory, client }) => {
  const rules = new Map();
  const notices = new Map();
  const reviewed = new Map(); // session -> last assistant message id already reviewed
  const holds = new Map(); // session -> stops Karen has held in a row
  const plans = new Map(); // session -> todo list signature at its last review
  const offDuty = new Set(); // sessions already told that no policy is enforced
  const lastTokens = new Map(); // session -> context tokens of its last assistant message
  const models = new Map(); // session -> the model of the last assistant message that reported tokens
  const windows = new Map(); // session -> { model, limit }: the window resolved for that model, limit null when resolved to none
  const errors = new Map(); // session -> the provider's own last failure, cleared when a later message completes clean
  // A systemMessage holds one line per notice. A policy that does not load fails open, and
  // each session hears that once, not on every call; every other line passes through.
  const notice = (sid, message) => {
    const lines = String(message ?? "").split("\n").filter((line) => {
      if (!line.startsWith("Karen-AI is not enforcing")) return line.length > 0;
      if (offDuty.has(sid)) return false;
      offDuty.add(sid);
      return true;
    });
    return lines.length ? lines.join("\n") : null;
  };
  // Only Karen's own status lines wake an idle session, and each at most once (the binary tells
  // a policy merge once per session). A gate's warning on an allowed stop never does: waking a
  // session for it would end every reply in another stop, another review and another wake.
  const STATUS = /^Karen-AI (is not enforcing|policy merge)/;
  const statusNotice = (sid, message) => {
    const lines = (notice(sid, message) ?? "").split("\n").filter((line) => STATUS.test(line));
    return lines.length ? lines.join("\n") : null;
  };
  // The occupancy facts of a session, as its own payload `facts`: the context tokens of its
  // last assistant message (kept current by `message.updated`) and the context window of the
  // model that message ran on, resolved once per model and cached. A session that switches
  // model mid-run is measured against the new model's window from its next message on. A gate
  // on every tool call then costs no API call of its own, and each fact is supplied only when
  // known: Karen reads a payload's own facts first and never guesses one.
  const contextFacts = async (sid) => {
    const facts = {};
    const tokens = lastTokens.get(sid);
    if (Number.isFinite(tokens)) facts.context_tokens = tokens;
    // The provider's own last failure rides the Stop review, because that is where a
    // gate can act on it: OpenCode never delivers a ProviderError to the agent, so the
    // failure would otherwise be a fact nobody can branch on. Cleared the moment a
    // later assistant message completes without an error, so the fact never goes stale.
    const failure = sid ? errors.get(sid) : undefined;
    if (failure) facts.provider_error = failure;
    if (sid && client?.config) {
      try {
        // The assistant message names its model in the SDK's own types. A session that has
        // not produced one yet is asked once for the model its record carries, as the wake
        // is; what that finds, or fails to find, is cached like any other window.
        let model = models.get(sid);
        if (!model && !windows.has(sid) && client.session) {
          const s = (await client.session.get({ path: { id: sid } }))?.data;
          if (s?.model?.providerID && s?.model?.modelID) model = { providerID: s.model.providerID, modelID: s.model.modelID };
          else windows.set(sid, { model: null, limit: null });
        }
        const key = model ? JSON.stringify([model.providerID, model.modelID]) : null;
        if (key && windows.get(sid)?.model !== key) {
          const found = (await client.config.providers())?.data?.providers?.find((p) => p.id === model.providerID)?.models?.[model.modelID]?.limit?.context;
          // A window she could not resolve is cached as null, so one failed lookup never
          // becomes one lookup per tool call; the fact stays absent, never guessed.
          windows.set(sid, { model: key, limit: Number.isFinite(found) ? found : null });
        }
        const limit = windows.get(sid)?.limit;
        if (Number.isFinite(limit)) facts.context_window = limit;
      } catch (e) {
        fail("context window on session.get/providers", sid, e);
      }
    }
    return facts;
  };
  // A prompt sent without a model runs on the session's creation-time model, so a session
  // whose model was changed mid-run would be woken on the old one. The wake names the model
  // the session uses now: the live session record first, then the "provider/model" field of
  // OPENCODE_CONFIG, else none (the server default).
  const wakeModel = async (sid) => {
    try {
      const s = (await client.session.get({ path: { id: sid } }))?.data;
      const modelID = s?.model?.modelID ?? s?.model?.id;
      if (s?.model?.providerID && modelID) return { providerID: s.model.providerID, modelID };
    } catch {}
    try {
      const model = process.env.OPENCODE_CONFIG ? JSON.parse(readFileSync(process.env.OPENCODE_CONFIG, "utf8")).model : null;
      const slash = typeof model === "string" ? model.indexOf("/") : -1;
      if (slash > 0) return { providerID: model.slice(0, slash), modelID: model.slice(slash + 1) };
    } catch {}
    return undefined;
  };
  // Resolves once the prompt is sent, never when it is answered: the prompt's own promise
  // settles only after the model replies, and the event loop must not wait for that.
  const wake = async (sid, text, label) => {
    const model = await wakeModel(sid);
    client.session
      .prompt({ path: { id: sid }, body: { ...(model ? { model } : {}), parts: [{ type: "text", text }] } })
      .catch((e) => fail(`session.prompt (${label})`, sid, e));
  };
  // A command is typed when the session is idle, never into the middle of a turn: one handed back
  // while a turn runs waits for the session's next idle, and is filed as undelivered if none comes.
  const busy = new Set(); // sessions mid-turn: a tool ran, a prompt landed, or the server said busy
  const pending = new Map(); // session -> commands waiting for idle, oldest first
  const WAIT_MS = Number(process.env.KAREN_COMMAND_WAIT_MS) || 120000; // KAREN_COMMAND_WAIT_MS shortens it, for tests
  // The built-ins that live in the TUI, as the SDK names them for `tui.executeCommand`.
  const TUI_COMMANDS = { new: "session.new", share: "session.share" };
  // A request that comes back with an error answers later than the call returns, so the answer is read where it lands.
  const outcome = (sid, cmd, route, call) => {
    const failed = (detail) => { fail(`runtime_command ${cmd.command} via ${route}`, sid, detail); filed(sid, cmd, route, "undelivered:error"); };
    Promise.resolve(call).then((r) => { if (r?.error) failed(`HTTP ${r?.response?.status ?? "error"}`); }, failed);
  };
  const send = async (sid, cmd) => {
    const name = String(cmd.command).replace(/^\//, "");
    try {
      if (name === "compact" || name === "summarize") {
        // The TUI's own /compact calls this, so it needs no TUI. The call takes no instructions.
        const model = (await wakeModel(sid)) ?? models.get(sid);
        if (!model) return filed(sid, cmd, "session.summarize", "undelivered:no-model");
        outcome(sid, cmd, "session.summarize", client.session.summarize({ path: { id: sid }, body: { providerID: model.providerID, modelID: model.modelID } }));
        return filed(sid, cmd, "session.summarize", "sent");
      }
      const known = (await client.command?.list?.())?.data;
      if (Array.isArray(known) && known.some((c) => c?.name === name)) {
        outcome(sid, cmd, "session.command", client.session.command({ path: { id: sid }, body: { command: name, arguments: cmd.arguments ?? "" } }));
        return filed(sid, cmd, "session.command", "sent");
      }
      if (TUI_COMMANDS[name] && client.tui?.executeCommand) {
        // The server answers true whether or not a TUI is listening, so this is never "sent".
        outcome(sid, cmd, "tui.executeCommand", client.tui.executeCommand({ body: { command: TUI_COMMANDS[name] } }));
        return filed(sid, cmd, "tui.executeCommand", "sent-unconfirmed");
      }
      filed(sid, cmd, "none", "undelivered:unknown-command");
    } catch (e) {
      fail(`runtime_command ${cmd.command}`, sid, e);
      filed(sid, cmd, "none", "undelivered:error");
    }
  };
  // One command per idle: a command starts a turn of its own (a compaction, a custom prompt), and
  // the next waits for that turn's idle.
  const flush = (sid) => {
    const queue = pending.get(sid);
    const next = queue?.shift();
    if (!next) return;
    clearTimeout(next.timer);
    if (!queue.length) pending.delete(sid);
    busy.add(sid);
    send(sid, next.cmd);
  };
  handOff = (sid, commands) => {
    if (!sid || !client) return;
    for (const cmd of commands) {
      const queue = pending.get(sid) ?? [];
      const item = { cmd };
      item.timer = setTimeout(() => {
        const left = pending.get(sid);
        if (!left?.includes(item)) return;
        left.splice(left.indexOf(item), 1);
        if (!left.length) pending.delete(sid);
        filed(sid, cmd, "none", "undelivered:timeout");
      }, WAIT_MS);
      item.timer.unref?.();
      queue.push(item);
      pending.set(sid, queue);
    }
    if (!busy.has(sid)) flush(sid);
  };
  return {
    event: async ({ event }) => {
      // `message.updated` is the meter reader: each assistant message reports its tokens as
      // it lands, so the cache a tool hook reads is current without the hook asking anyone.
      if (event?.type === "message.updated") {
        const info = event?.properties?.info;
        if (info?.role === "assistant") {
          const t = info.tokens;
          const sid = event?.properties?.sessionID ?? info.sessionID;
          // A completed assistant message without an error means the pipe works
          // again: the provider's last failure stops being a fact the moment a
          // later call gets through.
          if (sid && info.time?.completed && !info.error) errors.delete(sid);
          const tokens = t && !info.summary ? (t.input ?? 0) + (t.cache?.read ?? 0) + (t.cache?.write ?? 0) : 0;
          // Tokens and the model they were counted on travel together, so a ratio never
          // divides one model's tokens by another's window.
          if (sid && tokens > 0) {
            lastTokens.set(sid, tokens);
            if (info.providerID && info.modelID) models.set(sid, { providerID: info.providerID, modelID: info.modelID });
          }
        }
        return;
      }
      // The provider speaking for itself: cache the failure for the Stop review,
      // and file it with Karen as its own event for whoever tails the log. The
      // failure never reaches the agent here — OpenCode has no stop to hold —
      // so the Stop that follows is where a gate gets to say it.
      if (event?.type === "session.error") {
        const sid = event?.properties?.sessionID;
        const err = event?.properties?.error;
        if (sid && err) {
          const message = err?.data?.message ?? err?.message ?? String(err?.name ?? "provider error");
          const model = await wakeModel(sid);
          errors.set(sid, { message, ...(model ? { provider: model.providerID, model: model.modelID } : {}) });
        }
        await karen({ hook_event_name: "ProviderError", session_id: sid, cwd: directory, facts: { runtime: "opencode", ...(sid && errors.has(sid) ? { provider_error: errors.get(sid) } : {}) } });
        return;
      }
      // A compaction is the runtime's PostCompact: the context was rebuilt, so every
      // `reset_on_compact` gate forgets its strikes. The tokens counted on the old context are
      // not a measurement of the new one, so they are dropped too; the fact stays absent until
      // the next reply reports a count, and absence is never a match.
      if (event?.type === "session.compacted") {
        const sid = event?.properties?.sessionID;
        if (sid) {
          lastTokens.delete(sid);
          await karen({ hook_event_name: "PostCompact", session_id: sid, cwd: directory });
        }
        return;
      }
      // A deleted session's entries are dropped the moment it is, so a long-running
      // OpenCode does not carry a Map for every session it ever saw.
      if (event?.type === "session.deleted") {
        const sid = event?.properties?.sessionID;
        if (sid) {
          rules.delete(sid);
          reviewed.delete(sid);
          holds.delete(sid);
          plans.delete(sid);
          offDuty.delete(sid);
          lastTokens.delete(sid);
          models.delete(sid);
          windows.delete(sid);
          errors.delete(sid);
          busy.delete(sid);
          for (const item of pending.get(sid) ?? []) {
            clearTimeout(item.timer);
            filed(sid, item.cmd, "none", "undelivered:session-deleted");
          }
          pending.delete(sid);
        }
        return;
      }
      if (event?.type === "session.status" && event.properties?.status?.type && event.properties.status.type !== "idle" && event.properties.sessionID) {
        busy.add(event.properties.sessionID);
        return;
      }
      const idle = event?.type === "session.idle" || (event?.type === "session.status" && event.properties?.status?.type === "idle");
      const sid = event?.properties?.sessionID;
      // The turn is over, so a command that waited for it goes now, ahead of the review below.
      if (idle && sid) { busy.delete(sid); flush(sid); }
      if (!idle || !sid || !client?.session) return;
      let last = null;
      try { last = lastAssistant((await client.session.messages({ path: { id: sid } }))?.data); } catch (e) { fail("session.messages on idle", sid, e); return; }
      if (!last || reviewed.get(sid) === last.id) return;
      reviewed.set(sid, last.id);
      // The latest assistant message is in hand, so its tokens are read from it directly:
      // the `message.updated` cache is current even if this OpenCode process just loaded
      // and never saw the turn land.
      if (last.tokens > 0) {
        lastTokens.set(sid, last.tokens);
        if (last.model) models.set(sid, last.model);

      }
      // An inline child (task tool) has a parentID and stops as a subagent, as in Claude Code. A lane started
      // in its own terminal is a top-level session and gets Stop: scope gates to roles with conditions (cwd).
      let parent = null;
      try { parent = (await client.session.get({ path: { id: sid } }))?.data?.parentID ?? null; } catch {}
      // The session's own todo list is evidence the closing text cannot reword. Without the todo API the
      // fields are left out, so no todo gate can fire on missing data.
      let todo = {};
      try {
        const todos = (await client.session.todo({ path: { id: sid } }))?.data;
        if (Array.isArray(todos)) {
          const sig = JSON.stringify(todos.map((t) => [t.content, t.status]));
          const before = plans.get(sid);
          plans.set(sid, sig);
          todo = {
            open_todos: todos.filter((t) => t.status === "pending" || t.status === "in_progress").map((t) => `${t.status}: ${t.content}`).join("\n"),
            todo_count: todos.length,
            todos_updated: before === undefined || before !== sig, // no baseline yet: nothing to accuse
          };
        }
      } catch {}
      // As in Claude Code, stop_hook_active is true on every Stop that follows a hold, so a gate
      // that exempts it holds a session only once; bound repeated holds with the gate's strike
      // state instead.
      const out = await karen({ hook_event_name: parent ? "SubagentStop" : "Stop", session_id: sid, cwd: directory, stop_hook_active: holds.has(sid), stop_hold_count: holds.get(sid) ?? 0, last_assistant_message: last.text, ...todo, ...(parent ? { agent_id: sid } : {}), facts: await contextFacts(sid) });
      if (out.decision !== "block") {
        holds.delete(sid);
        const status = statusNotice(sid, out.systemMessage);
        if (status) await wake(sid, `[Karen-AI] ${status}`, "notice");
        return;
      }
      holds.set(sid, (holds.get(sid) ?? 0) + 1);
      await wake(sid, `[Karen-AI] ${out.reason ?? "blocked by policy"}`, "hold not delivered");
    },
    "experimental.chat.system.transform": async (input, output) => {
      const sid = input.sessionID ?? "unknown";
      if (!rules.has(sid)) {
        const parts = [];
        for (let part = 1; part <= 64; part++) {
          const out = await karen({ hook_event_name: "SessionStart", session_id: `opencode-rules:${process.pid}:${sid}`, cwd: directory }, part);
          const context = out?.hookSpecificOutput?.additionalContext;
          if (!context) break;
          parts.push(context);
        }
        rules.set(sid, parts.join("\n\n"));
      }
      if (rules.get(sid)) output.system.push(rules.get(sid));
    },
    // OpenCode's UserPromptSubmit. The user's own words are the text parts they typed; a
    // synthetic part is OpenCode's own (an attachment read for the model) or another plugin's.
    // A block throws, as a tool block does, so the prompt never becomes a message. Rules are
    // not delivered here (they ride the system prompt above); only her notices ride along,
    // as one synthetic part the agent reads, sorted after the part it follows.
    "chat.message": async (input, output) => {
      const sid = input?.sessionID ?? output?.message?.sessionID;
      if (sid) busy.add(sid);
      const parts = output?.parts ?? [];
      const prompt = parts.filter((p) => p?.type === "text" && !p.synthetic && !p.ignored).map((p) => p.text).join("\n");
      const out = await karen({ hook_event_name: "UserPromptSubmit", session_id: sid, cwd: directory, prompt, facts: await contextFacts(sid) });
      if (out.decision === "block") throw new Error(`Karen-AI: ${out.reason ?? "blocked by policy"}`);
      const message = notice(sid, out.systemMessage);
      if (!message) return;
      const last = parts[parts.length - 1]?.id;
      parts.push({
        id: last ? `${last}z` : `prt_karen${randomUUID().replaceAll("-", "")}`,
        sessionID: sid,
        messageID: output?.message?.id,
        type: "text",
        text: `[Karen-AI] ${message}`,
        synthetic: true,
      });
    },
    "tool.execute.before": async (input, output) => {
      if (input.sessionID) busy.add(input.sessionID);
      const out = await karen({ hook_event_name: "PreToolUse", session_id: input.sessionID, cwd: directory, tool_name: toolName(input.tool), tool_input: output.args, facts: await contextFacts(input.sessionID) });
      if (out.modifiedInput && typeof out.modifiedInput === "object") {
        Object.assign(output.args, out.modifiedInput);
      } else {
        const decision = out.hookSpecificOutput?.permissionDecision;
        const reason = out.hookSpecificOutput?.permissionDecisionReason ?? "blocked by policy";
        if (decision === "deny") throw new Error(`Karen-AI: ${reason}`);
        if (decision === "ask") throw new Error(`Karen-AI: needs human approval, and OpenCode plugins cannot ask: ${reason}`);
      }
      const message = notice(input.sessionID, out.systemMessage);
      if (message) notices.set(input.callID, message);
    },
    "tool.execute.after": async (input, output) => {
      if (input.sessionID) busy.add(input.sessionID);
      const out = await karen({ hook_event_name: "PostToolUse", session_id: input.sessionID, cwd: directory, tool_name: toolName(input.tool), tool_input: input.args, tool_response: output.output, facts: await contextFacts(input.sessionID) });
      const notes = [notices.get(input.callID), out.decision === "block" ? out.reason : null, notice(input.sessionID, out.systemMessage)].filter(Boolean);
      notices.delete(input.callID);
      if (notes.length) output.output = `${output.output ?? ""}\n\n[Karen-AI] ${notes.join("\n")}`;
    },
  };
};
"#;

/// The bare binary name a hook runs from PATH.
const BARE_BINARY: &str = if cfg!(windows) { "karen.exe" } else { "karen" };

/// The absolute binary a hook command names, or `None` when hooks run the bare
/// name from PATH. Installed in her home (`<Karen home>/bin`), she pins her own
/// path, so a hook never depends on PATH. Run from anywhere else (a build
/// directory, a package manager's bin) she writes the bare name when PATH
/// resolves it, and otherwise falls back to her home's default binary, so a hook
/// never names a command its shell cannot find.
fn pinned_binary() -> Option<String> {
    let home_bin = karen::home::karen_home().join("bin");
    let exe = std::env::current_exe().and_then(std::fs::canonicalize);
    if let (Ok(exe), Ok(home)) = (exe, std::fs::canonicalize(&home_bin))
        && exe.starts_with(&home)
    {
        return Some(shown_path(&exe));
    }
    if on_path(BARE_BINARY) {
        return None;
    }
    Some(shown_path(&home_bin.join(BARE_BINARY)))
}

/// A path as a hook command shows it: no verbatim prefix, forward slashes.
fn shown_path(path: &Path) -> String {
    let shown = path.display().to_string();
    let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown);
    shown.replace('\\', "/")
}

/// Whether a file named `name` sits in a directory on PATH (an unset PATH holds none).
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

/// A hook command's binary, as it sits inside a JSON string.
fn hook_binary() -> String {
    pinned_binary().map_or_else(|| BARE_BINARY.to_string(), |path| format!("\\\"{path}\\\""))
}

fn claude_hooks_json() -> String {
    let karen = hook_binary();
    format!(
        "{{
  \"_comment\": \"Claude Code hook registration for Karen-AI. Rule injection (karen hook --part N) and gate enforcement (karen hook) both use the Rust binary, which evaluates JSON/JSONC/JSON5 gates. This file is installed by `karen setup claude` — no manual editing of settings.json required.\",
  \"hooks\": {{
    \"SessionStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude --part 1\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude --part 2\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude --part 3\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude --part 4\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude --part 5\", \"timeout\": 10}}] }}
    ],
    \"UserPromptSubmit\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 10}}] }}
    ],
    \"PreToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 10}}] }}
    ],
    \"PostToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 10}}] }}
    ],
    \"Stop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 5}}] }}
    ],
    \"SessionEnd\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 5}}] }}
    ],
    \"PreCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 5}}] }}
    ],
    \"PostCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 5}}] }}
    ],
    \"SubagentStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 10}}] }}
    ],
    \"SubagentStop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime claude\", \"timeout\": 5}}] }}
    ]
  }}
}}")
}

fn codex_hooks_json() -> String {
    let karen = hook_binary();
    format!(
        "{{
  \"_comment\": \"Codex hook registration for Karen-AI. Rule injection (karen hook --part N) and gate enforcement (karen hook) both use the Rust binary, which evaluates JSON/JSONC/JSON5 gates. This file is installed by `karen setup codex` — no manual editing of Codex config required. Codex auto-discovers hooks from .codex-plugin/plugin.json.\",
  \"hooks\": {{
    \"SessionStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex --part 1\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex --part 2\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex --part 3\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex --part 4\", \"timeout\": 10}}] }},
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex --part 5\", \"timeout\": 10}}] }}
    ],
    \"UserPromptSubmit\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 10}}] }}
    ],
    \"PreToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 10}}] }}
    ],
    \"PostToolUse\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 10}}] }}
    ],
    \"Stop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 5}}] }}
    ],
    \"PreCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 5}}] }}
    ],
    \"PostCompact\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 5}}] }}
    ],
    \"SubagentStart\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 10}}] }}
    ],
    \"SubagentStop\": [
      {{ \"hooks\": [{{\"type\": \"command\", \"command\": \"{karen} hook --adapter claude-classic --runtime codex\", \"timeout\": 5}}] }}
    ]
  }}
}}")
}
