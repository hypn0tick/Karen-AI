//! The terminal delivery layer's queue: lines handed to the host wait here, first in first out,
//! until the session is idle, and each ends as one line in the runtime log. The Karen who takes a
//! number and waits to be called.
//!
//! The process that handed a line over has exited long before it is typed, so the host files what
//! became of it: `sent` when it was typed, `undelivered:timeout` when no idle moment came in time,
//! `undelivered:host-exited` when the runtime ended first. Nothing leaves the queue unrecorded.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::endpoint::DeliveryMeta;
use super::idle::{self, Verdict};
use super::terminal::ScreenDump;
use crate::engine::jsonl;
use crate::engine::runtime_command::{LOG, layer, status};

/// How many lines may wait at once. A queue that grows without end is a runtime that is never idle,
/// and the lines behind the first are better refused than remembered for two minutes.
pub const MAX_QUEUE: usize = 16;

/// After typing a line, how long the next one waits at least. A runtime that does not echo may
/// look idle for a moment after it was handed a prompt.
const SETTLE: Duration = Duration::from_secs(1);

/// One line waiting for an idle session.
#[derive(Debug, Clone)]
pub struct Pending {
    pub text: String,
    pub meta: DeliveryMeta,
    pub since: Instant,
}

/// The waiting lines.
#[derive(Debug, Default)]
pub struct Queue {
    items: Mutex<VecDeque<Pending>>,
}

impl Queue {
    fn items(&self) -> MutexGuard<'_, VecDeque<Pending>> {
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Join the queue; the place in line (1 is next), or `None` when it is full.
    pub fn push(&self, text: String, meta: DeliveryMeta) -> Option<usize> {
        let mut items = self.items();
        if items.len() >= MAX_QUEUE {
            return None;
        }
        items.push_back(Pending {
            text,
            meta,
            since: Instant::now(),
        });
        Some(items.len())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items().is_empty()
    }

    fn front(&self) -> Option<Pending> {
        self.items().front().cloned()
    }

    fn pop(&self) -> Option<Pending> {
        self.items().pop_front()
    }
}

/// What the worker needs from the hosted session.
pub trait Session: Send + Sync {
    fn runtime(&self) -> &str;
    fn screen(&self) -> ScreenDump;
    /// How long the runtime has drawn nothing.
    fn quiet(&self) -> Duration;
    /// Type `text` and press Enter.
    ///
    /// # Errors
    ///
    /// When the terminal would not take it.
    fn press(&self, text: &str) -> Result<(), String>;
}

/// How the worker decides and waits.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// The runtime must have drawn nothing for this long to count as idle.
    pub quiet: Duration,
    /// A line that has waited this long is given up on.
    pub timeout: Duration,
    /// How often the worker looks.
    pub poll: Duration,
}

/// Where the worker files a line of the runtime log. Production appends to the log; tests collect.
pub type Sink = Arc<dyn Fn(Value) + Send + Sync>;

/// The sink that writes to `<Karen home>/logs/karen-runtime.jsonl`.
#[must_use]
pub fn log_sink() -> Sink {
    Arc::new(|line| jsonl::append(LOG, &line))
}

fn record(item: &Pending, outcome: &str) -> Value {
    json!({
        "time": jsonl::now_millis(),
        "session": item.meta.session,
        "gate": item.meta.gate,
        "runtime": item.meta.runtime,
        "command": item.meta.command,
        "arguments": item.meta.arguments,
        "layer": layer::TERMINAL_HOST,
        "route": "karen run",
        "status": outcome,
        "waited_ms": u64::try_from(item.since.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

/// One look at the head of the queue: give it up, type it, or leave it. `last_typed` is when the
/// worker last typed.
pub fn step(
    session: &dyn Session,
    queue: &Queue,
    tuning: &Tuning,
    sink: &Sink,
    last_typed: &mut Option<Instant>,
) {
    let Some(head) = queue.front() else {
        return;
    };
    if head.since.elapsed() >= tuning.timeout {
        queue.pop();
        sink(record(&head, status::TIMEOUT));
        return;
    }
    if last_typed.is_some_and(|at| at.elapsed() < SETTLE) {
        return;
    }
    let verdict = idle::judge(
        session.runtime(),
        &session.screen(),
        session.quiet(),
        tuning.quiet,
    );
    if verdict != Verdict::Idle {
        return;
    }
    queue.pop();
    match session.press(&head.text) {
        Ok(()) => {
            *last_typed = Some(Instant::now());
            sink(record(&head, status::SENT));
        }
        Err(_) => sink(record(&head, status::TYPE_FAILED)),
    }
}

/// Run [`step`] every `tuning.poll` until `stop` is set.
pub fn run_worker(
    session: &Arc<dyn Session>,
    queue: &Arc<Queue>,
    tuning: Tuning,
    sink: &Sink,
    stop: &AtomicBool,
) {
    let mut last_typed = None;
    while !stop.load(Ordering::SeqCst) {
        step(session.as_ref(), queue, &tuning, sink, &mut last_typed);
        std::thread::sleep(tuning.poll);
    }
}

/// File every line still waiting as `outcome`: the runtime ended first.
pub fn drain(queue: &Queue, sink: &Sink, outcome: &str) {
    while let Some(item) = queue.pop() {
        sink(record(&item, outcome));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        screen: Mutex<String>,
        quiet: Mutex<Duration>,
        typed: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(screen: &str) -> Arc<Self> {
            Arc::new(Self {
                screen: Mutex::new(screen.to_string()),
                quiet: Mutex::new(Duration::from_secs(5)),
                typed: Mutex::new(Vec::new()),
            })
        }
        fn set_screen(&self, text: &str) {
            *self.screen.lock().unwrap() = text.to_string();
        }
        fn typed(&self) -> Vec<String> {
            self.typed.lock().unwrap().clone()
        }
    }

    impl Session for Fake {
        fn runtime(&self) -> &'static str {
            "fake-runtime"
        }
        fn screen(&self) -> ScreenDump {
            ScreenDump {
                text: self.screen.lock().unwrap().clone(),
                rows: 24,
                cols: 80,
                cursor_row: 0,
                cursor_col: 0,
                alternate: false,
            }
        }
        fn quiet(&self) -> Duration {
            *self.quiet.lock().unwrap()
        }
        fn press(&self, text: &str) -> Result<(), String> {
            self.typed.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    fn meta(command: &str) -> DeliveryMeta {
        DeliveryMeta {
            session: "s1".into(),
            gate: "g".into(),
            runtime: "fake-runtime".into(),
            command: command.into(),
            arguments: Some("{gate}".into()),
            sent_at_ms: None,
        }
    }

    fn tuning(timeout_ms: u64) -> Tuning {
        Tuning {
            quiet: Duration::from_millis(400),
            timeout: Duration::from_millis(timeout_ms),
            poll: Duration::from_millis(5),
        }
    }

    fn sink() -> (Sink, Arc<Mutex<Vec<Value>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&lines);
        (Arc::new(move |line| kept.lock().unwrap().push(line)), lines)
    }

    #[test]
    fn an_idle_session_gets_the_head_of_the_queue_and_the_log_says_sent() {
        let (session, queue, (sink, lines)) = (Fake::new("> "), Queue::default(), sink());
        queue.push("/compact g".into(), meta("/compact"));
        let mut last = None;

        step(session.as_ref(), &queue, &tuning(60_000), &sink, &mut last);

        assert_eq!(session.typed(), ["/compact g"]);
        assert!(queue.is_empty());
        let lines = lines.lock().unwrap();
        assert_eq!(lines[0]["status"], "sent");
        assert_eq!(lines[0]["layer"], "terminal-host");
        assert_eq!(
            lines[0]["arguments"], "{gate}",
            "the template, never the line typed"
        );
        assert!(!lines[0].to_string().contains("/compact g"));
    }

    #[test]
    fn a_busy_session_keeps_the_queue_waiting() {
        let (session, queue, (sink, lines)) =
            (Fake::new("> hello\nworking..."), Queue::default(), sink());
        queue.push("/compact".into(), meta("/compact"));
        let mut last = None;

        step(session.as_ref(), &queue, &tuning(60_000), &sink, &mut last);

        assert_eq!(session.typed(), Vec::<String>::new());
        assert_eq!(queue.len(), 1);
        assert!(lines.lock().unwrap().is_empty());
    }

    #[test]
    fn a_half_typed_prompt_is_never_typed_into() {
        let (session, queue, (sink, _lines)) = (Fake::new("> par"), Queue::default(), sink());
        queue.push("/compact".into(), meta("/compact"));
        let mut last = None;

        step(session.as_ref(), &queue, &tuning(60_000), &sink, &mut last);

        assert!(
            session.typed().is_empty(),
            "the owner's half-typed prompt stays theirs"
        );
    }

    #[test]
    fn a_line_that_waits_too_long_is_filed_as_a_timeout_and_never_typed() {
        let (session, queue, (sink, lines)) = (Fake::new("> par"), Queue::default(), sink());
        queue.push("/compact".into(), meta("/compact"));
        std::thread::sleep(Duration::from_millis(30));
        let mut last = None;

        step(session.as_ref(), &queue, &tuning(10), &sink, &mut last);

        assert!(queue.is_empty());
        assert_eq!(session.typed(), Vec::<String>::new());
        assert_eq!(lines.lock().unwrap()[0]["status"], "undelivered:timeout");
    }

    #[test]
    fn lines_go_in_the_order_they_arrived_and_one_per_idle() {
        let (session, queue, (sink, lines)) = (Fake::new("> "), Queue::default(), sink());
        queue.push("/first".into(), meta("/first"));
        queue.push("/second".into(), meta("/second"));
        let mut last = None;

        step(session.as_ref(), &queue, &tuning(60_000), &sink, &mut last);
        step(session.as_ref(), &queue, &tuning(60_000), &sink, &mut last);

        assert_eq!(
            session.typed(),
            ["/first"],
            "the second waits out the settle time"
        );
        last = None;
        session.set_screen("> ");
        step(session.as_ref(), &queue, &tuning(60_000), &sink, &mut last);
        assert_eq!(session.typed(), ["/first", "/second"]);
        assert_eq!(lines.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_full_queue_refuses_the_next_line() {
        let queue = Queue::default();
        for place in 1..=MAX_QUEUE {
            assert_eq!(queue.push("/x".into(), meta("/x")), Some(place));
        }
        assert_eq!(queue.push("/x".into(), meta("/x")), None);
    }

    #[test]
    fn what_is_still_waiting_when_the_runtime_ends_is_filed_not_forgotten() {
        let queue = Queue::default();
        let (sink, lines) = sink();
        queue.push("/a".into(), meta("/a"));
        queue.push("/b".into(), meta("/b"));

        drain(&queue, &sink, status::HOST_EXITED);

        let lines = lines.lock().unwrap();
        assert_eq!(lines.len(), 2);
        assert!(
            lines
                .iter()
                .all(|l| l["status"] == "undelivered:host-exited")
        );
        assert_eq!(lines[0]["command"], "/a", "first in, first filed");
    }
}
