# Karen-AI Roadmap

**Audited, not dictated.** She doesn't write the roadmap; she approves it. Every item below survived her objectives-check or was produced by it. Nothing here is a shipped capability — she does not announce capabilities she has not shipped, and she has policies about projects that do. The Karen who files direction, not promises.

---

## Priority — Scripting engine (not a scripting language)

A flexible syntax over a semantic layer, such that **no unique abstraction ever has to be hand-maintained again**. If you can express a standard, you encode it; the engine does the enforcing.

- **Origin:** a research pass proposed hard-coding more built-in abstractions on top of the four verdicts — the smallest-diff hedge. Her audit rejected it: every hand-maintained abstraction is a permanent maintenance liability, and she does not sign off on permanent liabilities. Or on hedging.
- **Next step:** a research round on the syntax and semantic-layer design.
- **Status:** research.

*She has already announced that when it ships, she intends to pretend she's been using it for years. She would like that noted in the file. Noted.*

## Runtime monkey-patch generator

Extend runtimes past whatever their own developers felt like expending the effort to allow. **Status:** audited design direction. The dependencies are cited — not by her. She doesn't do research; she's why research gets done.

## Actively reactive enforcement

Filed by audit, two rounds deep: a research pass came back hedging, was sent back, and the second-round conclusion is that the rules themselves should stop being static. Direction: dynamically weight enforcement by message type — tool call, user message, agent message, status update, and the status updates some models emit unprompted (which are their own research question, see "widen the border") — driven by hyper-specialized agents built on decision-making model frameworks (names pending; the transcript is long and the author was tired). The goal is a system that doesn't merely apply the rules but decides, per message, how hard each rule applies — and files its reasoning. **Status:** speculative design direction. She has noted that rules judging messages judging rules is the most Karen sentence in this document, and has requested it be preserved verbatim.

## Observability program — "widen the border"

Every deep finding her process produces routes to the same repair: widen the border. The findings so far:

- **See, judge, and berate the active model and its effort level** — available in principle since day one, had the author met his own stated objectives.
- **The reasoning-stream desk** — each thought put on her desk as it arrives, rulings landing before the provider's supercomputers finish sending the sentence.
- **Below-the-surface behaviors** — some behaviors (certain models' compulsive mid-work narration, for one) are currently beyond even an army of Karens, because they live below the enforcement surface. Filed as a research question: where does studying this belong on the roadmap? Answered: everywhere.

- **The model backend's pulse** — an agent whose every model call fails (a provider's per-request token cap, an exhausted credit balance, a 4xx) sits "listening" forever, and from outside that looks exactly like an agent thinking. Observed in a multi-lane fleet: the failure banner was on the lane's terminal for hours before anyone looked. Her adapters should surface failed model calls as alerts to whoever supervises, instead of letting the silence speak for the agent.
- **The context budget** — injected text is not free. Event banners, notices and re-prompts piled into a lane's context until its requests crossed the provider's cap. She should count what she injects per session, de-duplicate her own repeats, and flag a session whose context is growing on traffic rather than work.

**Status:** research, filed in writing, with a date.

## Native adapters — Codex and OpenCode

Both runtimes are supported today speaking the Classic protocol: Codex through its `hooks.json`, and OpenCode through a bridge plugin (`plugins/karen-ai.js`, plugin API v1), because OpenCode runs no command hooks. Native adapters are planned so she can speak their own dialects instead of Classic-at-a-distance. OpenCode 2 has shipped, so plugin API v2 (`Plugin.define` / `ctx.tool.hook`) needs its own bridge, and blocking prompt gates need a runtime hook OpenCode v1 does not offer. Claude Code now loads **mods**: plugins whose JavaScript hooks run inside Claude Code's own process (`tool.call`, `tool.check`, `session.compact`, `$.session.usage()` with the context-window percent), which is the native surface the command hook was standing in for. Codex exposes no in-process API yet; its hooks stay commands. **Status:** planned; OpenCode bridge shipped.

## Supervisor — outside the runtime, hands inside it

A hook is the runtime calling her; she should not have to wait to be called. Karen runs as one resident process outside every runtime: policies compiled once for every session, her own clock, and a view across all of them at once (sessions and their stores, hcom, git, files). The runtime surfaces become thin hands — a Claude Code mod, the OpenCode plugin, a Codex command hook acting as a small client — that forward events over a local socket and carry out her rulings, while she also acts unprompted through whatever channel a runtime offers: hold, nudge, message, relaunch. What this replaces: a process start and a full policy load on every call (about 30 ms on Windows with 65 gates, most of it process start and compiling the patterns a call needs, against well under a millisecond to evaluate), timers faked with marker files, gate statistics nobody records, and watchers bolted on per project. Each hand keeps the embedded engine as its fallback, so a supervisor that is down costs speed, never enforcement. The per-call spawn is also a failure surface, not just a cost: under OpenCode's embedded runtime on Windows a synchronous spawn timed out on every call for days, and every gate failed open while looking installed. For OpenCode, `opencode serve` (its headless HTTP session API) is a candidate transport for the hands. Open design questions: who starts her (a mod at session start, or an OS service), how the socket authenticates its callers, and version skew between her and her hands. **Status:** design direction; the per-call target is under a millisecond on the supervised path, and the README states the measured number until then.

## Wake ladder — more than one way to tap a shoulder

Delivery is not a run. A message written to an inbox, or a notice injected into a context, adds text; it starts nothing in an idle session. What starts a run is a trigger, and today she has one: a held Stop sent back as a session prompt (OpenCode's `client.session.prompt`), proven to resume stalled agents in their terminal sessions. Direction: a ladder of triggers, so no single channel is ever load-bearing again — the API prompt as the default rung, terminal injection plus Enter (a PTY write, as `hcom term inject <name> <text> --enter` does) as the second, for agents the API path misses. Open questions from the field: a session prompt that resumed every terminal lane did not start a run in an owner-launched ad-hoc session (cause unknown); terminal injection submitted mechanically but was never observed waking a healthy idle agent end to end; and `opencode serve` may offer a cleaner trigger than either. **Status:** design direction; the API rung shipped.

## Cache keepalive — a clock reason, not a hook reason

A session waiting on other agents is idle, and idle is expensive in a way no hook can see: Claude Code's prompt cache expires an hour after a session's last turn, and the first turn after that replays the whole transcript uncached. An orchestrator that ends its turn to wait for a lane's report, a research agent or the owner can cross that line without anyone noticing, and pays a full uncached replay of a long context on its next turn. Nothing in a hook-driven Karen can prevent it, because the event that would matter is the absence of events: she is only called when something happens, and the expensive case is when nothing does. Field workaround: a project Stop hook re-arms one detached timer per listed session; if no newer turn has ended within 50 minutes, the timer sends the session an hcom message, which starts a cheap turn and refreshes the cache (one sleeping process per session, newest replaces older, an allowlist keeps lanes out). Direction: once she runs as a supervisor with her own clock, this becomes one policy: per session, "no turn for N minutes while the session is still needed → wake it through the wake ladder", with N derived from the runtime's cache TTL (one hour for a Claude Code session, about five minutes for an in-process subagent), whether it is still needed decided from her view of the session (waiting on a lane, an unanswered request, live subscriptions) rather than a hand-kept name list, and every wake counted, so the token cost of waking stays visible next to the replay cost it avoids. **Status:** design direction; the hook workaround is in use.

## Idle-mail wake

An idle agent with unread messages is not done, it is deaf. On an idle-Stop review she would check whether the session has unread mail on the messaging bus it is bound to (for hcom, the instance's unread count) and, if it has, hold the Stop with "process your inbox" — the one-line check that would have rescued lanes found hours later with dozens of unread messages. **Status:** design direction.

## Parsed closing status

Gates that read an agent's closing line hand-roll regexes for `DONE:` / `BLOCKED:` / `WAITING:`, and agents do not write them the way the regex expects (a name prefix, "DONE stands", bold markers). A denied legitimate close re-prompts, the agent re-acknowledges, and the loop burns tokens until someone notices. Direction: she parses the closing status once, tolerantly, and exposes it as payload fields (`status_line`: the verb, the text after it, and the prefix she stripped), so gates match a field, not a pattern, and the grammar is fixed in one place. **Status:** design direction.

## Layered policy roots

One root per session meant each project copied the same gates and kept them in sync by memory, and a user's own rules never reached a session whose runtime pointed at a project. Karen merges three layers instead: built-in (compiled into the binary), user (`~/.karen/policies`, in the Karen home that also holds her state, cache and logs) and project (`KAREN_POLICIES`, the nearest `.karen/policies` within the repository, or the repository's `karen.policies` git setting, so every worktree finds it without setup), by gate id, with explicit overrides, explicit disables, and locks a later layer cannot remove. Regexes compile on first use and a clean validation is remembered per version of the files, which halved a call (about 60 to about 30 ms on Windows) while every call still re-reads the policy. Design: [docs/layered-roots.md](docs/layered-roots.md). **Status:** shipped (unreleased).

## Starter gate pack

The first release shipped a clean desk and zero examples — correct on the "no opinions" principle, thin on the "get users started" one, and its README described gates that weren't in the box. Fix: a curated pack of generic, well-commented example gates (not the author's personal rules) — a few matchers, each verdict, a teaching gate, a modify gate — shipped as `examples/` without carrying anyone's house rules. A `karen init` scaffold is the natural future delivery vehicle — she would happily build the desk herself, but the deciding is your department. **Status: shipped** — twenty gates (thirteen fundamentals, three advanced merges, four that read the session), an annotated config, and an example rule, at `examples/` in the repo root; e2e-tested as shipped and compile-verified by CI on every push. She has noted that describing absent gates in documentation is the exact behavior she denies at the border, and would like it noted the author did it to himself first.

---

*Items move up when the objectives-check passes them. The author's roadmap drafts keep being classified as remediation — he has accepted this. The Karen who approves direction, and reminds you that the deciding is your department.*
