# Karen-AI

**Fast, policy-driven guardrails engine for AI coding runtimes.**

**The Karen who's watching everything agents do, looking for any reason to tell them they're wrong or call the manager.**

[![Build](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Runtime](https://img.shields.io/badge/runtime-Rust%201.85-orange)]()
[![Policy](https://img.shields.io/badge/policy-JSON%2FJSONC%2FJSON5-brightgreen)]()
[![Manager](https://img.shields.io/badge/manager-contacted-red)]()

> The "Karen" you'll invite to every party. This one enforces YOUR policy in under half a millisecond (source: her own stopwatch, which has never once been wrong).

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

# Create your policy root (config.toml is optional — the defaults are sensible)
mkdir -p policies/gates policies/rules

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

The `setup` command wires `karen hook` into each runtime where that runtime actually looks. Claude Code reads hooks only from its settings files, so she merges her entries into `settings.json`: everything that isn't hers stays put, an older copy of her entries is replaced rather than duplicated, and a `settings.json` that isn't valid JSON is refused, never overwritten. OpenCode runs no command hooks at all, so she installs a bridge plugin (`plugins/karen-ai.js`) that forwards tool calls and rule injection to the binary. Codex still gets its `hooks.json`. A stale `hooks.json` from an earlier `setup` is removed from the Claude and OpenCode directories, but only when every command in it is hers. She brought her own ladder, and she put the neighbours' ladders back. You're welcome.

---

## Commands

| Command    | Purpose                                    | The Karen Who...                     |
| ---------- | ------------------------------------------ | ------------------------------------ |
| `hook`     | External hook: JSON on stdin, protocol JSON on stdout. `--adapter` defaults to `claude-classic`; `--part N` delivers SessionStart rule part N | respects the protocol                |
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

She finds your policy root in this order: `$KAREN_POLICIES` if set, then `./policies` in your working directory, then `$CLAUDE_PLUGIN_ROOT/policies`, then any `policies/` above the binary. Where you're standing outranks where she's installed. The Karen who checks the street before the building — your project is the street.

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
      { "type": "command_regex", "pattern": "git\\s+push\\b.*(--force|-f)\\b" },
      { "type": "command_regex", "pattern": "\\b(main|master)\\b" }
    ]
  },

  "action": {
    "type": "deny",
    "reason": "It's Friday. Absolutely not."
  }
}
```

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
| `field_equals`      | Exact match on `tool_input.<field>`, else the top-level payload field (Stop's `stop_hook_active`) | demands precision    |
| `always`            | Matches everything                   | doesn't miss a thing |

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

The Karen who remembers. Strike one: denied, with an explanation and a lecture. Strike two: allowed, with the shorter explanation and a nudge toward the right tool — she has made her point, and repeating herself is what the manager is for. Strike three: there is no third strike. She has already moved on. (She already called your manager.)

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
│  • compiles regex once, runs forever        │
│  • tracks strikes per session               │
│  • writes to ~/.cache/karen                 │
│  The Karen who decides / The Karen who remembers / The Karen who has the receipts │
└─────────────────────────────────────────────┘
```

\*Codex: an install target speaking Classic through `hooks.json`. OpenCode: a bridge plugin speaking Classic to the engine (plugin API v1; blocking prompt gates are not bridged, since that API has no blocking prompt hook). Native adapters live on the [ROADMAP](ROADMAP.md). The asterisk is doing the same work it does on a food label: honesty about what's inside.

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
file_dir = "~/.cache/karen"  # The Karen who remembers
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
cargo build --release        # e2e and integration tests exec this binary
cargo test                  # unit + integration + e2e tests
cargo test --test test_e2e  # e2e lane only
cargo test --test integration # integration tests only
```

151 tests passing (14 unit, 17 integration, 120 e2e — zero ignored). The Karen
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