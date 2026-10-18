use serde::Serialize;

/// Which stream a log line came from, so the UI can style stderr differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepInfo {
    /// Stable identifier, used by the UI to pick an icon.
    pub id: String,
    /// 1-based position of this step.
    pub index: usize,
    pub total: usize,
    /// Human-readable title, already localised by the caller.
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressInfo {
    /// Overall completion, 0.0..=1.0. `None` when the step cannot estimate.
    pub percent: Option<f32>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub stream: Stream,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishInfo {
    pub success: bool,
    pub message: String,
}

/// What the installation engine reports while it works.
///
/// Serialised with an internal tag so the TypeScript side can discriminate on
/// `event` and read the payload from `data`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum InstallEvent {
    Step(StepInfo),
    Progress(ProgressInfo),
    Log(LogLine),
    Finished(FinishInfo),
}

/// Where events go. The Tauri app forwards them to a `Channel`; the CLI prints
/// them; tests drop them.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: InstallEvent);
}

/// Discards every event. Useful in tests and for dry runs.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: InstallEvent) {}
}

/// Writes events to stdout as human-readable lines. Used by the `noos-install`
/// CLI so the engine can be exercised without a graphical session.
pub struct StdoutSink;

impl EventSink for StdoutSink {
    fn emit(&self, event: InstallEvent) {
        match event {
            InstallEvent::Step(step) => {
                println!("\n[{}/{}] {}", step.index, step.total, step.title);
            }
            InstallEvent::Progress(p) => match p.percent {
                Some(pct) => println!("  {:>5.1}%  {}", pct * 100.0, p.message),
                None => println!("  ....   {}", p.message),
            },
            InstallEvent::Log(log) => {
                let prefix = match log.stream {
                    Stream::Stdout => "  | ",
                    Stream::Stderr => "  ! ",
                };
                println!("{prefix}{}", log.line);
            }
            InstallEvent::Finished(f) => {
                println!("\n{} {}", if f.success { "OK" } else { "ECHEC" }, f.message);
            }
        }
    }
}
