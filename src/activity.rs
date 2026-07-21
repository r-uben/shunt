//! Bounded, privacy-safe in-memory activity store backing the admin "live
//! activity" view (M13, issue #214).
//!
//! One process-lifetime, fixed-capacity queue holds both active and recently
//! finished rows; a row transitions from active to terminal in place rather
//! than being duplicated into a second store. The queue is created only when
//! `[server.admin]` is present at boot (see `crate::server::AppState`) and is
//! reset on process restart — it is an operational aid, not an audit log.
//!
//! Frozen privacy contract (issue #214): a record may carry only a synthetic
//! process-local id, bounded provider/model display strings, protocol/adapter
//! labels, timestamps/durations, HTTP status, terminal outcome, and
//! upstream-reported streaming token counts. It must never carry prompts,
//! responses, reasoning, tool arguments/results, request bodies or headers,
//! credentials, account identities, or raw/derived session ids.
//!
//! Every operation here is synchronous, best-effort, and non-panicking —
//! including recovery from a poisoned lock — so a bug or race in this module
//! can never fail an inference request.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

/// Maximum number of rows (active + recent) the queue retains. When full, the
/// oldest *terminal* row is evicted first, so a still-active request is never
/// dropped to make room for a newer one while any finished row remains to
/// reclaim. Only when every retained row is still active is the oldest active
/// row evicted — some observation must be lost once capacity is fully live.
pub const MAX_ROWS: usize = 200;

/// Maximum number of bytes retained for a copied, untrusted display string
/// (provider/model labels). Capping happens on a UTF-8 character boundary so
/// the stored string is always valid UTF-8.
pub const MAX_LABEL_BYTES: usize = 64;

/// Synthetic, process-local row identifier. Assigned by the store itself and
/// unrelated to any client, session, or account identifier.
pub type ActivityId = u64;

/// The protocol surface a request arrived through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityProtocol {
    /// `/v1/messages`.
    Messages,
    /// The inbound Codex Responses endpoint.
    Codex,
}

/// Lifecycle/terminal state of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    /// Routing resolved and the request is in flight; no terminal outcome yet.
    Active,
    /// The response completed successfully.
    Completed,
    /// The adapter or upstream returned an error.
    Error,
    /// The upstream connection was cut mid-response.
    UpstreamCut,
    /// The inbound client disconnected before a terminal outcome.
    ClientDisconnect,
}

impl ActivityState {
    /// Whether this state is a terminal outcome (anything but `Active`).
    pub fn is_terminal(self) -> bool {
        !matches!(self, ActivityState::Active)
    }
}

/// One bounded, sanitized activity row.
///
/// Deliberately has no content, header, credential, account, or session
/// field: only operational metadata described in the module docs above.
#[derive(Debug, Clone)]
pub struct ActivityRecord {
    pub id: ActivityId,
    pub protocol: ActivityProtocol,
    /// Bounded, UTF-8-safely capped provider display label.
    pub provider: String,
    /// Bounded, UTF-8-safely capped model display label.
    pub model: String,
    pub state: ActivityState,
    pub started_at: Instant,
    /// Elapsed time until the response headers arrived (not full completion).
    pub header_latency: Option<Duration>,
    /// Elapsed time until the first streamed token, for streaming responses.
    pub ttft: Option<Duration>,
    /// Total elapsed time from start to the terminal transition. `None` while
    /// active. Stored explicitly (rather than derived from `started_at` at read
    /// time) so a finished row carries a stable, serializable duration; the
    /// wall-clock `Instant` in `started_at` is never itself serialized.
    pub duration: Option<Duration>,
    pub status: Option<u16>,
    /// Upstream-reported streaming input tokens, when available.
    pub input_tokens: Option<u64>,
    /// Upstream-reported streaming output tokens, when available.
    pub output_tokens: Option<u64>,
}

/// Truncate `s` to at most `max_bytes` bytes without splitting a UTF-8
/// character, so the result is always valid UTF-8. Used to bound copied,
/// untrusted provider/model strings.
fn cap_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Bounded, privacy-safe in-memory queue of activity rows. Process-lifetime:
/// created once (see `crate::server::AppState`) and shared across hot config
/// reloads, but never persisted and never restored across a process restart.
pub struct ActivityStore {
    capacity: usize,
    next_id: AtomicU64,
    rows: Mutex<VecDeque<ActivityRecord>>,
}

impl ActivityStore {
    /// Build a store bounded by [`MAX_ROWS`].
    pub fn new() -> Self {
        Self::with_capacity(MAX_ROWS)
    }

    /// Build a store with an explicit capacity. Exposed at crate visibility
    /// so unit tests can exercise eviction without allocating `MAX_ROWS`
    /// rows.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: AtomicU64::new(1),
            rows: Mutex::new(VecDeque::with_capacity(capacity.min(MAX_ROWS))),
        }
    }

    /// Lock the row queue, recovering rather than panicking if a prior
    /// operation poisoned the mutex (e.g. a panic while holding the lock).
    /// A poisoned lock must never fail the inference request calling in.
    fn lock_rows(&self) -> MutexGuard<'_, VecDeque<ActivityRecord>> {
        self.rows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Start a new active row for a request whose provider/adapter metadata
    /// is already trusted (routing resolved). When the queue is at capacity the
    /// oldest *terminal* row is evicted first, falling back to the oldest active
    /// row only when every retained row is still active. Always returns an id;
    /// never panics. A zero-capacity store retains nothing but still returns an
    /// id, so callers need no special case (the matching `finish` no-ops).
    pub fn start(&self, protocol: ActivityProtocol, provider: &str, model: &str) -> ActivityId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let record = ActivityRecord {
            id,
            protocol,
            provider: cap_utf8(provider, MAX_LABEL_BYTES),
            model: cap_utf8(model, MAX_LABEL_BYTES),
            state: ActivityState::Active,
            started_at: Instant::now(),
            header_latency: None,
            ttft: None,
            duration: None,
            status: None,
            input_tokens: None,
            output_tokens: None,
        };
        if self.capacity == 0 {
            return id;
        }
        let mut rows = self.lock_rows();
        if rows.len() >= self.capacity {
            // Terminal-first eviction: reclaim the oldest finished row before
            // ever dropping a live one, so an operator does not lose a slow or
            // hung request to a burst of quick ones. `position` scans oldest to
            // newest, so it finds the oldest terminal row.
            match rows.iter().position(|row| row.state.is_terminal()) {
                Some(index) => {
                    rows.remove(index);
                }
                None => {
                    rows.pop_front();
                }
            }
        }
        rows.push_back(record);
        id
    }

    /// Record the terminal outcome of the row with `id`, in place and exactly
    /// once. Best-effort and non-failing: a non-terminal `state`, an id already
    /// evicted by capacity pressure, or a row that already settled are all
    /// silently ignored, so a stale or duplicate handle can neither fail a
    /// request nor overwrite a recorded outcome. The total `duration` is stamped
    /// here from the row's own start instant.
    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &self,
        id: ActivityId,
        state: ActivityState,
        status: Option<u16>,
        header_latency: Option<Duration>,
        ttft: Option<Duration>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    ) {
        // Only terminal transitions are recorded: `finish` is the single
        // active-to-terminal edge, never an in-flight mutation.
        if !state.is_terminal() {
            return;
        }
        let mut rows = self.lock_rows();
        // Match only a row that is still active, so a duplicate terminal call
        // (e.g. the stream observer's poll path and its Drop both firing) does
        // not overwrite the outcome already recorded.
        if let Some(row) = rows
            .iter_mut()
            .find(|row| row.id == id && !row.state.is_terminal())
        {
            row.state = state;
            row.status = status;
            row.header_latency = header_latency;
            row.ttft = ttft;
            row.duration = Some(row.started_at.elapsed());
            row.input_tokens = input_tokens;
            row.output_tokens = output_tokens;
        }
    }

    /// A best-effort, oldest-first snapshot of every retained row (active and
    /// terminal). Never panics, including behind a poisoned lock.
    pub fn snapshot(&self) -> Vec<ActivityRecord> {
        self.lock_rows().iter().cloned().collect()
    }
}

impl Default for ActivityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_of(store: &ActivityStore, id: ActivityId) -> Option<ActivityState> {
        store
            .snapshot()
            .into_iter()
            .find(|row| row.id == id)
            .map(|row| row.state)
    }

    #[test]
    fn queue_cannot_exceed_capacity_even_when_all_rows_are_active() {
        let store = ActivityStore::with_capacity(3);
        for _ in 0..10 {
            store.start(ActivityProtocol::Messages, "anthropic", "claude");
        }
        assert_eq!(store.snapshot().len(), 3);
    }

    #[test]
    fn eviction_is_oldest_first() {
        let store = ActivityStore::with_capacity(2);
        let first = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        let second = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        let third = store.start(ActivityProtocol::Codex, "openai", "gpt");

        let ids: Vec<ActivityId> = store.snapshot().into_iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![second, third]);
        assert!(!ids.contains(&first));
    }

    #[test]
    fn active_to_terminal_update_happens_in_place() {
        let store = ActivityStore::with_capacity(10);
        let id = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        assert_eq!(state_of(&store, id), Some(ActivityState::Active));

        store.finish(
            id,
            ActivityState::Completed,
            Some(200),
            Some(Duration::from_millis(50)),
            Some(Duration::from_millis(120)),
            Some(10),
            Some(20),
        );

        let rows = store.snapshot();
        assert_eq!(rows.len(), 1, "terminal update must not append a new row");
        let row = &rows[0];
        assert_eq!(row.id, id);
        assert_eq!(row.state, ActivityState::Completed);
        assert_eq!(row.status, Some(200));
        assert_eq!(row.input_tokens, Some(10));
        assert_eq!(row.output_tokens, Some(20));
        assert!(
            row.duration.is_some(),
            "a terminal row must carry a total duration"
        );
    }

    #[test]
    fn eviction_reclaims_the_oldest_terminal_row_before_any_active_one() {
        let store = ActivityStore::with_capacity(3);
        let active_old = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        let terminal = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        let active_new = store.start(ActivityProtocol::Codex, "openai", "gpt");
        store.finish(
            terminal,
            ActivityState::Completed,
            Some(200),
            None,
            None,
            None,
            None,
        );

        // Queue is full (3/3); the incoming row must evict `terminal`, the only
        // finished row, and spare both active rows despite `active_old` being
        // the oldest row overall.
        let incoming = store.start(ActivityProtocol::Messages, "zai", "claude");

        let ids: Vec<ActivityId> = store.snapshot().into_iter().map(|row| row.id).collect();
        assert!(
            !ids.contains(&terminal),
            "oldest terminal row must be evicted"
        );
        assert!(
            ids.contains(&active_old),
            "an active row must not be evicted while a terminal row exists"
        );
        assert!(ids.contains(&active_new));
        assert!(ids.contains(&incoming));
    }

    #[test]
    fn a_zero_capacity_store_retains_nothing_but_still_hands_back_ids() {
        let store = ActivityStore::with_capacity(0);
        let id = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        // The handle is still valid to call `finish` with; it just no-ops.
        store.finish(
            id,
            ActivityState::Completed,
            Some(200),
            None,
            None,
            None,
            None,
        );
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn a_second_terminal_update_does_not_overwrite_the_recorded_outcome() {
        let store = ActivityStore::with_capacity(10);
        let id = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        store.finish(
            id,
            ActivityState::Completed,
            Some(200),
            None,
            None,
            Some(10),
            Some(20),
        );
        // A duplicate terminal call (e.g. the observer's Drop after its poll
        // path already settled the row) must be ignored.
        store.finish(
            id,
            ActivityState::ClientDisconnect,
            Some(499),
            None,
            None,
            Some(1),
            Some(1),
        );

        let row = store
            .snapshot()
            .into_iter()
            .find(|row| row.id == id)
            .unwrap();
        assert_eq!(row.state, ActivityState::Completed);
        assert_eq!(row.status, Some(200));
        assert_eq!(row.output_tokens, Some(20));
    }

    #[test]
    fn a_non_terminal_finish_state_is_ignored() {
        let store = ActivityStore::with_capacity(10);
        let id = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        store.finish(id, ActivityState::Active, Some(200), None, None, None, None);

        let row = store
            .snapshot()
            .into_iter()
            .find(|row| row.id == id)
            .unwrap();
        assert_eq!(row.state, ActivityState::Active);
        assert!(
            row.status.is_none(),
            "a non-terminal finish must not mutate the row"
        );
        assert!(row.duration.is_none());
    }

    #[test]
    fn finishing_an_evicted_id_is_a_silent_no_op() {
        let store = ActivityStore::with_capacity(1);
        let evicted = store.start(ActivityProtocol::Messages, "anthropic", "claude");
        let _current = store.start(ActivityProtocol::Messages, "anthropic", "claude");

        // Must not panic, and must not resurrect the evicted row.
        store.finish(
            evicted,
            ActivityState::Completed,
            Some(200),
            None,
            None,
            None,
            None,
        );

        assert_eq!(store.snapshot().len(), 1);
        assert!(store.snapshot().into_iter().all(|row| row.id != evicted));
    }

    #[test]
    fn provider_and_model_labels_are_capped_utf8_safely() {
        // Multi-byte characters so a naive byte-index cap would split a
        // character and either panic or produce invalid UTF-8.
        let long_provider: String = "\u{1F600}".repeat(100); // 4-byte emoji * 100
        let long_model: String = "\u{00e9}".repeat(200); // 2-byte char * 200

        let store = ActivityStore::with_capacity(10);
        let id = store.start(ActivityProtocol::Messages, &long_provider, &long_model);

        let rows = store.snapshot();
        let row = rows.into_iter().find(|row| row.id == id).unwrap();
        assert!(row.provider.len() <= MAX_LABEL_BYTES);
        assert!(row.model.len() <= MAX_LABEL_BYTES);
        // The capped string must still be valid UTF-8 (guaranteed by `String`,
        // but assert non-empty to prove capping did not degenerate to "").
        assert!(!row.provider.is_empty());
        assert!(!row.model.is_empty());
    }

    #[test]
    fn cap_utf8_never_panics_on_a_multibyte_boundary() {
        let s = "é".repeat(10); // each 'é' is 2 bytes; odd byte cap forces a boundary search
        let capped = cap_utf8(&s, 5);
        assert!(capped.len() <= 5);
        assert!(s.starts_with(&capped));
    }

    #[test]
    fn recording_operations_survive_a_poisoned_lock() {
        let store = std::sync::Arc::new(ActivityStore::with_capacity(10));
        let id = store.start(ActivityProtocol::Messages, "anthropic", "claude");

        // Poison the mutex by panicking while holding the lock, from another
        // thread, mirroring how a poisoned lock could occur in production.
        let poison_store = store.clone();
        let result = std::thread::spawn(move || {
            let _guard = poison_store.rows.lock().unwrap();
            panic!("intentionally poisoning the activity lock for the test");
        })
        .join();
        assert!(result.is_err(), "the spawned thread must have panicked");

        // None of these may panic despite the poisoned lock.
        store.finish(
            id,
            ActivityState::Completed,
            Some(200),
            None,
            None,
            None,
            None,
        );
        let snapshot = store.snapshot();
        let new_id = store.start(ActivityProtocol::Codex, "openai", "gpt");

        assert!(snapshot.iter().any(|row| row.id == id));
        assert!(store.snapshot().iter().any(|row| row.id == new_id));
    }

    #[test]
    fn activity_state_is_terminal_classifies_correctly() {
        assert!(!ActivityState::Active.is_terminal());
        assert!(ActivityState::Completed.is_terminal());
        assert!(ActivityState::Error.is_terminal());
        assert!(ActivityState::UpstreamCut.is_terminal());
        assert!(ActivityState::ClientDisconnect.is_terminal());
    }
}
