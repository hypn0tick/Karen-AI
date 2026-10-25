# Example Gates — the starter pack

**Fifteen heavily-commented gates: twelve fundamentals, three advanced merges. Plus a minimal `config.toml` and one example rule.** They are not the author's rules — they are his lectures, yours to assign. Copy what you like into your `policies/gates/`, adjust to taste, and run `karen validate`. The Karen who ships the desk, the curriculum, and none of her opinions about your house.

Two things worth noticing before you copy anything:

1. **Every file is `.json` — with comments in it.** That's not a typo; the loader parses all extensions through the same engine, and comments in `.json` files are the house style. The permissiveness claim in the README ships fifteen proofs of itself in this directory.
2. **Every regex here avoids lookaheads.** The engine uses Rust's regex crate, which doesn't have them — exclusion is the `none` block, and it's a first-class citizen. When you write your own gates, reach for `none` instead of `(?!...)`, or she will hand you a `POLICY_INVALID` envelope and a look.

## The fundamentals (one mechanism each)

| File | Verdict | Teaches |
| ---- | ------- | ------- |
| `no-force-push-to-main.json` | deny | `command_regex`, priorities |
| `test-attention.json` | deny_once_then_warn | `file_regex`, tool filters, attention enforcement |
| `claims-need-evidence.json` | deny_once_then_warn (ships **disabled**) | `content_regex` (reads `content`/`new_string`/`newString` — the text you're writing). The "before" of a pair: enable it or the advanced pipeline below — never both, or she denies the very claims the pipeline would accept |
| `search-before-current-facts.json` | deny | `prompt_regex`, the `UserPromptSubmit` event, `decision: "block"` |
| `jq-yaml-teaching-gate.json` | deny_once_then_warn | the full `state` block: `track`, `key`, `max_strikes` |
| `head-to-select-object.json` | modify | the `modifier` field, the engine's one shipped rewriter |
| `break-glass-ask.json` | ask | escalation as a feature, the `any` block, `command_contains` |
| `no-secrets-in-commands.json` | deny | regex alternation, credentials never ride in commands |
| `lockfile-respect.json` | deny | `path_regex`, and when-exact-vs-when-pattern |
| `rehearsal-vs-performance.json` | allow + ask | multi-gate files, the `none` block, and the precedence law (below) |
| `scope-creep-warner.json` | deny_once_then_warn | `prompt_regex` round two — she enforces process, not just commands |
| `friday-deploy-freeze.json` | deny (disabled) | the `always` matcher, the `enabled` flag, and one honest engine limit |

## The advanced (concepts merged)

| File | The merge |
| ---- | --------- |
| `evidence-pipeline.json` | Conditional enforcement: claims without citations get one hard look; claims WITH citations get an acknowledgment every time — via the `max_strikes: 1` discovery, which turns a teaching gate into a notice-injector that never reaches its denial branch. |
| `unix-isms-on-windows.json` | The portability suite: teaching gates that lecture instead of rewrite, and the honest engine limit stated where you'll meet it — exactly one `modifier` ships today. |
| `structured-payload-trust-model.json` | Precedence, demonstrated instead of claimed: a deny at priority 80 short-circuits before the ask at 60 can even evaluate; `field_equals` on structured payload fields, used where exactness is the point. |

## The law of the land (verified from the dispatcher)

**Deny short-circuits the moment it is reached. Without a deny, Modify beats Ask beats Allow. Priority orders arrival — it does not let a permission overrule a prohibition.** The way to "allow with exceptions" is the `none` block inside the stop itself, which is why `rehearsal-vs-performance.json` is built the way it is. She is not overruled by permissions; you sharpen what she stops.

## Also in the box

- `config.toml` — every knob she has, annotated. Copy to your `policies/` root and adjust.
- `rule-00-briefing.md` — the delivered-prose side of the house: rules are injected into every session in tracked parts, but prose binds nobody. That's what the gates are for. The briefing is the briefing; the gates are the border.

## After you copy

```bash
karen validate     # she checks your work before accepting it
karen check        # every gate and rule, by name
```

And if a gate here isn't strict enough for you: good. That was the deciding part, and it was always going to be your department.
