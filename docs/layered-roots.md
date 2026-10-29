# Layered policy roots

Karen merges three policy layers on every call, in a fixed order: built-in, user, project. One user's rules follow them into every project and every runtime, a project carries only what is specific to it, and nobody keeps copies of the same gate in sync.

## Layers

| Layer | Found at | Holds |
|---|---|---|
| Built-in | Compiled into the binary | Gates no reasonable user objects to: Karen's own policy roots are not edited by the agents she judges (locked), no force-push to a default branch, no secrets in commands |
| User | `<Karen home>/policies` | The user's own rules, in every project |
| Project | `KAREN_POLICIES` if set, else the nearest `.karen/policies` at or above the working directory | Rules specific to one project |

Karen home is `~/.karen` on every platform (`KAREN_HOME` overrides it), beside the other agent tools' home directories (`~/.claude`, `~/.codex`). Everything Karen keeps lives there, split by kind so the policy folder can be versioned or synced alone:

| Path | Holds |
|---|---|
| `~/.karen/policies/` | The user layer: `config.toml`, `gates/`, `rules/` |
| `~/.karen/state/` | Session state (strikes, cached facts) |
| `~/.karen/cache/` | The merged, compiled policy |
| `~/.karen/logs/` | Runtime plugin logs, such as the OpenCode bridge's fail-open log |

Each layer has exactly one discovery rule. A missing user or project layer is normal; the built-in layer is always present, so Karen never runs with nothing loaded. Every runtime calls the same binary, so Claude Code, Codex and OpenCode sessions see the same three layers.

## Merge rules

Gates are merged by `id`, layer by layer.

1. **Add.** A new id joins the set.
2. **Replace.** A later layer that defines an existing id must set `"override": true`; the new definition replaces the old one whole. Fields are never merged: a gate is always readable in one place. A reused id without the flag is a validation error.
3. **Disable.** A layer's `config.toml` lists `disable = ["gate-id", ...]` by exact id. An id that matches nothing is a warning.
4. **Lock.** A gate with `"locked": true` cannot be replaced or disabled by a later layer. At hook time the offending override or disable is dropped, the locked gate stays enforced, and the session sees a warning; `karen validate` reports it as an error. A lock violation never becomes a load failure, because load failures fail open: otherwise one malformed file in a project would switch every protection off.
5. **Rules.** Each layer's rule files are injected in layer order; a same-named file in a later layer replaces the earlier one. `rules.max_chars` bounds the total.
6. **Config.** A later layer wins per key; list values (`facts.tools`) are unioned.

## Provenance

`karen check` lists every gate with its layer and source file, and every replacement and disable with the layer that made it. An evaluation names the layer of each fired gate (`fired: gr-closing-check (project)`).

## Cost

Parsing three layers on every hook call would add to the load cost that already dominates a call (about 46 ms of about 60 ms on Windows with 54 gates). The merged, validated policy is therefore cached in `~/.karen/cache/`, keyed by the path, size and modification time of every file in every layer, and rebuilt only when one changes; regexes compile on first use by an applicable gate. The resident supervisor (ROADMAP) remains the path below one millisecond.

## Not included

- `extends` or include lists. Add them when a second need beyond the three layers appears.
- A team or organisation layer.
- Compatibility with the single-root search (`./policies`, `$CLAUDE_PLUGIN_ROOT/policies`, `policies/` above the binary) or the old locations (`~/.cache/karen`, `%TEMP%/karen-opencode.log`): Karen has no users depending on them.

## Tests

- Each merge rule, including a project that tries to disable or replace a locked gate (the gate stays enforced) and a reused id without `override` (validation error).
- Discovery of each layer, and the built-in layer alone when no other layer exists.
- Cache invalidation when any file in any layer changes, is added or is removed.
- Provenance in `karen check` and in evaluation output.
