use std::process::Child;
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Default)]
pub enum RecorderStatus {
    #[default]
    Idle,
    Running(RecorderProcess),
}

impl RecorderStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }
}

pub struct RecorderProcess {
    pub child: Child,
    pub stdout_join: Option<JoinHandle<()>>,
    pub stderr_join: Option<JoinHandle<()>>,
    pub started_at: Instant,
    pub output_file: String,
}

impl RecorderProcess {
    pub fn finish(&mut self) {
        if let Some(handle) = self.stdout_join.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_join.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
pub struct LogEntry {
    pub source: LogSource,
    pub line: String,
}

#[derive(Clone, Copy)]
pub enum LogSource {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputChoice {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioDeviceKind {
    Speaker,
    Microphone,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
    pub kind: AudioDeviceKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowChoice {
    pub id: String,
    pub label: String,
    pub geometry: String,
}
