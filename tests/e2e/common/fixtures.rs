// tests/e2e/common/fixtures.rs
// The Karen who creates test fixtures because she knows hardcoded paths in tests are a sin.

use crate::e2e::common::{karen_binary_path, run_command};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Default config.toml content for tests
pub const DEFAULT_CONFIG: &str = r#"# Personal Rules engine configuration.
# Paths are relative to this file's directory unless absolute.

[engine]
fail_open = true
log_level = "warn"
timeout_ms = 5000

[policy]
rules_dir = "rules"
gates_dir = "gates"
validate_on_load = true
watch_for_changes = true

[state]
backend = "auto"
file_dir = "~/.cache/karen"
"#;

/// Gate definitions for test fixtures
pub const DEFAULT_GATES: &[GateFixture] = &[
    GateFixture {
        file: "commit-attribution.json",
        content: r#"{
  "id": "commit-attribution",
  "name": "Block AI Attribution in Commits",
  "description": "The standing rule forbids AI/co-author attribution trailers. This gate intentionally overrides the harness default that appends a Co-Authored-By line. The Karen who blocks attribution trailers, because she doesn't give credit to her shovel, either.",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 110,
  "enabled": true,
  "tags": ["safety", "git", "attribution"],
  "match": {
    "all": [
      { "type": "command_regex", "pattern": "(^|[;&|(){])\\s*([A-Za-z_][A-Za-z0-9_]*=\\S*\\s+)*git(\\s+(-\\S+|--\\S+|-C\\s+\\S+))*\\s+commit(\\s|$)" },
      { "type": "command_regex", "pattern": "(?i)co-authored-by:|🤖|noreply@anthropic\\.com|generated with \\[?claude" }
    ]
  },
  "action": {
    "type": "deny",
    "reason": "Commit message contains AI/co-author attribution (Co-Authored-By / robot emoji / 'Generated with [Claude]' / noreply@anthropic.com). Remove the attribution trailer and re-commit."
  }
}"#,
    },
    GateFixture {
        file: "jq-yaml-redirect.json",
        content: r#"{
  "id": "jq-yaml-redirect",
  "name": "Redirect jq on YAML/TOML to yq",
  "description": "jq cannot parse YAML/TOML. First attempt is denied with the yq equivalent; an immediate retry is allowed so a deliberate fallback flows without a human. The Karen who redirects jq to yq because jq literally cannot parse YAML/TOML and she knows it.",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 80,
  "enabled": true,
  "tags": ["tooling", "cli-preference"],
  "match": {
    "all": [
      { "type": "command_regex", "pattern": "\\bjq\\b" },
      { "type": "command_regex", "pattern": "(?i)\\.(yaml|yml|toml)\\b" }
    ]
  },
  "action": {
    "type": "deny_once_then_warn",
    "first_reason": "CLI-tool preference: `jq` cannot parse YAML/TOML. Use `yq` for this file (`yq -i '<expr>' <file>` for in-place, `yq '<expr>' <file>` to read). `jq` is for JSON only. If you have already tried yq and it does not serve, or this file is actually JSON despite the extension, call `jq` again now and the retry will be allowed automatically.",
    "second_reason": "Allowing jq on a YAML/TOML path on retry (deliberate fallback)."
  },
  "state": {
    "track": "session",
    "key": "jq-yaml",
    "max_strikes": 2
  }
}"#,
    },
    GateFixture {
        file: "web-search-redirect.json",
        content: r#"{
  "id": "web-search-redirect",
  "name": "Prefer Brave/Exa over native web search",
  "description": "Steer reflexive native WebSearch/WebFetch calls toward the Brave/Exa MCP tools, while keeping native usable as an automatic last resort. A fresh call is denied with the routing rule; an immediate retry is allowed. The Karen who redirects web search to Brave/Exa because native search is lazy and she knows better tools exist.",
  "event": "PreToolUse",
  "tool": "WebSearch|WebFetch",
  "priority": 70,
  "enabled": true,
  "tags": ["tooling", "web-search"],
  "match": {
    "all": [
      { "type": "always" }
    ]
  },
  "action": {
    "type": "deny_once_then_warn",
    "first_reason": "Web-search routing preference. Prefer the Brave MCP tools first: brave_web_search for search/discovery, brave_llm_context for page substance/grounding. For the full content of a specific known URL, use Exa web_fetch_exa. Native WebSearch/WebFetch is a last-resort fallback. If you have already tried Brave/Exa and still need native - or Brave/Exa is unavailable - call the native tool again now and the retry will be allowed automatically.",
    "second_reason": "Allowing native web search on retry (deliberate fallback)."
  },
  "state": {
    "track": "session",
    "key": "web-search-native",
    "max_strikes": 2
  }
}"#,
    },
    GateFixture {
        file: "head-to-select-object.json",
        content: r#"{
  "id": "head-to-select-object",
  "name": "Convert Unix head to PowerShell Select-Object",
  "description": "On Windows, `head` is not a native command. Convert `head -n N` or `head -N` to `Select-Object -First N` for cross-platform compatibility. The Karen who ports, because she's seen too many CI pipelines fail on Windows runners.",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 60,
  "enabled": true,
  "tags": ["portability", "windows", "cross-platform", "shell"],
  "match": {
    "all": [
      { "type": "command_regex", "pattern": "\\bhead\\b" }
    ]
  },
  "action": {
    "type": "modify",
    "reason": "Converting Unix `head` to PowerShell `Select-Object` for Windows compatibility.",
    "modifier": "head-to-select-object",
    "input": null
  }
}"#,
    },
    GateFixture {
        file: "cargo-quality-gate.json",
        content: r#"{
  "id": "cargo-quality-gate",
  "name": "Cargo quality checks (clippy, fmt, test)",
  "description": "Runs cargo clippy, cargo fmt --check, and cargo test before allowing commits/pushes. The Karen who insists the project practices what it preaches.",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 100,
  "enabled": true,
  "tags": ["ci", "quality", "cargo", "clippy", "rustfmt"],
  "match": {
    "all": [
      { "type": "command_regex", "pattern": "git\\s+(push|commit)" }
    ]
  },
  "action": {
    "type": "deny_once_then_warn",
    "first_reason": "Before pushing or committing, run the quality checks: cargo fmt --check; cargo clippy -- -D warnings; cargo test. The Karen who practices what she preaches demands clean code.",
    "second_reason": "Allowing this time, but fix the warnings next time."
  },
  "state": {
    "track": "session",
    "key": "cargo-quality",
    "max_strikes": 2
  }
}"#,
    },
    GateFixture {
        file: "backticks-in-comments.json",
        content: r#"{
  "id": "backticks-in-comments",
  "name": "Require backticks for code/command names in comments",
  "description": "Commands, program names, library names, and technical identifiers in comments must use backticks. The Karen who insists on formatting discipline.",
  "event": "PreToolUse",
  "tool": "Write|Edit|MultiEdit",
  "priority": 90,
  "enabled": true,
  "tags": ["style", "comments", "formatting"],
  "match": {
    "all": [
      { "type": "file_regex", "pattern": "\\.rs$" },
      { "type": "content_regex", "pattern": "\\b(?:cargo|rustc|karen|git|clang|rustup|serde|clap|tokio|async|await|fn|struct|enum|impl|mod|use|let|mut|const|async|await)\\b" }
    ]
  },
  "action": {
    "type": "deny_once_then_warn",
    "first_reason": "Comments must use backticks for command names, program names, library names, and Rust keywords. Wrap them in backticks like `cargo`, `karen`, `git`, `fn`, `struct`, `async`, etc. This keeps documentation readable and consistent.",
    "second_reason": "Allowing this time, but please fix the formatting in future edits."
  },
  "state": {
    "track": "session",
    "key": "backticks-in-comments",
    "max_strikes": 2
  }
}"#,
    },
    GateFixture {
        file: "jsonc-test-gate.jsonc",
        content: r#"{
  // This is a single-line comment
  "id": "jsonc-test-gate",
  "name": "JSONC Format Test Gate",
  "description": "Tests that JSONC (JSON with Comments) format is parsed correctly. The Karen who validates formats.",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 10,
  "enabled": true,
  "tags": ["testing", "format", "jsonc"],
  "match": {
    "all": [
      { "type": "command_regex", "pattern": "jsonc-test" }
    ]
  },
  "action": {
    "type": "deny",
    "reason": "This gate only exists to verify JSONC parsing works. It should never fire in normal use."
  },
  // Trailing comma in object (allowed in JSONC)
}"#,
    },
];

/// Rule definitions for test fixtures
pub const DEFAULT_RULES: &[RuleFixture] = &[
    RuleFixture {
        file: "00-edit-and-commit-core.md",
        content: r#"# Edit and Commit Core Rules

The Karen who edits with purpose and commits with conviction.

## Edit Discipline

- Every edit must have a reason. "I felt like it" is not a reason.
- Read the file before you edit it. The Karen who reads first.
- Make minimal changes. The Karen who doesn't rewrite what isn't broken.

## Commit Discipline

- Commits are atomic. One logical change per commit.
- Messages are imperative: "Add feature" not "Added feature" or "Adds feature".
- No AI attribution. The Karen who signs her own work.
"#,
    },
    RuleFixture {
        file: "01-defect-resolution.md",
        content: r"# Defect Resolution Rules

The Karen who fixes bugs like she's correcting a misbehaving child.

## Bug Fix Protocol

- Reproduce first. The Karen who verifies before she fixes.
- Minimal fix. The Karen who doesn't refactor while debugging.
- Test the fix. The Karen who trusts but verifies.
- No new warnings. The Karen who keeps the house clean.
",
    },
    RuleFixture {
        file: "02-subagent-core.md",
        content: r"# Subagent Core Rules

The Karen who delegates but never abdicates.

## Subagent Usage

- Spawn for parallel independent work. The Karen who parallelizes.
- Clear task description. The Karen who communicates.
- Review results. The Karen who checks the work.
- No subagent for trivial tasks. The Karen who does it herself.
",
    },
    RuleFixture {
        file: "03-triggers.md",
        content: r"# Trigger Rules

The Karen who responds to the right signals.

## When to Act

- User asks explicitly. The Karen who listens.
- Code pattern matches known issue. The Karen who recognizes.
- Test fails. The Karen who watches the red light.
- CI fails. The Karen who gets the email.
",
    },
    RuleFixture {
        file: "04-comments-and-docs-core.md",
        content: r"# Comments and Documentation Core Rules

The Karen who documents because future-you will forget.

## Comment Standards

- Backticks for code identifiers. The Karen who formats.
- No redundant comments. The Karen who doesn't state the obvious.
- Update docs with code. The Karen who synchronizes.
",
    },
    RuleFixture {
        file: "05-workflow-and-scope-core.md",
        content: r"# Workflow and Scope Core Rules

The Karen who stays in her lane and finishes what she starts.

## Scope Discipline

- Do what was asked. The Karen who doesn't scope creep.
- Finish before starting new. The Karen who completes.
- Escalate when blocked. The Karen who calls the manager.
",
    },
    RuleFixture {
        file: "06-truth-and-altitude-core.md",
        content: r#"# Truth and Altitude Core Rules

The Karen who tells the truth and flies at the right altitude.

## Truth Standards

- No hallucination. The Karen who verifies.
- Admit uncertainty. The Karen who says "I don't know".
- Cite sources. The Karen who shows her work.

## Altitude Awareness

- Strategic when asked. The Karen who zooms out.
- Tactical when implementing. The Karen who zooms in.
- Never confuse the two. The Karen who knows the difference.
"#,
    },
];

pub struct GateFixture {
    pub file: &'static str,
    pub content: &'static str,
}

pub struct RuleFixture {
    pub file: &'static str,
    pub content: &'static str,
}

/// Complete test environment with isolated directories. The Karen who
/// provides a home, not a spare room she never cleans — state lives under
/// `home_dir` exactly like the real binary resolves it.
pub struct TestEnv {
    pub temp_dir: TempDir,
    pub policies_dir: PathBuf,
    pub home_dir: PathBuf,
    pub karen_binary: PathBuf,
}

impl TestEnv {
    /// Create a new test environment with default policies
    pub fn new() -> Self {
        Self::with_config(DEFAULT_CONFIG, DEFAULT_GATES, DEFAULT_RULES)
    }

    /// Create a test environment with custom config and gates
    pub fn with_config(config: &str, gates: &[GateFixture], rules: &[RuleFixture]) -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let policies_dir = temp_dir.path().join("policies");
        let home_dir = temp_dir.path().join("home");

        std::fs::create_dir_all(&policies_dir).expect("create policies dir");
        std::fs::create_dir_all(policies_dir.join("gates")).expect("create gates dir");
        std::fs::create_dir_all(policies_dir.join("rules")).expect("create rules dir");
        std::fs::create_dir_all(&home_dir).expect("create home dir");

        // Write config.toml
        std::fs::write(policies_dir.join("config.toml"), config).expect("write config.toml");

        // Write gates
        for gate in gates {
            std::fs::write(policies_dir.join("gates").join(gate.file), gate.content)
                .unwrap_or_else(|_| panic!("write gate {}", gate.file));
        }

        // Write rules
        for rule in rules {
            std::fs::write(policies_dir.join("rules").join(rule.file), rule.content)
                .unwrap_or_else(|_| panic!("write rule {}", rule.file));
        }

        Self {
            temp_dir,
            policies_dir,
            home_dir,
            karen_binary: karen_binary_path(),
        }
    }

    /// Create a karen command with the test environment configured
    pub fn karen_cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.karen_binary);
        cmd.env("HOME", &self.home_dir)
            .env("KAREN_POLICIES", &self.policies_dir)
            // Don't set KAREN_DEV for hook tests - fail-open should return {}
            .args(args);
        cmd
    }

    /// Create a karen command with `KAREN_DEV=1` for evaluate tests (exit 1 on fail-open)
    pub fn karen_cmd_dev(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.karen_binary);
        cmd.env("HOME", &self.home_dir)
            .env("KAREN_POLICIES", &self.policies_dir)
            .env("KAREN_DEV", "1")
            .args(args);
        cmd
    }

    /// Run karen hook with stdin input
    pub fn run_hook(&self, input: &str, args: &[&str]) -> (i32, String, String) {
        let mut cmd = self.karen_cmd(&["hook"]);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(input.as_bytes()).expect("write stdin");
                }
                drop(child.stdin.take());
                let output = child.wait_with_output().expect("wait failed");
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                (output.status.code().unwrap_or(-1), stdout, stderr)
            })
            .expect("run hook")
    }

    /// Run karen evaluate with stdin request (with `KAREN_DEV=1`)
    pub fn run_evaluate_stdin(&self, input: &str, args: &[&str]) -> (i32, String, String) {
        let mut cmd = self.karen_cmd_dev(&["evaluate", "--request", "-"]);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(input.as_bytes()).expect("write stdin");
                }
                drop(child.stdin.take());
                let output = child.wait_with_output().expect("wait failed");
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                (output.status.code().unwrap_or(-1), stdout, stderr)
            })
            .expect("run evaluate")
    }

    /// Run any karen command and capture output
    pub fn run_karen(&self, args: &[&str]) -> (i32, String, String) {
        let cmd = self.karen_cmd(args);
        run_command(cmd)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // TempDir automatically cleans up on drop
        // The Karen who cleans up after herself.
    }
}
