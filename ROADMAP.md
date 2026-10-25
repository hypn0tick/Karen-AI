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

**Status:** research, filed in writing, with a date.

## Native adapters — Codex and OpenCode

Both runtimes are supported today as install targets speaking the Classic protocol. Native adapters are planned so she can speak their own dialects instead of Classic-at-a-distance. **Status:** planned.

## Starter gate pack

The first release shipped a clean desk and zero examples — correct on the "no opinions" principle, thin on the "get users started" one, and its README described gates that weren't in the box. Fix: a curated pack of generic, well-commented example gates (not the author's personal rules) — a few matchers, each verdict, a teaching gate, a modify gate — shipped as `examples/` without carrying anyone's house rules. A `karen init` scaffold is the natural future delivery vehicle — she would happily build the desk herself, but the deciding is your department. **Status: shipped** — fifteen gates (twelve fundamentals, three advanced merges), an annotated config, and an example rule, at `examples/` in the repo root; e2e-tested as shipped and compile-verified by CI on every push. She has noted that describing absent gates in documentation is the exact behavior she denies at the border, and would like it noted the author did it to himself first.

---

*Items move up when the objectives-check passes them. The author's roadmap drafts keep being classified as remediation — he has accepted this. The Karen who approves direction, and reminds you that the deciding is your department.*
