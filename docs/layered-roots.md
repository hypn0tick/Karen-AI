# Layered policy roots

Karen merges three policy layers on every call, in a fixed order: built-in, user, project. One user's rules follow them into every project and every runtime, a project carries only what is specific to it, and nobody keeps copies of the same gate in sync.

## Layers

| Layer | Found at | Holds |
|---|---|---|
| Built-in | Compiled into the binary | Gates no reasonable user objects to: no force-push to a default branch (`karen-no-force-push-default-branch`), no secrets in commands (`karen-no-secrets-in-commands`). Policy roots stay editable by agents, so rules can be improved while sessions run; a project that wants its root locked adds that gate itself |
| User | `<Karen home>/policies` | The user's own rules, in every project |
| Project | `--policies` or `KAREN_POLICIES` if set, else the nearest `.karen/policies` from the working directory up to the repository root (never above your home directory), else the repository's `karen.policies` git setting | Rules specific to one project |

Karen home is `~/.karen` on every platform (`KAREN_HOME` overrides it), beside the other agent tools' home directories (`~/.claude`, `~/.codex`). Everything Karen keeps lives there, split by kind so the policy folder can be versioned or synced alone:

| Path | Holds |
|---|---|
| `~/.karen/policies/` | The user layer: `config.toml`, `gates/`, `rules/` |
| `~/.karen/state/` | Session state (strikes, cached facts) |
| `~/.karen/cache/` | Validation markers: which versions of each layer's files compiled cleanly |
| `~/.karen/logs/` | Runtime plugin logs, such as the OpenCode bridge's fail-open log |

The project layer is found from the session's working directory, never from how a runtime was set up, so a new worktree or a new agent cannot miss it. A checkout's own `.karen/policies` wins; the walk stops at the repository root and never climbs above the home directory, because a root there belongs to another project. A repository whose policies live outside its checkouts names them once with `git config karen.policies <path>`: git keeps that setting in the repository's common directory, which every linked worktree shares, and a relative path resolves against the main working tree. `karen validate` and `karen check` report `project_found_by` (`explicit`, `in-tree` or `git-config`).

Each layer has exactly one discovery rule. A missing user or project layer is normal; the built-in layer is always present, so Karen never runs with nothing loaded. Every runtime calls the same binary, so Claude Code, Codex and OpenCode sessions see the same three layers.

## Merge rules

Gates are merged by `id`, layer by layer.

1. **Add.** A new id joins the set.
2. **Replace.** A later layer that defines an existing id must set `"override": true`; the new definition replaces the old one whole. Fields are never merged: a gate is always readable in one place. A reused id without the flag is a validation error.
3. **Disable.** A layer's `config.toml` lists `disable = ["gate-id", ...]` by exact id. An id that matches nothing is a warning.
4. **Lock.** A gate with `"locked": true` cannot be replaced or disabled by a later layer. At hook time the offending override or disable is dropped, the locked gate stays enforced, and the session sees a warning; `karen validate` reports it as an error. A lock violation never becomes a load failure, because load failures fail open: otherwise one malformed file in a project would switch every protection off.
5. **Rules.** Each layer's rule files are injected in layer order; a same-named file in a later layer replaces the earlier one.
6. **Config.** A later layer wins per key; list values (`facts.tools`) are unioned.

## Provenance

`karen check` lists every gate with its layer and source file, and every replacement and disable with the layer that made it. An evaluation names the layer of each fired gate (`fired: gr-closing-check (project)`).

## Cost

Measured on Windows with 63 gates, reading and parsing every policy file costs about 1 ms and compiling every regex about 55 ms of a 60 ms call. So the files are re-read on every call, which keeps hot reload exact, and regexes compile only when a gate that applies to the event evaluates them, with conditions short-circuiting. A layer's patterns are validated once per version of its files: a marker in `~/.karen/cache/`, keyed by the path, size and modification time of every file in that layer, records a clean validation, and any edit validates again; `karen validate` always compiles everything. Result: about 34 ms for a Bash call and 29 ms for a Stop with 65 gates, of which about 9 ms is process start. The rest is compiling the patterns that call needs, paid again by every fresh process; the resident supervisor (ROADMAP) compiles once and is the path below one millisecond.

## Not included

- `extends` or include lists. Add them when a second need beyond the three layers appears.
- A team or organisation layer.
- Compatibility with the single-root search (`./policies`, `$CLAUDE_PLUGIN_ROOT/policies`, `policies/` above the binary) or the old locations (`~/.cache/karen`, `%TEMP%/karen-opencode.log`): Karen has no users depending on them.

## Tests

- Each merge rule, including a project that tries to disable or replace a locked gate (the gate stays enforced) and a reused id without `override` (validation error).
- Discovery of each layer, and the built-in layer alone when no other layer exists.
- A linked worktree finds the repository's `karen.policies` layer, a root above the repository is ignored, and a checkout's own root wins over the pointer.
- An edited gate is enforced on the next call, and an edit that breaks a pattern is caught on the next call although the previous version validated.
- Provenance in `karen check` and in evaluation output.
