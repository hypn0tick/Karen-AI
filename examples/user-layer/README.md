# A user layer — the part of Karen that may touch the keyboard

The starter pack in `examples/gates/` is for any layer. This directory is only for **your** layer, `~/.karen/policies/`, because what it shows is refused anywhere else: a gate that types into a session, and the config that lets it. A repository can ship rules, but it cannot lend itself your keyboard.

| File | What it is |
| ---- | ---------- |
| `config.toml` | `[actions] runtime_commands` (the allowlist), `[host]` (how `karen run` waits and types), and the commented knobs the gates below lean on. |
| `gates/compact-when-full.json` | A `runtime_command` gate: type `/compact` into the session when the window passes 85%, once per filling (the latch). Ships **disabled**. |
| `gates/screen-shows-a-usage-limit.json` | A gate on `facts.screen.text`: notice a usage-limit banner only the terminal shows. Ships **disabled** and **provisional**: its pattern has not been checked against a recorded banner. |

## Install

```bash
# merge config.toml by hand if you already have one
cp examples/user-layer/config.toml ~/.karen/policies/config.toml
cp examples/user-layer/gates/*.json  ~/.karen/policies/gates/
karen validate
```

The gates ship disabled, so nothing types until you set `"enabled": true` in the one you want. `karen validate` then checks it the way a delivery would: the gate is the user layer's, its command is on the allowlist, the arguments can be typed, and the runtime has a layer that reaches it.

## Reaching the session

- **OpenCode**: through the bridge plugin's own API. Nothing else to do.
- **Claude Code and Codex**: start the session as `karen run claude` or `karen run codex`. Karen then owns the terminal the runtime runs in and can type into it when it is idle. A session started any other way has no layer, and the attempt is logged as `undelivered:no-layer` in `~/.karen/logs/karen-runtime.jsonl`. Nothing is typed.

What a delivery does, what can stop it and every status it can end in: [`docs/runtime-control.md`](../../docs/runtime-control.md). The facts a gate can read from the screen: [`docs/session-facts.md`](../../docs/session-facts.md).

## What is not verified

`karen run` has been tried against a live OpenCode (a line typed at once when idle; a second queued line typed only after the first turn ended). It has **not** been tried against a live Claude Code or Codex. The idle rules for Claude Code come from one recorded screen at rest, and Codex's from how Codex describes itself. Try `compact-when-full` on a session you can afford to watch before you leave it alone.
