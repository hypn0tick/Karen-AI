# Runtime control

Karen is a separate process. A runtime's hook API lets her answer a tool call; it does not let her type. Claude Code's hooks have no output field that starts a slash command, so `/compact` cannot be asked of the agent by a hook, only typed at it. `runtime_command` is the action that types, and this page is the reference behind the [README's summary](../README.md#runtime-commands): what a gate writes, every rule between the gate and the keyboard, every status an attempt can end as, what the log keeps, and what `karen validate` tells you before any of it matters.

The rule under all of it: **a gate that types into a session needs the owner's permission in writing, and every attempt leaves a line in a ledger, delivered or not.**

## The action

```json
{
  "id": "compact-when-full",
  "description": "Ask the session to compact before the window fills.",
  "event": "Stop",
  "match": { "all": [ { "type": "field_gte", "field": "facts.context_ratio", "value": 0.85 } ] },
  "action": {
    "type": "runtime_command",
    "command": "/compact",
    "arguments": "keep the open items and the shas"
  },
  "state": { "key": "latch:compact", "field": "facts.context_ratio", "rearm_below": 0.5 }
}
```

`command` is one word beginning with `/`. `arguments` is optional text typed after it. The gate fires like any gate (events, matchers, latches), and the call it fired on proceeds untouched: a typed command is a side effect, not a verdict, and it never holds a tool call. Put a latch on a gate that types, or it types on every call that matches; the latch is the same `field` and `rearm_below` pair every other action uses.

## The owner's permission

Two lists in the **user layer's** `config.toml` (`~/.karen/policies/config.toml`) decide what a `runtime_command` may do:

```toml
[actions]
runtime_commands = ["/compact", "/dcp-compress"]   # default: none
runtimes = ["claude", "codex", "opencode"]         # default: all three
```

Both are read from the user layer alone. A project layer is repository content, and a repository that could name a command or a runtime would hand its own gates a keyboard the owner never lent; a project layer that sets either list has it ignored, and `karen validate` names the layer. This is the same stand `[facts] env` takes, for the same reason.

An entry in `runtime_commands` that is not one word beginning with `/`, or an entry in `runtimes` that is not `claude`, `codex` or `opencode`, is dropped and named by validate. `runtimes` scopes what validate reports and nothing else: a delivery is attempted on any runtime that has a layer, so a forgotten entry never switches a reachable runtime off. An unset `runtime_commands` is the empty list, not "anything": absence is not a permission.

## What can stop an attempt, in the order she checks

| Status | Meaning |
|--------|---------|
| `refused:not-user-layer` | The gate is not the user layer's. It is ignored, every time, and `karen validate` names it. |
| `refused:not-allowlisted` | The command is not in `[actions] runtime_commands`. |
| `undelivered:unsafe-text` | The command or the rendered arguments hold a control character, or the whole line runs past 1000 characters. |
| `undelivered:no-layer` | Allowed, and nothing reaches this session: no layer exists for its runtime, or the caller did not offer the one that does. |
| `undelivered:host-unreachable` | The session's environment names a terminal host (`karen run`) for its runtime, and it did not answer in three seconds, or could not be reached. The `layer` field names the one that was tried. |
| `undelivered:host-refused` | The host answered, and would not take the line: its queue was full, or it was not a line it will type. |
| `handed-off` | Allowed, and a layer took it. The `layer` field names it. The layer delivers after her process has exited, so this is the last thing she can know. |

`refused` means a rule of hers stopped it. `undelivered` means it was allowed and did not arrive. `handed-off` means a layer has it, and the layer files the rest: the OpenCode bridge writes `sent`, `sent-unconfirmed` and its own `undelivered:*` statuses (below), and the terminal host writes `sent`, `undelivered:timeout` (no idle moment came in time), `undelivered:host-exited` (the runtime ended with the line still waiting) and `undelivered:type-failed` (the terminal would not take it). Every one leaves a log line, and none is ever silent.

## Arguments

`arguments` renders `{gate}` and `{facts.<path>}` the way a reason does: a string as itself, anything else as its compact JSON, an absent fact as `(unknown)`. It renders nothing else:

- `{command}` and `{tool}` stay literal. They are the agent's own words, and an agent should not be able to write the prompt a gate types at it. `karen validate` names a gate that templates them.
- `{facts.env.*}` stays literal, as it does in every reason: the allowlist granted a match, not a broadcast. `karen validate` names a gate that templates one.

What is typed must be one printable line. A newline typed into a prompt submits it and what follows is a second prompt; an escape sequence is a keystroke, not text. So a control character anywhere in the command or the rendered arguments is `undelivered:unsafe-text`, as is a line past 1000 characters (a prompt line a person could have typed). `karen validate` finds a literal control character in a gate's own arguments before the gate runs.

## The runtime log

Every attempt writes one line to `<Karen home>/logs/karen-runtime.jsonl`, rotated to `.jsonl.1` past 10 MiB like the notify log:

```json
{"time":1762300000000,"session":"abc","gate":"compact-when-full","runtime":"claude","command":"/compact","arguments":"keep the open items and the shas","layer":null,"status":"undelivered:no-layer"}
```

`layer` is the layer that took the command, `null` when none did. A layer that delivers on its own, after her process has exited, appends its own lines to the same log: they carry the same `session`, `gate` and `command`, plus a `route` (the call it made) and the `status` that call came to. `arguments` is the gate's own text with its placeholders unexpanded: what a session's facts rendered into the arguments is data about the session, and the ledger does not keep it. The same record rides the verdict: `karen evaluate --json` lists it under `runtime_commands`. A rehearsal decides the same status and files nothing, so a dry run does not write a line.

## What `karen validate` says

`karen validate` reports, as warnings, a gate that cannot do what it says: one outside the user layer, one whose command is off the allowlist, one whose arguments cannot be typed or template the agent's own text, and one whose runtime has no layer. It also lists the layers that can reach each configured runtime, in the text report whenever an enabled `runtime_command` gate exists and always in `--json` as `runtime_layers`. A gate switched off types nothing and gets no warning, so a starter pack can ship one that stays quiet until somebody turns it on.

## Which layer each runtime gets

A command reaches a session through whichever layer that session has: the runtime's own plugin API, or a terminal Karen can write to. A runtime with no layer ends every allowed attempt as `undelivered:no-layer`, and in the lean build `karen validate` names it. Set `[actions] runtimes` to the runtimes you can actually reach and the report goes quiet for the rest; the list never stops a delivery on a runtime that has a layer.

A layer that exists is not a layer that is there: `karen validate` prints what each one needs, and a session that does not have it ends `undelivered:no-layer` like any other. When a runtime has two, the first one the session has wins, so OpenCode's own API beats the terminal.

| Runtime | Layer | Reaches it when |
|---------|-------|-----------------|
| Claude Code | `terminal-host` | the session was started with `karen run claude` |
| Codex | `terminal-host` | the session was started with `karen run codex` |
| OpenCode | `opencode-native`, then `terminal-host` | the bridge plugin is installed and current (`karen setup opencode`); or the session was started with `karen run opencode` |

## OpenCode: the native layer

OpenCode runs its plugins inside its own process, so the bridge is the one that types, through the server API the runtime already exposes. The bridge offers the layer in every payload it sends (`runtime_layers`). Karen answers a `runtime_command` with the rendered line in `karenRuntimeCommands`; the bridge removes it before any hook reads the verdict, and makes one call once the session is idle:

| Command | Call | Notes |
|---------|------|-------|
| `/compact` (or `/summarize`) | `session.summarize`, on the session's current provider and model | The TUI's own `/compact` calls the same endpoint, so it needs no TUI. It takes no instructions, so the arguments are not carried. A session whose model cannot be named is `undelivered:no-model`, never a guess. |
| a custom command OpenCode lists (`/ship-it`) | `session.command` with the command name and the rendered arguments | Whatever `GET /command` lists on the server. |
| `/new`, `/share` | `tui.executeCommand` (`session.new`, `session.share`) | The server answers `true` whether or not a TUI is listening, and nothing a plugin can read says whether one is. So the ledger records `sent-unconfirmed`, never `sent`, and under a headless server (`opencode serve`) the command is a request nobody picks up. |
| anything else | nothing | `undelivered:unknown-command`. |

The calls are not awaited: a compaction or a custom prompt runs for as long as it runs. `sent` means the request was issued. A server that refuses it answers with an error rather than throwing one, and the bridge reads that answer where it arrives and files `undelivered:error`. The bridge's status vocabulary: `sent`, `sent-unconfirmed`, `undelivered:error`, `undelivered:no-model`, `undelivered:unknown-command`, `undelivered:timeout` and `undelivered:session-deleted`.

**Timing.** A command is typed when the session is idle, never into the middle of a turn. One handed back while a turn runs (a tool call, a prompt, a `busy` status) waits for the session's next idle and is typed ahead of that idle's Stop review. Each command starts a turn of its own, so a second one waits for the next idle after that. A command that gets no idle within two minutes is filed `undelivered:timeout` (`KAREN_COMMAND_WAIT_MS` changes the wait, for tests), and a session deleted with commands waiting files them `undelivered:session-deleted`.

**An older bridge.** A bridge installed before the layer existed offers nothing, so a command for its session is `undelivered:no-layer` rather than a hand-off nothing would pick up. Re-run `karen setup opencode`.

## `karen run`: the terminal host

Claude Code and Codex cannot be handed a slash command by a hook, so a command reaches them the way yours does: typed into their terminal. `karen run <runtime> [args...]` starts the runtime in a pseudo-terminal Karen owns (ConPTY on Windows, a pty elsewhere), draws a copy of its screen, and serves a small local endpoint that the runtime's own hook processes can find.

```
karen run claude --resume
karen run codex
karen run opencode
```

Everything after the runtime goes to the runtime untouched. `<runtime>` is any program on `PATH` (on Windows a `.cmd` shim is started through `cmd.exe`); `claude`, `codex` and `opencode` are just the ones she expects. She exits with the runtime's own exit code. Her own failures are `127` (not on `PATH`, as a shell says it) and `70` (anything else), with the reason on stderr and nothing on stdout, because stdout belongs to the runtime.

**What the hosted runtime inherits.** Four variables, so any process under it (its hooks, its shells) can find the host:

| Variable | Holds |
|----------|-------|
| `KAREN_HOST_ENDPOINT` | the endpoint's name: a named pipe on Windows, a socket path on Unix |
| `KAREN_HOST_TOKEN` | the password for it, new for every `karen run` |
| `KAREN_HOST_RUNTIME` | the runtime's name (the program's file stem, lowercased) |
| `KAREN_HOST_PID` | the pid of the `karen run` process |

**The endpoint.** One connection is one exchange: the token on a line, one JSON request on the next, one JSON answer back. Three requests exist:

| Request | Answer |
|---------|--------|
| `{"op":"status"}` | the runtime, both pids, the terminal size, and how many milliseconds since the runtime last drew anything |
| `{"op":"screen"}` | the visible screen as plain text, the size, the cursor and whether the runtime is on the alternate screen |
| `{"op":"type","text":"/compact"}` | the line typed, then Enter, now |
| `{"op":"deliver","text":"/compact","meta":{...}}` | the line's place in the queue (1 is next), at once; see below |

`type` accepts one printable line: no control character, no line break, at most 1000 characters. An escape sequence is a keystroke she was not asked to make, and in testing a cooked-mode terminal ate an injected cursor-up. The Enter follows the text after a short pause (150 ms), because a TUI that treats a burst of input as a paste would otherwise take the Enter for part of the paste and never submit. That pause is a guess about how those TUIs behave, and it is not verified against a real Claude Code or Codex; the tests type into the fake runtime only.

`type` types whatever the runtime is doing: it is the primitive, and nothing in a gate's path uses it. `deliver` is the one a gate's `runtime_command` reaches, and it waits for the session.

Screen text is the host's data: it is never logged and never rendered into a gate's text. A gate can still match on it: `facts.screen.text` for a `field_regex`, and the named fields `prompt_empty`, `turn_running`, `context_used_percent` and `usage_limit`, which read the host's screen through the `screen` request each time a hook asks for one (see [`session-facts.md`](session-facts.md)).

**Delivery: waiting for an idle session.** `deliver` takes the line and answers at once, because the `karen hook` process that asked is about to exit and must not hold up a tool call. The host keeps the line in a queue (first in, first out, at most 16) and types it when the session is idle, one per idle. Idle means three things at once:

- the runtime has drawn nothing for `quiet_ms` (400 ms by default; a spinner redraws constantly, so a running turn is never quiet);
- the prompt is on the screen and empty. A half-typed prompt is busy: the line would be appended to what the owner was writing, and then submitted for them;
- the runtime's own "press Esc to interrupt" line is not near the prompt.

Nothing is guessed in the idle direction: a screen with no prompt she recognises is busy, so a wrong rule makes a command time out and say so, and never types into a turn. A line that waits `timeout_secs` (120 by default) is given up on as `undelivered:timeout` and never typed. When the runtime exits with lines still waiting, each is filed `undelivered:host-exited`. The host files all of these itself, in the runtime log, with `layer` `terminal-host`, `route` `karen run`, `status` and `waited_ms`, and it keeps the arguments as the gate wrote them, as the rest of the log does.

How far the prompt rules are checked, runtime by runtime:

- **OpenCode**: recorded from a live `opencode` (a free model, in a throwaway home) hosted by `karen run`: the first screen, two characters typed, mid-turn and after a turn (`tests/fixtures/screens/opencode-*.txt`). Its composer is not a `>` row but a box of `┃` rows closed by a `╹▀▀▀` rule, with a placeholder hint ("Ask anything…") on a fresh session, and its turn marker is `esc interrupt`. Delivery was tried against that live session too: with two lines queued back to back, the first was typed at once (169 ms) and the second waited 7.4 s, through the first turn, and was typed only after OpenCode went idle; both were submitted and answered.
- **Claude Code**: one screen at rest, recorded read-only from a live session (`claude-idle.txt`). The mid-turn and half-typed screens are not recorded, so those two rules follow how Claude Code describes itself.
- **Codex**: no recording; the rules follow how Codex describes itself. This is the least checked.
- **Any other program** gets the plain rule (a `>` prompt on the last row), which the fake runtime's recorded screens check.

A dim suggestion sitting in an otherwise empty prompt reads as typed text for Claude Code and Codex, so it counts as busy: the command times out rather than risk it. OpenCode's own hint is recognised and is not typed text.

**Recording a screen yourself.** `cargo run --example record-screens -- --runtime <name> --out DIR -- <program> [args]` hosts any program under `karen run` in a throwaway home (so it starts with none of your configuration; `--env KEY=VALUE` passes it what it needs), types two characters, submits a prompt, and saves the idle, half-typed, mid-turn and after-turn screens and every distinct screen in between. `--deliver` sends the prompt and a second one through the host's queue instead and prints what the host filed. Read the screens before keeping any: a path, a name or an account can be on them.

**Hook side.** The host never decides what may be typed. The hook that calls it has already applied every rule above (user layer, allowlist, one printable line) and only then opens the endpoint, and a test pins that a refused attempt sends the endpoint nothing. The host is reached only when the hook's own environment names one (`KAREN_HOST_ENDPOINT` and `KAREN_HOST_TOKEN`) and `KAREN_HOST_RUNTIME` equals the session's runtime, so a Claude Code hook running under a hosted Codex does not type into Codex. `karen run --as claude ./wrapper.sh` records a program under the runtime's name when its file name is not it.

**Timings.** `[host]` in the user layer's `config.toml` (a project layer's is ignored and named):

| Key | Default | Range | What it does |
|-----|---------|-------|--------------|
| `enter_delay_ms` | 150 | 0 to 5000 | The pause between the typed text and its Enter, so a terminal UI that treats a burst of input as a paste does not take the Enter for part of it. The default is a guess. A measured value belongs here. |
| `quiet_ms` | 400 | 50 to 10000 | How long the runtime must have drawn nothing to count as idle. |
| `timeout_secs` | 120 | 1 to 3600 | How long a line waits for an idle session. |

A value outside its range is named by `karen validate` and the default stands.

**What the token is, and is not.** It keeps strangers off the endpoint: anything that is not a descendant of the hosted runtime does not know it, and the socket sits in a directory only its owner can enter (a Windows pipe's default access lets other users read from it but not write to it, and she does not lean on that). It is not a boundary against the hosted tree itself. The runtime and everything it starts share its terminal, and on Windows any of them can write to the console's input without Karen at all. So the rules that matter are the ones upstream of the endpoint: `runtime_command` is user-layer only, allowlisted, and one printable line, and the endpoint enforces the line rule again on its own. A host does not read the owner's allowlist, and she does not pretend otherwise.

**What is promised about bytes.** Input is promised byte-exact: what the real terminal sends goes to the runtime as it arrived. Output is not. Under ConPTY the runtime's output is ConPTY's repaint of its screen (line clears, mode toggles, a window title), and she forwards it as produced and draws it on her own copy. Some of that, `?9001h` (win32-input-mode) among it, changes what a terminal sends back; forwarding it is the correct thing for ConPTY to ask, and interactive passthrough is not yet checked by hand in Windows Terminal or conhost. Piped input is what the tests cover.

**Platform notes.**

- ConPTY asks for the cursor position before it starts the child, and starts nothing until it is answered. She answers at once, and again from the screen's cursor whenever it asks. Without both the child never starts, which the banner test pins down.
- Raw mode: while she hosts, the real terminal is put in raw mode (Windows: virtual-terminal input only, and virtual-terminal processing on the output) and put back when she leaves, including after a panic and on Ctrl-Break or a closing console. A hard kill (`taskkill /f`, `kill -9`) runs neither, and leaves the terminal raw: `reset` on Unix, or a new console on Windows, fixes it. When stdin is not a terminal, as in a pipe or a test, nothing is changed.
- The console code is the only `unsafe` in the crate (Windows, a handful of `SetConsoleMode`-family calls, each with its `SAFETY` comment). The Unix side is safe Rust.
- It follows the real terminal's size by looking every quarter second, not by signal.

**The lean build.** `karen run` and everything behind it is the default-on cargo feature `host`. `cargo build --no-default-features` leaves hooks, gates and the rest, and about two dozen fewer crates on Windows; there, `karen run` says so and exits `70`.

## How this is tested

The attempts are tested end to end against the real binary: a user layer and a project layer in a temporary home, a hook payload in, the log and the verdict out. Nothing here types into a live session. The OpenCode bridge is tested by running the real plugin under `node` against a mock client that records every call it is asked to make, so the tests prove which call is made and when, and never that OpenCode honors it. The terminal host is tested by starting the real `karen` binary on `examples/fake-runtime`, a small program that paints a banner and a prompt, takes a line, is busy for a moment and answers (it logs every line it receives, every argument and every `KAREN_HOST_*` variable, which is how the tests know exactly what reached it). The delivery tests drive the real `karen hook` with the variables a hosted runtime would pass it, against the real host: a command delivered to an idle session, commands queued behind a busy turn and typed in order, a half-typed prompt left alone until the line times out, lines still waiting when the runtime exits, a host for another runtime, and a recording endpoint that proves every refused attempt sends it nothing. The idle rules are unit-tested on screens written to mirror the real ones. The tests run on Windows (ConPTY) and on Linux (a pty, under WSL); the macOS arm is the Unix arm and has not been run. Any layer that types into a terminal is tested against the fake runtime and recorded screens, never a real agent.
