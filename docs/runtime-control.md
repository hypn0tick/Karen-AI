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
| `undelivered:no-layer` | Allowed, and nothing reaches this session. |

`refused` means a rule of hers stopped it. `undelivered` means it was allowed and did not arrive. Both leave a log line, and neither is ever silent.

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

`layer` is the layer that carried the command, `null` when none did. `arguments` is the gate's own text with its placeholders unexpanded: what a session's facts rendered into the arguments is data about the session, and the ledger does not keep it. The same record rides the verdict: `karen evaluate --json` lists it under `runtime_commands`. A rehearsal decides the same status and files nothing, so a dry run does not write a line.

## What `karen validate` says

`karen validate` reports, as warnings, a gate that cannot do what it says: one outside the user layer, one whose command is off the allowlist, one whose arguments cannot be typed or template the agent's own text, and one whose runtime has no layer. It also lists the layers that can reach each configured runtime, in the text report whenever an enabled `runtime_command` gate exists and always in `--json` as `runtime_layers`. A gate switched off types nothing and gets no warning, so a starter pack can ship one that stays quiet until somebody turns it on.

## Which layer each runtime gets

A command reaches a session through whichever layer that session has: the runtime's own plugin API, or a terminal Karen can write to. Today no layer is wired, so every allowed attempt ends as `undelivered:no-layer` and validate says so for each configured runtime. Set `[actions] runtimes` to the runtimes you can actually reach and the warning goes quiet for the rest. The layers arrive one at a time, and this table grows with them.

| Runtime | Layer | Reaches it when |
|---------|-------|-----------------|
| Claude Code | none yet | |
| Codex | none yet | |
| OpenCode | none yet | |

## How this is tested

The attempts are tested end to end against the real binary: a user layer and a project layer in a temporary home, a hook payload in, the log and the verdict out. Nothing here types into a live session; there is no layer to do it yet, and when there is, its tests run against a fake runtime and recorded screens, never a real agent.
