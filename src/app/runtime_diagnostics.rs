use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::events::{AppEvent, AppInput, DrawSource};

const REPORT_LIMIT: usize = 16 * 1024;
const DRAW_BUCKETS: [u64; 8] = [1, 5, 10, 25, 50, 100, u64::MAX, u64::MAX];
const CATEGORY_NAMES: [&str; 8] = [
    "terminal",
    "draw_log_triggered",
    "message",
    "media_avatar",
    "presence",
    "receipt",
    "community_readiness",
    "other",
];
const PHASE_NAMES: [&str; 14] = [
    "contacts_chat_projection_rows",
    "message_list_render_layout",
    "community_detail_list",
    "avatar_scheduling",
    "read_receipt_observation_dispatch",
    "composer_submit_send",
    "message_ingestion_db",
    "other",
    "message_assembly",
    "message_preparation",
    "message_selection_reconciliation",
    "message_viewport_total",
    "message_pending_tail",
    "message_overlays",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    ContactsChatProjectionRows,
    MessageListRenderLayout,
    CommunityDetailList,
    AvatarScheduling,
    ReadReceiptObservationDispatch,
    ComposerSubmitSend,
    MessageIngestionDb,
    Other,
    MessageAssembly,
    MessagePreparation,
    MessageSelectionReconciliation,
    MessageViewportTotal,
    MessagePendingTail,
    MessageOverlays,
}

impl Phase {
    const fn index(self) -> usize {
        match self {
            Self::ContactsChatProjectionRows => 0,
            Self::MessageListRenderLayout => 1,
            Self::CommunityDetailList => 2,
            Self::AvatarScheduling => 3,
            Self::ReadReceiptObservationDispatch => 4,
            Self::ComposerSubmitSend => 5,
            Self::MessageIngestionDb => 6,
            Self::Other => 7,
            Self::MessageAssembly => 8,
            Self::MessagePreparation => 9,
            Self::MessageSelectionReconciliation => 10,
            Self::MessageViewportTotal => 11,
            Self::MessagePendingTail => 12,
            Self::MessageOverlays => 13,
        }
    }
}

struct Counters {
    started_us: u64,
    events: [u64; 8],
    draws_should: u64,
    draws_actual: u64,
    draws_go_log: u64,
    draws_go_log_suppressed: u64,
    draws_ordinary: u64,
    draw_count: u64,
    draw_min_us: u64,
    draw_max_us: u64,
    draw_total_us: u128,
    draw_histogram: [u64; 8],
    phases: [PhaseCounters; 14],
    message_counts: MessageListCounts,
    send_sequences: [u64; 5],
    chat_view_rebuilds: u64,
    chat_view_cache_hits: u64,
    chat_view_rebuild_total_us: u128,
}

#[derive(Clone, Copy, Default)]
pub struct MessageListCounts {
    pub canonical_messages_cloned: u64,
    pub author_groups_built: u64,
    pub height_measurements: u64,
    pub height_cache_retained_count: u64,
    pub visible_rows: u64,
    pub temporary_buffer_rows: u64,
    pub media_rows: u64,
    pub receipt_candidates: u64,
    pub pending_candidates: u64,
    pub pending_rows_rendered: u64,
}

#[derive(Clone, Copy)]
struct PhaseCounters {
    count: u64,
    total_us: u128,
    min_us: u64,
    max_us: u64,
}

impl Default for PhaseCounters {
    fn default() -> Self {
        Self {
            count: 0,
            total_us: 0,
            min_us: u64::MAX,
            max_us: 0,
        }
    }
}

pub struct RuntimeDiagnostics {
    state: Option<Counters>,
    report_path: Option<PathBuf>,
    clock: Option<Box<dyn PerfClock>>,
    finalized: bool,
}

pub trait PerfEnvironment {
    fn value(&self, name: &str) -> Option<String>;
}

struct ProcessEnvironment;

impl PerfEnvironment for ProcessEnvironment {
    fn value(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

pub trait PerfClock {
    fn now_us(&self) -> u64;
}

struct MonotonicClock {
    started: Instant,
}

impl MonotonicClock {
    fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl PerfClock for MonotonicClock {
    fn now_us(&self) -> u64 {
        self.started.elapsed().as_micros().min(u64::MAX as u128) as u64
    }
}

impl RuntimeDiagnostics {
    pub fn from_environment(cache_dir: &Path) -> Self {
        let environment = ProcessEnvironment;
        Self::from_environment_with(&environment, cache_dir, || Box::new(MonotonicClock::new()))
    }

    pub fn from_environment_with(
        environment: &dyn PerfEnvironment,
        cache_dir: &Path,
        clock: impl FnOnce() -> Box<dyn PerfClock>,
    ) -> Self {
        if environment.value("WPTUI_PERF").as_deref() != Some("1") {
            return Self {
                state: None,
                report_path: None,
                clock: None,
                finalized: false,
            };
        }
        let clock = clock();
        Self {
            state: Some(Counters {
                started_us: clock.now_us(),
                events: [0; 8],
                draws_should: 0,
                draws_actual: 0,
                draws_go_log: 0,
                draws_go_log_suppressed: 0,
                draws_ordinary: 0,
                draw_count: 0,
                draw_min_us: u64::MAX,
                draw_max_us: 0,
                draw_total_us: 0,
                draw_histogram: [0; 8],
                phases: [PhaseCounters::default(); 14],
                message_counts: MessageListCounts::default(),
                send_sequences: [0; 5],
                chat_view_rebuilds: 0,
                chat_view_cache_hits: 0,
                chat_view_rebuild_total_us: 0,
            }),
            report_path: Some(cache_dir.join("perf-report.txt")),
            clock: Some(clock),
            finalized: false,
        }
    }

    pub fn enabled(&self) -> bool {
        self.state.is_some()
    }

    pub fn record_input(&mut self, input: &AppInput) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let category = match input {
            AppInput::Terminal(_) => 0,
            AppInput::Draw(_) => 1,
            AppInput::Message { .. } => 2,
            AppInput::App(event) => match event {
                AppEvent::OptimisticTextSent { .. } | AppEvent::TextSendFailed { .. } => 5,
                AppEvent::ContactAvatar(_)
                | AppEvent::ContactAvatarRefreshed { .. }
                | AppEvent::DownloadFile(_, _)
                | AppEvent::DownloadFileDone(_, _)
                | AppEvent::LoadFilePreview(_)
                | AppEvent::SetFilePreview(_, _, _)
                | AppEvent::LoadViewerPreview(_)
                | AppEvent::SetViewerPreview(_, _)
                | AppEvent::SetFileState(_, _)
                | AppEvent::SetAudioDuration(_, _, _) => 3,
                AppEvent::ReadReceiptResult(_, _)
                | AppEvent::ReadReceiptRestored(_)
                | AppEvent::ReadReceiptPersisted(_, _)
                | AppEvent::ReadReceiptCompleted(_, _)
                | AppEvent::ReadReceiptRejected(_, _) => 5,
            },
            AppInput::Presence(_) => 4,
            AppInput::WhatsApp(event) => match event {
                whatsrust::Event::AppStateSyncComplete
                | whatsrust::Event::Connected
                | whatsrust::Event::SyncProgress(_) => 6,
                whatsrust::Event::Receipt { .. } => 5,
                whatsrust::Event::Chat { .. }
                | whatsrust::Event::Reaction { .. }
                | whatsrust::Event::MessageAction { .. }
                | whatsrust::Event::LogoutResult(_) => 7,
            },
        };
        state.events[category] = state.events[category].saturating_add(1);
    }

    pub fn record_should_draw(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.draws_should = state.draws_should.saturating_add(1);
        }
    }

    pub fn record_draw_source(&mut self, source: DrawSource) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match source {
            DrawSource::GoLog => state.draws_go_log = state.draws_go_log.saturating_add(1),
            DrawSource::Ordinary => state.draws_ordinary = state.draws_ordinary.saturating_add(1),
        }
    }

    pub fn record_go_log_draw_suppressed(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.draws_go_log_suppressed = state.draws_go_log_suppressed.saturating_add(1);
        }
    }

    pub fn draw_started(&mut self) -> Option<u64> {
        self.clock.as_ref().map(|clock| clock.now_us())
    }

    pub fn record_draw_finished(&mut self, started_us: u64) {
        let Some(now_us) = self.clock.as_ref().map(|clock| clock.now_us()) else {
            return;
        };
        self.record_draw_duration(Duration::from_micros(now_us.saturating_sub(started_us)));
    }

    pub fn record_draw_duration(&mut self, duration: Duration) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let us = duration.as_micros().min(u64::MAX as u128) as u64;
        state.draws_actual = state.draws_actual.saturating_add(1);
        state.draw_count = state.draw_count.saturating_add(1);
        state.draw_min_us = state.draw_min_us.min(us);
        state.draw_max_us = state.draw_max_us.max(us);
        state.draw_total_us = state.draw_total_us.saturating_add(us as u128);
        let bucket = DRAW_BUCKETS
            .iter()
            .position(|limit| us <= *limit)
            .unwrap_or(7);
        state.draw_histogram[bucket] = state.draw_histogram[bucket].saturating_add(1);
    }

    pub fn phase_started(&mut self) -> Option<u64> {
        self.clock.as_ref().map(|clock| clock.now_us())
    }

    pub fn record_phase_finished(&mut self, phase: Phase, started_us: u64) {
        let Some(now_us) = self.clock.as_ref().map(|clock| clock.now_us()) else {
            return;
        };
        self.record_phase(
            phase,
            Duration::from_micros(now_us.saturating_sub(started_us)),
        );
    }

    pub fn record_phase(&mut self, phase: Phase, duration: Duration) {
        let Some(counters) = self
            .state
            .as_mut()
            .map(|state| &mut state.phases[phase.index()])
        else {
            return;
        };
        let us = duration.as_micros().min(u64::MAX as u128) as u64;
        counters.count = counters.count.saturating_add(1);
        counters.total_us = counters.total_us.saturating_add(us as u128);
        counters.min_us = counters.min_us.min(us);
        counters.max_us = counters.max_us.max(us);
    }

    pub fn record_message_list_counts(&mut self, counts: MessageListCounts) {
        let Some(total) = self.state.as_mut().map(|state| &mut state.message_counts) else {
            return;
        };
        total.canonical_messages_cloned = total
            .canonical_messages_cloned
            .saturating_add(counts.canonical_messages_cloned);
        total.author_groups_built = total
            .author_groups_built
            .saturating_add(counts.author_groups_built);
        total.height_measurements = total
            .height_measurements
            .saturating_add(counts.height_measurements);
        total.height_cache_retained_count = total
            .height_cache_retained_count
            .saturating_add(counts.height_cache_retained_count);
        total.visible_rows = total.visible_rows.saturating_add(counts.visible_rows);
        total.temporary_buffer_rows = total
            .temporary_buffer_rows
            .saturating_add(counts.temporary_buffer_rows);
        total.media_rows = total.media_rows.saturating_add(counts.media_rows);
        total.receipt_candidates = total
            .receipt_candidates
            .saturating_add(counts.receipt_candidates);
        total.pending_candidates = total
            .pending_candidates
            .saturating_add(counts.pending_candidates);
        total.pending_rows_rendered = total
            .pending_rows_rendered
            .saturating_add(counts.pending_rows_rendered);
    }

    pub fn record_send_sequence(&mut self, messages: usize) {
        if let Some(state) = self.state.as_mut() {
            let bucket = messages.min(4);
            state.send_sequences[bucket] = state.send_sequences[bucket].saturating_add(1);
        }
    }

    pub fn record_chat_view_rebuild(&mut self, duration: Duration) {
        if let Some(state) = self.state.as_mut() {
            state.chat_view_rebuilds = state.chat_view_rebuilds.saturating_add(1);
            state.chat_view_rebuild_total_us = state
                .chat_view_rebuild_total_us
                .saturating_add(duration.as_micros() as u128);
        }
    }

    pub fn record_chat_view_cache_hit(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.chat_view_cache_hits = state.chat_view_cache_hits.saturating_add(1);
        }
    }

    pub fn chat_view_counts(&self) -> (u64, u64) {
        self.state.as_ref().map_or((0, 0), |state| {
            (state.chat_view_rebuilds, state.chat_view_cache_hits)
        })
    }

    pub fn finalize(&mut self) -> io::Result<()> {
        if self.finalized {
            return Ok(());
        }
        let (Some(state), Some(path)) = (self.state.as_ref(), self.report_path.as_ref()) else {
            return Ok(());
        };
        let report = self.report(state);
        write_atomic_report(path, &report)?;
        self.finalized = true;
        Ok(())
    }

    fn report(&self, state: &Counters) -> String {
        let mut out = String::new();
        let duration_ms = self.clock.as_ref().map_or(0, |clock| {
            clock.now_us().saturating_sub(state.started_us) / 1000
        });
        writeln!(out, "wptui runtime performance report").unwrap();
        writeln!(out, "format=4").unwrap();
        writeln!(out, "build_source=package-{}", env!("CARGO_PKG_VERSION")).unwrap();
        writeln!(out, "run_duration_ms={duration_ms}").unwrap();
        writeln!(out, "events:").unwrap();
        for (name, count) in CATEGORY_NAMES.iter().zip(state.events) {
            writeln!(out, "  {name}={count}").unwrap();
        }
        writeln!(out, "draws_should={}", state.draws_should).unwrap();
        writeln!(out, "draws_actual={}", state.draws_actual).unwrap();
        writeln!(out, "draws_go_log={}", state.draws_go_log).unwrap();
        writeln!(
            out,
            "draws_go_log_suppressed={}",
            state.draws_go_log_suppressed
        )
        .unwrap();
        writeln!(out, "draws_ordinary={}", state.draws_ordinary).unwrap();
        let mean = if state.draw_count == 0 {
            0
        } else {
            (state.draw_total_us / state.draw_count as u128) as u64
        };
        writeln!(
            out,
            "draw_duration_us=count:{} min:{} max:{} mean:{}",
            state.draw_count,
            if state.draw_count == 0 {
                0
            } else {
                state.draw_min_us
            },
            state.draw_max_us,
            mean
        )
        .unwrap();
        writeln!(
            out,
            "draw_histogram_us=0-1:{},2-5:{},6-10:{},11-25:{},26-50:{},51-100:{},101+:{}",
            state.draw_histogram[0],
            state.draw_histogram[1],
            state.draw_histogram[2],
            state.draw_histogram[3],
            state.draw_histogram[4],
            state.draw_histogram[5],
            state.draw_histogram[6] + state.draw_histogram[7]
        )
        .unwrap();
        writeln!(out, "phases:").unwrap();
        for (name, counters) in PHASE_NAMES.iter().zip(state.phases) {
            let mean = if counters.count == 0 {
                0
            } else {
                (counters.total_us / counters.count as u128) as u64
            };
            writeln!(
                out,
                "  {name}=count:{} min:{} max:{} mean:{}",
                counters.count,
                if counters.count == 0 {
                    0
                } else {
                    counters.min_us
                },
                counters.max_us,
                mean
            )
            .unwrap();
        }
        writeln!(out, "message_list_counts:").unwrap();
        let counts = state.message_counts;
        writeln!(
            out,
            "  canonical_messages_cloned={}",
            counts.canonical_messages_cloned
        )
        .unwrap();
        writeln!(out, "  author_groups_built={}", counts.author_groups_built).unwrap();
        writeln!(out, "  height_measurements={}", counts.height_measurements).unwrap();
        writeln!(
            out,
            "  height_cache_retained_count={}",
            counts.height_cache_retained_count
        )
        .unwrap();
        writeln!(out, "  visible_rows={}", counts.visible_rows).unwrap();
        writeln!(
            out,
            "  temporary_buffer_rows={}",
            counts.temporary_buffer_rows
        )
        .unwrap();
        writeln!(out, "  media_rows={}", counts.media_rows).unwrap();
        writeln!(out, "  receipt_candidates={}", counts.receipt_candidates).unwrap();
        writeln!(out, "  pending_candidates={}", counts.pending_candidates).unwrap();
        writeln!(
            out,
            "  pending_rows_rendered={}",
            counts.pending_rows_rendered
        )
        .unwrap();
        writeln!(
            out,
            "send_sequences=0:{},1:{},2:{},3:{},4+:{}",
            state.send_sequences[0],
            state.send_sequences[1],
            state.send_sequences[2],
            state.send_sequences[3],
            state.send_sequences[4]
        )
        .unwrap();
        writeln!(
            out,
            "chat_view_rebuild=count:{} total_us:{}",
            state.chat_view_rebuilds, state.chat_view_rebuild_total_us
        )
        .unwrap();
        writeln!(
            out,
            "chat_view_cache_hit=count:{}",
            state.chat_view_cache_hits
        )
        .unwrap();
        out
    }
}

fn write_atomic_report(path: &Path, report: &str) -> io::Result<()> {
    if report.len() >= REPORT_LIMIT {
        let _ = fs::remove_file(path.with_extension("txt.tmp"));
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "perf report exceeded limit",
        ));
    }
    let Some(parent) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "perf report path has no parent",
        ));
    };
    if let Err(error) = fs::create_dir_all(parent) {
        let _ = fs::remove_file(path.with_extension("txt.tmp"));
        return Err(error);
    }
    let temp = path.with_extension("txt.tmp");
    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(report.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

impl Drop for RuntimeDiagnostics {
    fn drop(&mut self) {
        let _ = self.finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct FixedClock(u64);

    impl PerfClock for FixedClock {
        fn now_us(&self) -> u64 {
            self.0
        }
    }

    struct TestEnvironment(Option<&'static str>);

    impl PerfEnvironment for TestEnvironment {
        fn value(&self, name: &str) -> Option<String> {
            (name == "WPTUI_PERF")
                .then(|| self.0.map(str::to_owned))
                .flatten()
        }
    }

    fn profiler(path: Option<PathBuf>) -> RuntimeDiagnostics {
        RuntimeDiagnostics {
            state: Some(Counters {
                started_us: 1_000,
                events: [0; 8],
                draws_should: 0,
                draws_actual: 0,
                draws_go_log: 0,
                draws_go_log_suppressed: 0,
                draws_ordinary: 0,
                draw_count: 0,
                draw_min_us: u64::MAX,
                draw_max_us: 0,
                draw_total_us: 0,
                draw_histogram: [0; 8],
                phases: [PhaseCounters::default(); 14],
                message_counts: MessageListCounts::default(),
                send_sequences: [0; 5],
                chat_view_rebuilds: 0,
                chat_view_cache_hits: 0,
                chat_view_rebuild_total_us: 0,
            }),
            report_path: path,
            clock: Some(Box::new(FixedClock(6_000))),
            finalized: false,
        }
    }

    #[test]
    fn disabled_mode_has_no_state_and_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut profiler = RuntimeDiagnostics {
            state: None,
            report_path: None,
            clock: None,
            finalized: false,
        };
        assert!(!profiler.enabled());
        assert!(profiler.draw_started().is_none());
        profiler.record_draw_source(DrawSource::GoLog);
        profiler.record_go_log_draw_suppressed();
        profiler.finalize().unwrap();
        assert!(!dir.path().join("perf-report.txt").exists());
    }

    #[test]
    fn environment_constructor_is_injected_and_invalid_values_stay_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let disabled = TestEnvironment(Some("0"));
        let profiler = RuntimeDiagnostics::from_environment_with(&disabled, dir.path(), || {
            panic!("disabled construction must not create a clock")
        });
        assert!(!profiler.enabled());
        assert!(profiler.clock.is_none());
        assert!(!dir.path().join("perf-report.txt").exists());
    }

    #[test]
    fn injected_durations_produce_deterministic_counters() {
        let mut profiler = profiler(None);
        profiler.record_draw_duration(Duration::from_micros(2));
        profiler.record_draw_duration(Duration::from_micros(10));
        profiler.record_phase(Phase::Other, Duration::from_micros(4));
        let report = profiler.report(profiler.state.as_ref().unwrap());
        assert!(report.contains("count:2 min:2 max:10 mean:6"));
        assert!(report.contains("0-1:0,2-5:1,6-10:1"));
    }

    #[test]
    fn message_list_report_uses_fixed_subphases_and_counts() {
        let mut profiler = profiler(None);
        for phase in [
            Phase::MessageAssembly,
            Phase::MessagePreparation,
            Phase::MessageSelectionReconciliation,
            Phase::MessageViewportTotal,
            Phase::MessagePendingTail,
            Phase::MessageOverlays,
        ] {
            profiler.record_phase(phase, Duration::from_micros(7));
        }
        profiler.record_message_list_counts(MessageListCounts {
            canonical_messages_cloned: 2,
            author_groups_built: 2,
            height_measurements: 1,
            height_cache_retained_count: 2,
            visible_rows: 1,
            temporary_buffer_rows: 2,
            media_rows: 1,
            receipt_candidates: 1,
            pending_candidates: 1,
            pending_rows_rendered: 1,
        });
        let report = profiler.report(profiler.state.as_ref().unwrap());
        assert!(report.len() < REPORT_LIMIT);
        assert_eq!(report.matches("count:1 min:7 max:7 mean:7").count(), 6);
        for phase in [
            "message_assembly",
            "message_preparation",
            "message_selection_reconciliation",
            "message_viewport_total",
            "message_pending_tail",
            "message_overlays",
        ] {
            assert!(report.contains(&format!("{phase}=count:1 min:7 max:7 mean:7")));
        }
        assert!(report.contains("message_list_counts:"));
        assert!(report.contains("canonical_messages_cloned=2"));
        assert!(report.contains("receipt_candidates=1"));
        assert!(report.contains("pending_rows_rendered=1"));
    }

    #[test]
    fn finalize_replaces_atomically_and_bounds_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("perf-report.txt");
        let environment = TestEnvironment(Some("1"));
        let mut profiler =
            RuntimeDiagnostics::from_environment_with(&environment, dir.path(), || {
                Box::new(FixedClock(1_000))
            });
        profiler.report_path = Some(path.clone());
        profiler.finalize().unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.len() < REPORT_LIMIT);
        assert!(!text.contains("secret"));
    }

    #[test]
    fn go_log_draws_are_classified_without_affecting_should_draw() {
        let mut profiler = profiler(None);
        profiler.record_input(&AppInput::Draw(DrawSource::GoLog));
        profiler.record_draw_source(DrawSource::GoLog);
        profiler.record_go_log_draw_suppressed();
        profiler.record_input(&AppInput::Draw(DrawSource::Ordinary));
        profiler.record_draw_source(DrawSource::Ordinary);
        profiler.record_should_draw();
        profiler.record_draw_duration(Duration::from_micros(1));
        let report = profiler.report(profiler.state.as_ref().unwrap());
        assert!(report.contains("draw_log_triggered=2"));
        assert!(report.contains("draws_go_log=1"));
        assert!(report.contains("draws_go_log_suppressed=1"));
        assert!(report.contains("draws_ordinary=1"));
        assert!(report.contains("draws_should=1"));
        assert!(report.contains("draws_actual=1"));
    }

    #[test]
    fn injected_clock_makes_identical_reports_stable_in_schema_and_values() {
        let mut left = profiler(None);
        let mut right = profiler(None);
        for current in [&mut left, &mut right] {
            current.record_input(&AppInput::Draw(DrawSource::GoLog));
            current.record_draw_source(DrawSource::GoLog);
            current.record_input(&AppInput::Draw(DrawSource::Ordinary));
            current.record_draw_source(DrawSource::Ordinary);
            current.record_should_draw();
            current.record_draw_duration(Duration::from_micros(25));
            current.record_send_sequence(2);
        }
        assert_eq!(
            left.report(left.state.as_ref().unwrap()),
            right.report(right.state.as_ref().unwrap())
        );
        assert!(
            left.report(left.state.as_ref().unwrap())
                .contains("run_duration_ms=5")
        );
    }

    #[test]
    fn oversized_reports_are_rejected_strictly_and_temp_files_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("perf-report.txt");
        let temp = path.with_extension("txt.tmp");
        fs::write(&temp, "stale").unwrap();
        let result = write_atomic_report(&path, &"x".repeat(REPORT_LIMIT));
        assert!(result.is_err());
        assert!(!path.exists());
        assert!(!temp.exists());
    }

    #[test]
    fn failed_finalize_can_retry_and_successful_finalize_is_not_repeated_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let blocked_parent = dir.path().join("blocked");
        fs::write(&blocked_parent, "not a directory").unwrap();
        let environment = TestEnvironment(Some("1"));
        let mut profiler =
            RuntimeDiagnostics::from_environment_with(&environment, &blocked_parent, || {
                Box::new(FixedClock(1_000))
            });
        assert!(profiler.finalize().is_err());
        assert!(!profiler.finalized);
        fs::remove_file(&blocked_parent).unwrap();
        fs::create_dir(&blocked_parent).unwrap();
        let report_path = blocked_parent.join("perf-report.txt");
        profiler.finalize().unwrap();
        assert!(profiler.finalized);
        fs::write(&report_path, "sentinel").unwrap();
        drop(profiler);
        assert_eq!(fs::read_to_string(report_path).unwrap(), "sentinel");
        assert!(!blocked_parent.join("perf-report.txt.tmp").exists());
    }
}
