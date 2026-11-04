# Session facts

A gate can read what the session's runtime actually has — how full its window is, which servers it loaded, what the provider last said, who it is — through a `field_*` matcher on `facts.<name>`. This page is the reference behind the [README's summary](../README.md#environment-facts): where each fact comes from for each runtime, when it is absent, which source wins, how a latch behaves over time, and how to find out why a facts gate is silent.

The one rule under all of it: **a fact she cannot read is absent, never zero and never empty.** A numeric matcher (`field_gte`, `field_lte`) on an absent fact does not match, a `field_equals` on one does not match, and a reason templating one renders `(unknown)`. A gate on a fact is quiet where the fact is missing, which is the correct thing for it to be.

She collects a fact only when a gate that applies to the current event names it, so a policy with no facts gates pays nothing for any of this.

## Where each fact comes from

| Fact | Claude Code | Codex | OpenCode | Absent when |
|------|-------------|-------|----------|-------------|
| `facts.runtime` | the `--runtime claude` that `karen setup` writes into the hook, else the transcript's path | `--runtime codex`, else the rollout's path | the plugin says `opencode` in its payload | never: `unknown` is the worst case |
| `facts.mcp_servers` | `.claude.json` (user entry and the entry for `cwd`) under `CLAUDE_CONFIG_DIR`, else `~`; the project's `.mcp.json`; the plugins its settings enable | `mcp_servers` in `config.toml` under `CODEX_HOME`, minus `enabled = false` | `mcp` in `opencode.json`/`opencode.jsonc`: global directory, `cwd`, `OPENCODE_CONFIG`, `OPENCODE_CONFIG_DIR`, later files overriding earlier | an empty list is the empty string, not absent; an `unknown` runtime reads no files |
| `facts.tools` | `PATH`, for the names in `[facts] tools` | the same | the same | an empty list is the empty string |
| `facts.git_uncommitted` | `git status --porcelain` for `cwd`, first 50 lines | the same | the same | absent when `cwd` is missing, not a repository or git will not answer; the empty string means a clean tree |
| `facts.session_id` | the payload's `session_id` | the same | the same | the payload carries none |
| `facts.context_tokens` | the tail of the transcript, read backwards (256 KiB, doubling to 8 MiB): the latest assistant turn's input plus both cache counts | the rollout's last `token_count` event | the bridge's payload | no transcript path, an unreadable file, a usage line past the 8 MiB cap, or a closed (zstd) Codex rollout |
| `facts.context_window` | `[facts.context_window] claude` in `config.toml` | the rollout's `model_context_window`, else `[facts.context_window] codex` | the bridge's payload (the model's own limit), else `[facts.context_window] opencode` | no source above has a number |
| `facts.context_ratio` | `context_tokens` over `context_window` | the same | the same | either side is absent, or the window is `0` |
| `facts.provider_error` | the transcript's last `isApiErrorMessage` entry | the rollout's last `error` event | the bridge caches `session.error` and carries it into the `Stop` review and the `ProviderError` event | no error, or a later assistant entry got through (the failure is stale and she drops it) |
| `facts.env.<NAME>` | her own environment, for names the **user layer's** `[facts] env` allowlists | the same | the same | the name is not allowlisted, or the variable is unset (an empty value is still a value) |

`runtime`, `mcp_servers` and `tools` are cached for the session in the state store; everything else is read fresh on every call that needs it.

## Which source wins

For any one fact, the first of these that has it:

1. **The payload's own `facts` block.** A runtime that already knows a number says so (the OpenCode bridge sends `context_tokens`, `context_window` and `provider_error` this way) and she reads nothing of her own for it.
2. **What she can read herself:** the transcript or rollout, the runtime's config files, `PATH`, `git`.
3. **The config:** `[facts.context_window]`, for the window alone, keyed by runtime name, so one runtime's number never answers for another's model.

Two exceptions to "the payload wins". `facts.env` is never a payload fact: the allowlist over her own environment is its only source, so nothing a bridge hands her can reach a gate as a variable the user never named. And the allowlist is the **user layer's** alone (`~/.karen/policies/config.toml`): a project layer that sets `[facts] env` is ignored, and `karen validate` names the layer that tried.

## A latch over time

A state spec with `field` and `rearm_below` is a latch: the gate fires, the key closes, and it stays closed until the field falls back below the `rearm_below` line. The re-arm is checked on every dispatch, matched or not, because a gate that fires at 40% can never match below 30% — a latch that waited for its own match to clear would stay shut for good.

For a `Stop` gate that matches at `facts.context_ratio >= 0.4` with `rearm_below: 0.3`:

| Stop | Ratio | Latch before | What happens |
|------|-------|--------------|--------------|
| 1 | 0.20 | armed | no match, nothing fires |
| 2 | 0.45 | armed | fires; the latch closes |
| 3 | 0.50 | closed | matches, but the latch holds her quiet |
| 4 | 0.35 | closed | not yet below 0.3: still closed |
| 5 | 0.25 | closed | below the line: the latch re-arms |
| 6 | 0.45 | armed | fires again; the latch closes |

The shipped version is [`examples/gates/compress-finished-work.json`](../examples/gates/compress-finished-work.json). `reset_on_compact` is the other way a key comes back: a compaction wipes the key of every gate whose state sets it, whatever kind of key that is.

## Debugging a facts gate

A gate that never fires on a fact is almost always one of four things. In the order worth checking:

1. **`karen validate`.** It names a gate matching `facts.env.<NAME>` for a variable the user layer never allowlisted ("the fact stays absent"), a project layer that tried to allowlist one, and a reason templating `{facts.env.NAME}` (env facts match; they never render). It also names an enabled gate that reads a `facts.<name>` she does not have — a matcher, a latch field or a reason that says `facts.context_ration` — because a misspelt fact is a gate that never fires and never says so. A disabled gate reads nothing, so none of these warnings apply to it.
2. **Is the fact there at all?** Feed `karen evaluate` the payload and look at the verdict: `data.fired_policies` names every gate that fired.

   ```bash
   echo '{"hook_event_name":"Stop","session_id":"s","facts":{"context_tokens":450,"context_window":1000}}' \
     | karen evaluate --request - --json --runtime opencode
   ```

   A `facts` block in the payload is source number one above, so this tests the gate without a transcript. For an environment fact, set the variable on the command (`HCOM_INSTANCE=luna karen evaluate …`) and allowlist it in your user layer.
3. **Is it the right event?** The runtime has to deliver it — see the [events table](../README.md#events). OpenCode sends no `UserPromptSubmit` and no compaction events, so a gate on one of those never runs there.
4. **Is the state what you think?** `karen evaluate` is a dry run: it never writes state, so a strike or a latch will not move under it. Run the real `karen hook` twice with the same payload to see a `deny_once_then_warn` or a latch change its answer.

The pack's [`examples/`](../examples/README.md) carry one tested gate for each fact family: the occupancy pair, the provider-error pair and the lanes gate.
