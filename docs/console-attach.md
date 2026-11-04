# Attaching to a console Karen did not start

`karen run` can type into a session because it started it: it owns the pseudo-terminal. A session started any other way (a plain terminal, a Windows Terminal tab, a window another tool opened) has no such owner. The question this page answers is whether Karen could still reach it on Windows, by attaching to the console the session already runs in. The answer was measured with a probe, [`examples/attach-probe.rs`](../examples/attach-probe.rs), and it is **yes for reading and for typing, on every target tried**. It is not wired into Karen as a layer, and the reasons are at the end.

## What the probe does

```text
attach-probe PID [--read] [--type TEXT] [--enter] [--enter-delay-ms 150]
```

It lets go of its own console (`FreeConsole`), takes the console of process `PID` (`AttachConsole`), then:

- `--read` opens `CONOUT$` and reads the visible rows of the screen buffer (`ReadConsoleOutputCharacterW`);
- `--type TEXT` opens `CONIN$` and writes TEXT as key events, one down and one up per character (`WriteConsoleInputW`);
- `--enter` presses Enter after the pause, so that a terminal UI that reads a burst of input as a paste does not take the Enter for part of it.

The console's screen buffer is the grid of cells the console holds, so what comes back is text: the escape sequences have already been interpreted. That is the same shape as the screen `karen run` keeps, and the same idle rules read it.

## What was tried, and what happened

| Target | How it was running | Read | Type and Enter | Evidence |
|--------|--------------------|------|----------------|----------|
| the fake runtime | hosted by `karen run` (a ConPTY), cooked input | yes | yes: `you said: from outside` | `tests/e2e/host/attach.rs`; the host's endpoint and stdin were not used |
| a live OpenCode (free model, throwaway home) | hosted by `karen run`, a TUI that reads VT input | not tried | yes: 29 characters and Enter were submitted from outside and answered `ok` | `record-screens --attach` |
| the fake runtime | its own hidden classic console (`conhost`), started with `Start-Process -WindowStyle Hidden` | yes | yes: the runtime logged the line and answered | run by hand, once; not a test |
| a live Claude Code | a Windows Terminal tab (its console is a ConPTY), not started by Karen | yes: 52 rows, with the composer between its rules and the status line | **not tried** | run by hand, read-only |
| a process that does not exist | | fails: `AttachConsole(…): The parameter is incorrect.` | | `tests/e2e/host/attach.rs` |

Not tried at all: typing into a live Claude Code or Codex (a live agent is not a thing to type into for a test), a console owned by another user, a remote session, and any non-Windows platform.

The live Claude Code read was also a recording. It was taken mid-turn, and it showed that the "esc to interrupt" hint the idle rules had relied on is not always on screen: the spinner line read `Calculating… (46m 18s · ↓ 139.5k tokens · thought for 6s)` with no such phrase, and an empty composer under it. The rules would have called that idle. A running turn is now also recognised by the spinner's timer, and the scrubbed screen is `tests/fixtures/screens/claude-busy.txt`.

## Things the probe taught

- **A process has one console.** Attaching to the target's console takes it from the one the probe had, so the probe is always its own process. Karen would never do this in the process of a session she cares about.
- **Standard handles break.** A standard stream that was the old console is closed by `FreeConsole`, and its handle value can be handed out again for the target's console. The probe holds its output until it has left the target, then writes it through the parent's console, re-attached; a stream that was a pipe or a file is untouched by the move and is written as it is. Both paths were exercised. A first version that printed as it went printed nothing when run from a console.
- **Typing is key events.** Characters as `KEY_EVENT` records are enough for a cooked-input program and for a VT-input TUI (OpenCode), which the console turns into the sequences it expects. Enter needs the same short pause `karen run` uses.
- **The screen is the visible window**, not the scrollback. That is what the idle rules want.

## Why it is not a layer

Reading and typing work. What a layer needs besides them does not exist for a session she did not start:

1. **Finding the process.** A hook is a child of the runtime, so the runtime is among its ancestors (in the session this was measured in: `claude.exe` ← `hcom.exe` ← `powershell.exe` ← `WindowsTerminal.exe`). Picking the right ancestor by name, and attaching to it from a hook that the runtime started without a console of its own, has not been tried.
2. **Somewhere to wait.** The hook exits, and the line has to wait for an idle session, possibly for minutes. `karen run` is the resident process that holds the queue, the clock and the ledger. A session Karen did not start has none, and a detached helper that waits would be a second long-lived Karen process per session.
3. **A boundary.** Any process of the same user can already do this. A layer built on it adds no new power, but it would type into sessions the owner never asked her to host, so it would need its own opt-in in the user layer's `config.toml`, like every other keyboard in Karen.

So `karen run claude` stays the way to give her a keyboard, because she owns idle, the queue and the ledger there. The probe stays as a tool and as the evidence for the day an owner cannot start their sessions that way. Building the layer would mean solving the three points above first, with a live Claude Code and Codex to try them on.

## Reproducing it

```powershell
cargo build --examples
# 1. a session Karen did not start: find its pid, then read it (read-only, safe on a live agent)
target\debug\examples\attach-probe.exe <pid> --read
# 2. a throwaway one: a hosted OpenCode, typed into from outside
target\debug\examples\record-screens.exe --runtime opencode --out DIR --attach `
  --env OPENCODE_DISABLE_AUTOUPDATE=1 -- opencode --model opencode/big-pickle
```

Read the screens before keeping any: a path, a name or an account can be on them.
