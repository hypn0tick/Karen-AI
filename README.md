# Karen-AI

**Fast, policy-driven guardrails engine for AI coding runtimes.**

**The Karen who's watching everything agents do, looking for any reason to tell them they're wrong or call the manager.**

[![Build](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Runtime](https://img.shields.io/badge/runtime-Rust%201.85-orange)]()
[![Policy](https://img.shields.io/badge/policy-JSON%2FJSONC%2FJSON5-brightgreen)]()
[![Manager](https://img.shields.io/badge/manager-contacted-red)]()

> The "Karen" you'll invite to every party. This one judges a call against YOUR policy in under a millisecond, but a hook starts her fresh each time, so the honest stopwatch reads about 30 ms per call on Windows with 65 gates: process start, then compiling only the patterns that call actually needs (source: hyperfine, which she checked twice).

Karen-AI is a guardrails engine for AI coding agents. She watches every tool call your agent makes, reads the JSON you gave her, and says "no" before the harness even blinks.

She is not a linter. She is not a suggestion. She is the person at the front desk who has been here nineteen years and does not care that you are in a hurry.

The important part: **Karen-AI has no opinions of her own.** She ships the mechanism. You write the policy. She is, however, extremely opinionated about enforcing yours.

---

## Why Karen-AI?

Your agent is 90% brilliant. The other 10% is why Karen-AI exists. It will:

- run the one command you specifically said not to run
- reach for the wrong tool with total confidence
- spawn a subagent that has somehow never heard of your rules
- fix the bug by deleting the test
- apologize for a mistake while making it again in the same paragraph

Every runtime gives you a different hook API, a different timeout, and a different definition of "deny." You should not have to learn four dialects of "please don't."

Karen-AI exists so you can write the rules once, in files a human can read, have them delivered to every session, and have what you compile into gates enforced everywhere your agent goes. The Karen who follows you into other people's houses and rearranges their hooks.

---

## Quick Start

```bash
# Build
cargo build --release

# Your rules for every project (config.toml is optional — the defaults are sensible)
mkdir -p ~/.karen/policies/gates ~/.karen/policies/rules
# ...or for one project, from its root
mkdir -p .karen/policies/gates .karen/policies/rules

# Validate policies
./target/release/karen validate

# Check policy health (verbose — lists every gate and rule)
./target/release/karen check

# Dry-run an evaluation
./target/release/karen evaluate --event PreToolUse --tool Bash --command 'git commit -m "x"'

# Run as a hook (JSON in on stdin, protocol JSON out on stdout)
./target/release/karen hook --adapter claude-classic --part 1 < sessionstart-payload.json

# View schema (machine-readable)
./target/release/karen schema

# Install hooks for your runtime
./target/release/karen setup --dry-run
./target/release/karen setup
```

That's the whole tutorial. She's watching now. Sleep well.

---

## Installation (Claude Code / Codex / OpenCode)

```bash
# Install hooks globally (recommended)
karen setup

# Install for specific runtime only
karen setup claude
karen setup codex
karen setup opencode

# Project-level installation
karen setup --project

# Preview what would be installed
karen setup --dry-run
```

The `setup` command wires `karen hook` into each runtime where that runtime actually looks. Claude Code reads hooks only from its settings files, so she merges her entries into `settings.json`: everything that isn't hers stays put, an older copy of her entries is replaced rather than duplicated, and a `settings.json` that isn't valid JSON is refused, never overwritten. OpenCode runs no command hooks at all, so she installs a bridge plugin (`plugins/karen-ai.js`) that forwards tool calls and rule injection to the binary, and reviews every idle session as a Stop: when a Stop gate blocks, the reason goes back into the session as a prompt and the turn continues. A child session (one with a parent, from the task tool) is reviewed as a SubagentStop. A lane started in its own terminal is a top-level session, so it gets Stop too; scope gates to roles with conditions such as `cwd`. Each OpenCode Stop also carries the session's todo list (`open_todos`, `todo_count`, `todos_updated`) and `stop_hold_count`, the number of stops Karen has held in a row, reset when one passes: a gate with `none: [{"type": "field_equals", "field": "stop_hold_count", "value": 2}]` insists twice and then lets go, instead of exempting every continuation. The plugin passes the session's directory, and Karen finds the layers from it (`KAREN_POLICIES` in OpenCode's environment still overrides). Codex still gets its `hooks.json`. A global install goes where each runtime reads it, following the runtime's own override: `CLAUDE_CONFIG_DIR` (else `~/.claude`), `CODEX_HOME` (else `~/.codex`), `OPENCODE_CONFIG_DIR` (else `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`). Every hook command names its runtime (`--runtime claude`), so a gate on `facts.runtime` holds wherever those directories live. Every hook command also names a binary the runtime can find: run from her home (`~/.karen/bin`, or `$KAREN_HOME/bin`), she pins that absolute path, so replacing that one file upgrades every hook; run from anywhere else, she writes the bare `karen` when it is on `PATH`, and otherwise falls back to her home's default binary, so a hook never names a command its shell cannot find. A stale `hooks.json` from an earlier `setup` is removed from the Claude and OpenCode directories, but only when every command in it is hers. She brought her own ladder, and she put the neighbours' ladders back. You're welcome.

---

## Commands

| Command    | Purpose                                    | The Karen Who...                     |
| ---------- | ------------------------------------------ | ------------------------------------ |
| `hook`     | External hook: JSON on stdin (or from the file `--input PATH` names), protocol JSON on stdout. `--adapter` defaults to `claude-classic`; `--part N` delivers SessionStart rule part N | respects the protocol                |
| `validate` | Validate policy files, print summary       | reads the fine print                 |
| `check`    | Verbose policy health (lists every gate and rule) | leaves no stone unturned             |
| `evaluate` | Dry-run evaluation (never writes state)    | plays it safe                        |
| `schema`   | Machine-readable command & type vocabulary | documents herself                    |
| `completion` | Generate shell completions (bash, zsh, fish) | finishes your sentences, correctly  |
| `setup`    | Install hooks for AI runtimes              | sets up for you, because she doesn't want to have to call the manager already |

### Global Options

She offers you choices. All of them are technically yours. The Karen who believes in choice, as long as she approves each one.

- `--json` — machine-readable JSON envelope on stdout
- `--output text|json` — output format
- `--fields a,b,c` — filter top-level JSON fields
- `--quiet` — suppress human output
- `--dry-run` — preview changes without writing (setup only)

### Exit Codes

| Code | Meaning                          | The Karen Who...              |
| ---- | -------------------------------- | ----------------------------- |
| 0    | success                          | gets it done                  |
| 2    | usage / invalid input            | doesn't guess                 |
| 3    | file not found                   | checks first                  |
| 78   | invalid or missing policies      | insists on order              |

---

## Policy Model

```
policies/
├── config.toml          # engine config (timeouts, log level, hot reload) — The Karen who configures
├── rules/               # always-on markdown, injected as context — The Karen who force-feeds context before the agent can say "I didn't know"
└── gates/               # event-triggered JSON/JSONC/JSON5: match this, do that — The Karen who decides
```

(Skill routing and tool redirection directories are planned, not implemented — the loader reads `rules/` and `gates/` only. The Karen who refuses to document furniture that hasn't been delivered.)

A rule reaches every runtime unless it opens with a front matter block naming the runtimes it is for, as the `runtime` fact names them (`claude`, `codex`, `opencode`):

```markdown
---
runtimes: [claude, codex]
---
# Rules about Claude Code's own subagent hooks
```

The block is not injected, and any key other than `runtimes` fails `karen validate`. A rule only one runtime can follow costs every other runtime nothing.

She loads three layers on every call and merges them by gate id:

| Layer | Where | For |
|---|---|---|
| Built-in | compiled into the binary | the few gates nobody objects to: no force-push to `main`/`master`, no credentials in a command line |
| User | `~/.karen/policies` | your rules, in every project and every runtime |
| Project | `--policies`, else `$KAREN_POLICIES`, else the nearest `.karen/policies` from the working directory up to the repository root, else the repository's `karen.policies` git setting | one project's rules |

The project layer belongs to the repository, so every session in it finds the layer without wiring, whichever worktree it starts in: the nearest `.karen/policies` from the working directory up to the repository root (outside a repository, up to your home; outside both, in the working directory alone), else the repository's `karen.policies` setting (`git config karen.policies <path>`; relative to the main working tree), which every worktree shares. `karen validate` says which way it was found.

A later layer adds gates freely. To replace a gate from an earlier layer it says so with `"override": true`; reusing an id without it is refused, so nothing is shadowed by accident. To switch one off, its `config.toml` lists `[policy] disable = ["gate-id"]`. A gate marked `"locked": true` survives every later layer: a project that tries to replace or disable it gets refused, the gate stays enforced, and the session is told once. Refusals never fail the load, and neither does a broken file: a gate file that does not parse, a gate whose pattern does not compile, a config section with a typo or a rule with bad front matter is left out alone and named, and every other gate stays enforced. `karen validate` fails on all of them and reports every gate's layer and source file; `karen evaluate` names the layer of each gate that fired. Config keys merge the same way: a later layer wins per key, lists are unioned. Everything she keeps lives under `~/.karen` (`KAREN_HOME` moves it): `policies/`, `state/`, `cache/`, `logs/`, and `run/`, where the OpenCode bridge hands her each payload as an owner-only file, removed once she answers. The Karen who checks the street, the building and the house rules, in that order.

If an explicitly named project root does not exist, or a file in a layer does not load, `karen hook` still enforces every layer and file that did load, so a broken install never bricks your agent and never switches the rest off, but it says so: the response carries a `systemMessage` (`Karen-AI policy merge: …`) naming what is missing, once per session and again whenever it changes. The OpenCode plugin passes that on in the first tool result or, for a session that only idles, as a prompt; a gate's warning on a stop it allowed never wakes a session. A guard who is off duty tells you she is off duty. When the OpenCode plugin itself cannot reach her (the binary fails to start, times out or exits non-zero, a session read fails, or a hold prompt is not delivered), it still lets the session through and writes one line per failure to `$KAREN_LOG`, by default `~/.karen/logs/karen-opencode.log`, so a stop that was not held can be explained from the log.

---

## Writing a Gate

```json
{
  "id": "my-gate",
  "name": "Block force push on Friday",
  "event": "PreToolUse",
  "tool": "Bash",
  "priority": 90,
  "enabled": true,

  "match": {
    "all": [
      { "type": "command_regex", "pattern": "\\bgit(?:\\s+-{1,2}[^\\s=]+(?:=\\S*|\\s+[^-\\s]\\S*)?)*\\s+push\\b[^;&|\\n]*\\s(?:-[a-zA-Z]*f[a-zA-Z]*\\b|--force\\b|--mirror\\b|[\"']?\\+[^\\s+])" },
      { "type": "command_regex", "pattern": "\\bgit(?:\\s+-{1,2}[^\\s=]+(?:=\\S*|\\s+[^-\\s]\\S*)?)*\\s+push\\b[^;&|\\n]*\\s(?:[\"']?\\+?(?:[^\\s:\"';&|]*:)?(?:refs/heads/)?(?:main|master)[\"']?(?:[\\s;&|)]|$)|--mirror\\b|--all\\b)" }
    ]
  },

  "action": {
    "type": "deny",
    "reason": "It's Friday. Absolutely not."
  }
}
```

`tool` is an exact tool name, matched case-insensitively, or a regex over it (`"Edit|Write"`). It must compile as a regex either way: `karen validate` refuses a filter that does not, because a typo there leaves a gate that silently never applies.

The Karen who decides. Four answers: `allow`, `deny`, `ask`, `modify`. No `maybe`. A `maybe` was proposed at a design review. She asked for the manager. It did not survive. The Karen who buried `maybe` in the backyard. The manager attended the funeral.

### Matchers

| Type                | Description                          | The Karen Who...     |
| ------------------- | ------------------------------------ | -------------------- |
| `command_regex`     | Regex against full command string    | reads the whole line |
| `command_contains`  | Substring match on command           | spots the pattern    |
| `file_regex`        | Regex against file path              | checks the path      |
| `path_regex`        | Regex against path                   | watches the tree     |
| `prompt_regex`      | Regex against user prompt            | listens to you       |
| `content_regex`     | Regex against tool input content     | reads the payload    |
| `tool_regex`        | Regex against tool name              | knows the tool       |
| `field_equals`      | Exact match on `tool_input.<field>`, else the top-level payload field (Stop's `stop_hook_active`); `.` walks nested keys (`tool_response.stdout`) | demands precision    |
| `field_regex`       | Regex against the same field, strings only (Stop's `last_assistant_message`, PostToolUse's `tool_response.stdout`) | reads your closing statement |
| `field_json_regex`  | Regex against any field: a string as itself, anything else as compact JSON (Stop's `background_tasks`: `"status":"running"`) | reads the fine print too |
| `file_older_than`   | The file at `path` is missing or was last modified more than `seconds` ago (`~/` is home; relative paths resolve against `cwd`), so a marker the finished chore touches makes a gate "due again" | checks when you last did it |
| `always`            | Matches everything                   | doesn't miss a thing |

### Environment facts

A gate can read what the session's runtime actually has instead of someone remembering it, through `field_equals` or `field_regex` on `facts.<name>`. She collects a fact only when a gate that applies to the current event names it, and a fact the runtime already put in the payload wins.

| Fact | Value | Source |
|------|-------|--------|
| `facts.runtime` | `claude`, `codex`, `opencode` or `unknown` | the hook command (`karen setup` writes `--runtime claude` or `--runtime codex` into every one); the OpenCode plugin says so in the payload; otherwise the transcript path, under `CLAUDE_CONFIG_DIR` or `CODEX_HOME` when they are set |
| `facts.mcp_servers` | comma-separated names of the enabled MCP servers | the runtime's own config files (Claude Code: `.claude.json` in `CLAUDE_CONFIG_DIR`, else `~/.claude.json`, the project's `.mcp.json`, and the plugins its settings enable; Codex: `config.toml` in `CODEX_HOME`, else `~/.codex`; OpenCode: `opencode.json` or `opencode.jsonc` in its global directory (`$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`), the project, `OPENCODE_CONFIG` and `OPENCODE_CONFIG_DIR`). Names only: server definitions can hold keys, and a gate never needs them |
| `facts.tools` | comma-separated names from `[facts] tools` found on `PATH` | `PATH` |
| `facts.git_uncommitted` | `git status --porcelain` for `cwd`, first 50 lines | git |

`runtime`, `mcp_servers` and `tools` are cached for the session; `git_uncommitted` is read on every call that needs it. A gate that should only exist where a server does, for example:

```json
{ "type": "field_regex", "field": "facts.mcp_servers", "pattern": "(^|,)brave-search(,|$)" }
```

### Actions

| Type                   | Meaning                  | The Karen Who...                             |
| ---------------------- | ------------------------ | -------------------------------------------- |
| `allow`                | Proceed                  | steps aside                                  |
| `deny`                 | Block with reason        | says no, and has the policy to back it up    |
| `ask`                  | Ask human                | escalates, because you need adult supervision |
| `modify`               | Rewrite command          | gets fed up and just does it for you         |
| `deny_once_then_warn`  | First deny, then allow on retry | teaches once                            |

### State (deny_once_then_warn)

```json
{
  "action": {
    "type": "deny_once_then_warn",
    "first_reason": "Use yq for YAML/TOML",
    "second_reason": "Allowing jq on retry"
  },
  "state": {
    "track": "session",
    "key": "jq-yaml",
    "max_strikes": 2
  }
}
```

The Karen who remembers. Strike one: denied, with an explanation and a lecture. Strike two: allowed, with the shorter explanation and a nudge toward the right tool — she has made her point, and repeating herself is what the manager is for. Strike three: there is no third strike. She has already moved on. (She already called your manager.) Add `"reset_on_compact": true` to the state and a compaction wipes the slate for that key: a nudge that taught something the compaction then dropped gets to teach it once more, so "once per session" becomes "once per context". `"track"` says what the count belongs to: `"session"` (the default) teaches once per session, and `"input"` counts each distinct tool input on its own, so every new command is held once and the same command retried goes through with the warning, which suits a check the agent has to confirm per call (a heredoc full of backslashes) rather than learn once. An input-tracked count is not wiped by `reset_on_compact`.

**Stop gates that hold more than once.** A held Stop comes back with `stop_hook_active: true` on every later Stop of that turn (Claude Code sets it natively; the OpenCode bridge sets it the same way), so a Stop gate whose `none` exempts `stop_hook_active` holds a session exactly once and lets every later idle through. To keep holding until the agent states what the gate wants, leave that exemption out and bound the loop with the gate's own state: `deny_once_then_warn` with `max_strikes` set to the number of holds you allow, plus `reset_on_compact` to refill the budget after a compaction. The Karen who keeps asking, counts how often she asked, and knows when asking again stops helping.

---

## Gate File Formats

Gates can be written in **JSON**, **JSONC**, or **JSON5** — all three extensions go through the same engine (via the `json5` crate). The table below describes the strict standards; she is more permissive than every one of them, on purpose. The Karen who reads the spec for fun and then makes exceptions, loudly.

| Feature                 | JSON     | JSONC           | JSON5        |
| ----------------------- | -------- | --------------- | ------------ |
| Standard compliance     | RFC 8259 | JSON + comments | ES5 superset |
| Comments (`//`, `/* */`)    | ❌       | ✅              | ✅           |
| Trailing commas         | ❌       | ✅              | ✅           |
| Unquoted keys           | ❌       | ❌              | ✅           |
| Single-quoted strings   | ❌       | ❌              | ✅           |
| Multi-line strings      | ❌       | ❌              | ✅           |
| Hex numbers / NaN / Infinity | ❌    | ❌              | ✅           |
| File extensions         | `.json`    | `.jsonc`          | `.json5`       |

**Recommendation**: Pick one extension and stay consistent — this repo ships its gates as `.json`. Because every extension is parsed by the same engine, JSONC niceties (comments, trailing commas) work in `.json` files too. The Karen who prefers clarity over cleverness, and one drawer for all the scissors.

**Note**: All three extensions are parsed identically — and every example in [`examples/`](examples/) ships as `.json` *with comments inside*, so the permissiveness claim above ships fifteen working proofs of itself, in the house extension.

---

## Known Limitations

### `modifiedInput` Partially Supported in Pure-Hook Adapters

When a gate uses `action.type: modify`, the engine correctly computes the modified input and returns it in the `modified_input` field. The `karen hook` adapter includes `modifiedInput` in its protocol response. However, **the Claude Code hook protocol's handling of `modifiedInput` is inconsistent** — some runtimes may ignore it.

| Command                              | Works? |
| ------------------------------------ | ------ |
| `karen evaluate`                     | ✅ Full `modified_input` in response |
| `karen hook --adapter claude-classic` | ⚠️ `modifiedInput` sent in response, but runtime may not act on it |

The adapter includes the modification in the protocol response. Whether the runtime actually applies it depends on the runtime implementation. This is a runtime protocol limitation, not an engine limitation. She delivers the correction; what the runtime does with it is between the runtime and its upbringing.

**Workaround:** Use `deny_once_then_warn` with a clear reason explaining what the user should do instead. The agent will see the reason and can retry with the corrected command.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│  RUNTIME LAYER                              │
│  Claude (native) │ Codex* │ OpenCode*        │
└─────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│  ADAPTER LAYER  ("no, not like that")       │
│  The Karen who translates                   │
└─────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│  Karen-AI                                    │
│  • reads your JSON/JSONC/JSON5              │
│  • compiles regex when a call needs it      │
│  • tracks strikes per session               │
│  • keeps everything in ~/.karen             │
│  The Karen who decides / The Karen who remembers / The Karen who has the receipts │
└─────────────────────────────────────────────┘
```

\*Codex: an install target speaking Classic through `hooks.json`. OpenCode: a bridge plugin speaking Classic to the engine (plugin API v1; Stop and SubagentStop come from session-idle events; blocking prompt gates are not bridged, since that API has no blocking prompt hook, and `ask` acts as `deny` because a plugin cannot ask; a held Stop is sent back as a prompt on the session's current model). Native adapters live on the [ROADMAP](ROADMAP.md). The asterisk is doing the same work it does on a food label: honesty about what's inside.

New runtimes get an adapter. New rules get a file. Neither requires a meeting.

---

## Adapters

Hook adapters translate Karen's verdicts into each runtime's protocol. Currently implemented:

| Adapter          | Runtimes                 | The Karen Who...               |
| ---------------- | ------------------------ | ------------------------------ |
| `claude`         | Claude Code (current)    | stays current                  |
| `claude-classic` | Claude Code (classic protocol) | delivers verdicts only — your tools are safe, your agent is not |

`setup` also accepts `codex` and `opencode` as install targets — Codex gets a `hooks.json`, OpenCode gets the `karen-ai.js` bridge plugin, both speaking the Classic protocol. Native hook adapters for those runtimes are planned; until then, `--adapter` rejects them rather than guessing. The Karen who won't pretend to speak a language she hasn't learned.

---

## Configuration (`policies/config.toml`)

```toml
[engine]
fail_open = true          # The Karen who forgives crashes but writes it in your permanent record
log_level = "warn"        # The Karen who doesn't chatter
hot_reload = true         # Live reload, proven: embedders watch the root and swap law mid-flight (Engine::watch_policies); the CLI re-reads every call. The Karen who changes her mind before you finish saving it
timeout_ms = 5000         # The Karen with a deadline

[policy]
validate_on_load = true   # The Karen who checks first
rules_dir = "rules"
gates_dir = "gates"

[state]
backend = "auto"          # auto | memory | file
# file_dir = "~/.karen/state"  # the default; The Karen who remembers
```

---

## Example Policies

The desk still arrives empty — `policies/` is local-only (`DO NOT COMMIT`), the crate ships no gates, and what she enforces is entirely what you compile. But the repo ships the curriculum: [`examples/`](examples/) — fifteen heavily-commented gates (twelve fundamentals, three advanced merges), a minimal annotated `config.toml`, and one example rule. Copy what you like into your `policies/`, adjust, and `karen validate`. She doesn't ship opinions; she ships her lectures, which are yours to assign.

The full syntax lives in [Writing a Gate](#writing-a-gate) above, and every category in "What people encode" below has working files in `examples/gates/`. The Karen who ships the desk, the curriculum, and not one opinion about your house.

### What people encode

Personality enforcement is the flagship demo — every comment in this repo is a live specimen. But a gate is just *match what the agent is doing → deliver a verdict → remember it*, and that grammar composes further than comedy:

- **Evidence before claims** — a claim statement that didn't do its homework gets denied and lectured until it brings receipts
- **Premises before conclusions** — conclusions that skip their work get sent back to show it
- **Current facts require a current search** — the web search comes first, or the claim comes back
- **The right tool before the wrong one** — tool redirects that teach exactly once, then watch the retry
- **Workflow discipline** — no commit without tests, no force-push to main, no fixing the bug by deleting the test

Each of those is about thirty seconds of gate-writing. If you can express the standard, she holds the line on it — per tool call, with receipts, forever. The personality is the demo; the enforcement is the product; what it enforces is your department.

---

## JSON Envelope

All JSON-output commands return a structured envelope. The Karen who follows protocol.

```json
// Success
{
  "status": "ok",
  "data": { ... },
  "warnings": []
}

// Error
{
  "status": "error",
  "error": {
    "code": "POLICY_NOT_FOUND",
    "message": "policy root missing",
    "fix": "Point --policies at a directory containing config.toml, rules/, and gates/.",
    "transient": false
  }
}
```

---

## Schema Introspection

```bash
# All commands
karen schema --json

# One command
karen schema evaluate --json

# Output includes: commands, types (events, conditions, actions), exit codes, invariants
```

The Karen who documents herself.

---

## Testing

```bash
cargo test                  # unit + integration + e2e tests; builds the binary they exec
cargo test --test test_e2e  # e2e lane only
cargo test --test integration # integration tests only
```

203 tests passing (27 unit, 20 integration, 156 e2e — zero ignored). The Karen
who verifies, then verifies the verification.

---

## Testimonials

> "I asked her to let one command through. Just once. She printed the policy, the line number, and my session ID."
> — a user who asked

> "I wrote a gate, changed the reason string, and it took effect before I finished typing. I have never been corrected so efficiently."
> — a user with a text editor

> "My agent tried the same forbidden command under a different alias. Karen had already accounted for that."
> — a user who was incorrect about that

> "The engine starts faster than the runtime can decide it needs the engine."
> — a benchmark, probably

> "I wrote four runtimes' worth of hooks in an afternoon and only two of them were my fault."
> — an adapter author, on their third afternoon

---

## Project Layout

```
src/
├── main.rs              # CLI entry point — The Karen who starts it all
├── lib.rs               # library exports — The Karen who exports
├── adapters/            # runtime adapters — The Karen who translates
├── engine/              # dispatcher, injector, maintenance — The Karen who runs it
├── policy/              # loader, gate compiler, config — The Karen who loads
├── state/               # FileStore, MemoryStore, ReadOnlyStore — The Karen who remembers, and forgets on request
└── types.rs             # shared types — The Karen who defines
ROADMAP.md               # audited direction — The Karen who approves, never writes, because she'd rather judge you than work on it herself
policies/                # your policies (NOT git-tracked — see .gitignore) — The Karen who enforces your rules, not your commit history
target/release/karen     # binary — The Karen you run
```

---

## License

The Karen License. These are MIT's operative terms wearing a name tag: do whatever you want, just keep the copyright notice on the receipts. She read the whole thing twice, corrected its grammar, and would like a word with the manager of your legal department.

See [LICENSE](LICENSE).

---

*karen does not negotiate with agents. She otherwise gets along with them splendidly.*

---

*The Karen who doesn't forget. She has a file. It's thick.*