use chrono::Local;
use std::io::{BufRead, BufReader};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::time::Instant;
use std::{fs, path::Path};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use crate::actions::{SimpleAction, run_simple_command};
use crate::config::RecorderConfig;
use crate::discovery::{detect_audio_devices, detect_outputs, detect_windows};
use crate::models::{
    AudioDevice, LogEntry, LogSource, OutputChoice, RecorderProcess, RecorderStatus, WindowChoice,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Section {
    CaptureBasics,
    VideoEncoding,
    AudioRecording,
    ToolsDiagnostics,
}

#[derive(Clone)]
pub struct ActionOutput {
    pub(super) title: String,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) status_code: Option<i32>,
    pub(super) error_message: Option<String>,
}

pub struct RecorderApp {
    pub(super) config: RecorderConfig,
    pub(super) status: RecorderStatus,
    pub(super) current_section: Section,
    pub(super) log_entries: Arc<Mutex<Vec<LogEntry>>>,
    pub(super) log_buffer: Arc<Mutex<String>>,
    pub(super) log_dirty: Arc<AtomicBool>,
    pub(super) log_display: String,
    pub(super) last_error: Option<String>,
    pub(super) last_action_output: Option<ActionOutput>,
    pub(super) last_recording_summary: Option<String>,
    pub(super) available_outputs: Vec<OutputChoice>,
    pub(super) outputs_loading: bool,
    pub(super) outputs_error: Option<String>,
    pub(super) outputs_receiver: Option<Receiver<Result<Vec<OutputChoice>, String>>>,
    pub(super) available_windows: Vec<WindowChoice>,
    pub(super) windows_loading: bool,
    pub(super) windows_error: Option<String>,
    pub(super) windows_receiver: Option<Receiver<Result<Vec<WindowChoice>, String>>>,
    pub(super) available_audio_devices: Vec<AudioDevice>,
    pub(super) audio_devices_loading: bool,
    pub(super) audio_devices_error: Option<String>,
    pub(super) audio_devices_receiver: Option<Receiver<Result<Vec<AudioDevice>, String>>>,
    pub(super) dark_theme: bool,
    pub(super) sidebar_state: SidebarState,
}

#[derive(Clone, Copy)]
pub(super) enum SidebarState {
    Expanded,
    Compact,
    Hidden,
}

impl SidebarState {
    pub fn next(&self) -> Self {
        match self {
            SidebarState::Expanded => SidebarState::Compact,
            SidebarState::Compact => SidebarState::Hidden,
            SidebarState::Hidden => SidebarState::Expanded,
        }
    }
}

impl RecorderApp {
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_state = self.sidebar_state.next();
    }

    pub fn new() -> Self {
        let mut app = Self {
            config: RecorderConfig::default(),
            status: RecorderStatus::default(),
            current_section: Section::CaptureBasics,
            log_entries: Arc::new(Mutex::new(Vec::new())),
            log_buffer: Arc::new(Mutex::new(String::new())),
            log_dirty: Arc::new(AtomicBool::new(true)),
            log_display: String::new(),
            last_error: None,
            last_action_output: None,
            last_recording_summary: None,
            available_outputs: Vec::new(),
            outputs_loading: false,
            outputs_error: None,
            outputs_receiver: None,
            available_windows: Vec::new(),
            windows_loading: false,
            windows_error: None,
            windows_receiver: None,
            available_audio_devices: Vec::new(),
            audio_devices_loading: false,
            audio_devices_error: None,
            audio_devices_receiver: None,
            dark_theme: true,
            sidebar_state: SidebarState::Expanded,
        };
        app.request_output_refresh();
        app.request_window_refresh();
        app.request_audio_refresh();

        app
    }

    pub(super) fn start_recording(&mut self) {
        let (args, output_file) = match self.config.build_command_args(None) {
            Ok(result) => result,
            Err(err) => {
                self.last_error = Some(err);
                return;
            }
        };

        if let Some(parent) = Path::new(&output_file).parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                self.last_error = Some(format!(
                    "Failed to create output directory {}: {err}",
                    parent.display()
                ));
                return;
            }
        }

        let mut command = Command::new("wf-recorder");
        command.args(&args);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        match command.spawn() {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                let stdout_join = stdout.map(|out| {
                    spawn_reader(
                        out,
                        Arc::clone(&self.log_entries),
                        Arc::clone(&self.log_buffer),
                        Arc::clone(&self.log_dirty),
                        LogSource::Stdout,
                    )
                });
                let stderr_join = stderr.map(|err| {
                    spawn_reader(
                        err,
                        Arc::clone(&self.log_entries),
                        Arc::clone(&self.log_buffer),
                        Arc::clone(&self.log_dirty),
                        LogSource::Stderr,
                    )
                });

                self.status = RecorderStatus::Running(RecorderProcess {
                    child,
                    stdout_join,
                    stderr_join,
                    started_at: Instant::now(),
                    output_file,
                });
                self.last_error = None;
                self.last_recording_summary = None;
            }
            Err(err) => {
                self.last_error = Some(format!("Failed to start wf-recorder: {err}"));
            }
        }
    }

    pub(super) fn stop_recording(&mut self) {
        let mut status = RecorderStatus::Idle;
        std::mem::swap(&mut status, &mut self.status);
        if let RecorderStatus::Running(mut process) = status {
            let pid = process.child.id();
            match request_graceful_stop(pid) {
                Ok(()) => {
                    self.status = RecorderStatus::Running(process);
                }
                Err(err) => {
                    process.finish();
                    self.status = RecorderStatus::Idle;
                    self.last_error = Some(format!("Failed to signal wf-recorder: {err}"));
                }
            }
        } else {
            self.status = status;
        }
    }

    pub(super) fn force_stop_recording(&mut self) {
        let mut status = RecorderStatus::Idle;
        std::mem::swap(&mut status, &mut self.status);
        if let RecorderStatus::Running(mut process) = status {
            if let Err(err) = process.child.kill() {
                self.last_error = Some(format!("Failed to terminate wf-recorder: {err}"));
            }
            process.finish();
        }
        self.status = RecorderStatus::Idle;
    }

    pub(super) fn poll_process(&mut self) {
        let current_status = std::mem::replace(&mut self.status, RecorderStatus::Idle);
        self.status = match current_status {
            RecorderStatus::Running(mut process) => match process.child.try_wait() {
                Ok(Some(status)) => {
                    process.finish();
                    let duration = process.started_at.elapsed();
                    let file_exists = Path::new(&process.output_file).exists();
                    let exit_suffix = format_exit_status(status);
                    let summary = if file_exists {
                        format!(
                            "Saved to {}\nwf-recorder exited after {:.1} seconds{}",
                            process.output_file,
                            duration.as_secs_f32(),
                            exit_suffix
                        )
                    } else {
                        let mut message = format!(
                            "wf-recorder exited after {:.1} seconds{} but no file was created at {}.",
                            duration.as_secs_f32(),
                            exit_suffix,
                            process.output_file
                        );
                        if let Some(log_tail) = self.recent_log_tail(8) {
                            message.push_str("\nRecent wf-recorder output:\n");
                            message.push_str(&log_tail);
                        } else {
                            message.push_str(
                                "\nNo wf-recorder output was captured. Use Tools & Diagnostics → wf-recorder log for details.",
                            );
                        }
                        message
                    };
                    if !file_exists {
                        self.last_error.get_or_insert_with(|| {
                            "Recording did not produce an output file.".to_string()
                        });
                    }
                    self.last_recording_summary = Some(summary);
                    RecorderStatus::Idle
                }
                Ok(None) => RecorderStatus::Running(process),
                Err(err) => {
                    self.last_error = Some(format!("Failed to poll wf-recorder status: {err}"));
                    RecorderStatus::Running(process)
                }
            },
            status => status,
        };
    }

    pub(super) fn invoke_simple_action(&mut self, action: SimpleAction) {
        let args = action.args();
        let title = action.title().to_string();
        match run_simple_command(&args) {
            Ok(simple) => {
                self.last_action_output = Some(ActionOutput {
                    title,
                    stdout: simple.stdout,
                    stderr: simple.stderr,
                    status_code: simple.status_code,
                    error_message: None,
                });
                self.last_error = None;
            }
            Err(err) => {
                self.last_action_output = Some(ActionOutput {
                    title,
                    stdout: String::new(),
                    stderr: String::new(),
                    status_code: None,
                    error_message: Some(err),
                });
            }
        }
    }

    pub(super) fn request_output_refresh(&mut self) {
        if self.outputs_loading {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.outputs_loading = true;
        self.outputs_error = None;
        self.outputs_receiver = Some(rx);
        std::thread::spawn(move || {
            let result = detect_outputs();
            let _ = tx.send(result);
        });
    }

    pub(super) fn request_window_refresh(&mut self) {
        if self.windows_loading {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.windows_loading = true;
        self.windows_error = None;
        self.windows_receiver = Some(rx);
        std::thread::spawn(move || {
            let result = detect_windows();
            let _ = tx.send(result);
        });
    }

    pub(super) fn request_audio_refresh(&mut self) {
        if self.audio_devices_loading {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.audio_devices_loading = true;
        self.audio_devices_error = None;
        self.audio_devices_receiver = Some(rx);
        std::thread::spawn(move || {
            let result = detect_audio_devices();
            let _ = tx.send(result);
        });
    }

    pub(super) fn poll_async_tasks(&mut self) {
        if self.outputs_loading {
            if let Some(receiver) = &self.outputs_receiver {
                match receiver.try_recv() {
                    Ok(Ok(outputs)) => {
                        self.available_outputs = outputs;
                        self.outputs_loading = false;
                        self.outputs_receiver = None;
                        self.outputs_error = None;
                    }
                    Ok(Err(err)) => {
                        self.outputs_error = Some(err);
                        self.outputs_loading = false;
                        self.outputs_receiver = None;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.outputs_loading = false;
                        self.outputs_receiver = None;
                        self.outputs_error =
                            Some("Background task disconnected unexpectedly.".to_string());
                    }
                }
            }
        }

        if self.windows_loading {
            if let Some(receiver) = &self.windows_receiver {
                match receiver.try_recv() {
                    Ok(Ok(windows)) => {
                        self.available_windows = windows;
                        self.windows_loading = false;
                        self.windows_receiver = None;
                        self.windows_error = None;
                    }
                    Ok(Err(err)) => {
                        self.windows_error = Some(err);
                        self.windows_loading = false;
                        self.windows_receiver = None;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.windows_loading = false;
                        self.windows_receiver = None;
                        self.windows_error =
                            Some("Window refresh task disconnected unexpectedly.".to_string());
                    }
                }
            }
        }

        if self.audio_devices_loading {
            if let Some(receiver) = &self.audio_devices_receiver {
                match receiver.try_recv() {
                    Ok(Ok(devices)) => {
                        self.available_audio_devices = devices;
                        self.audio_devices_loading = false;
                        self.audio_devices_receiver = None;
                        self.audio_devices_error = None;
                    }
                    Ok(Err(err)) => {
                        self.audio_devices_error = Some(err);
                        self.audio_devices_loading = false;
                        self.audio_devices_receiver = None;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.audio_devices_loading = false;
                        self.audio_devices_receiver = None;
                        self.audio_devices_error =
                            Some("Audio refresh task disconnected unexpectedly.".to_string());
                    }
                }
            }
        }

        if self.log_dirty.swap(false, Ordering::Relaxed) {
            if let Ok(buffer) = self.log_buffer.lock() {
                self.log_display = buffer.clone();
            }
        }
    }

    pub(super) fn run_slurp_for_geometry(&mut self) {
        match Command::new("slurp").output() {
            Ok(output) => {
                if output.status.success() {
                    let selection = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if selection.is_empty() {
                        self.last_error = Some(
                            "slurp did not return a selection. Try again or type the geometry."
                                .to_string(),
                        );
                    } else {
                        self.config.area_geometry = selection;
                        self.last_error = None;
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let code = output.status.code().unwrap_or(-1);
                    self.last_error =
                        Some(format!("slurp exited with code {code}: {}", stderr.trim()));
                }
            }
            Err(err) => {
                self.last_error = Some(format!(
                    "Failed to run slurp (is it installed and on PATH?): {err}"
                ));
            }
        }
    }

    pub(super) fn build_command_preview(&self) -> Result<String, String> {
        let preview_timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let (args, _) = self.config.build_command_args(Some(preview_timestamp))?;
        Ok(shell_preview(args))
    }

    fn recent_log_tail(&self, lines: usize) -> Option<String> {
        let buffer = self.log_buffer.lock().ok()?;
        if buffer.is_empty() {
            return None;
        }
        let mut collected: Vec<&str> = buffer.lines().rev().take(lines).collect();
        collected.reverse();
        Some(collected.join("\n"))
    }
}

impl Default for RecorderApp {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    log_entries: Arc<Mutex<Vec<LogEntry>>>,
    log_buffer: Arc<Mutex<String>>,
    log_dirty: Arc<AtomicBool>,
    source: LogSource,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match buffer.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\n', '\r']);
                    let new_entry = LogEntry {
                        source,
                        line: trimmed.to_string(),
                    };
                    let mut snapshot: Option<Vec<LogEntry>> = None;
                    if let Ok(mut logs) = log_entries.lock() {
                        logs.push(new_entry.clone());
                        if logs.len() > 2048 {
                            let excess = logs.len() - 2048;
                            logs.drain(0..excess);
                            snapshot = Some(logs.clone());
                        }
                    }
                    if let Ok(mut text) = log_buffer.lock() {
                        if let Some(entries) = snapshot {
                            text.clear();
                            for entry in entries {
                                append_log_line(&mut text, &entry);
                            }
                        } else {
                            append_log_line(&mut text, &new_entry);
                        }
                    }
                    log_dirty.store(true, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
    })
}

fn append_log_line(buffer: &mut String, entry: &LogEntry) {
    if !buffer.is_empty() {
        buffer.push('\n');
    }
    buffer.push_str(match entry.source {
        LogSource::Stdout => "[stdout] ",
        LogSource::Stderr => "[stderr] ",
    });
    buffer.push_str(&entry.line);
}

fn shell_preview(args: Vec<String>) -> String {
    let mut preview = Vec::with_capacity(args.len() + 1);
    preview.push("wf-recorder".to_string());
    preview.extend(args.into_iter().map(shell_escape));
    preview.join(" ")
}

fn shell_escape(arg: String) -> String {
    if arg.is_empty() {
        "''".to_string()
    } else if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:@".contains(c))
    {
        arg
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub(super) fn format_exit_status(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!(", exit code {code}")
    } else {
        #[cfg(unix)]
        {
            if let Some(signal) = status.signal() {
                return format!(
                    ", terminated by signal {} ({})",
                    signal,
                    signal_name(signal)
                );
            }
        }
        #[cfg(not(unix))]
        {
            return ", terminated by signal".to_string();
        }
        #[cfg(unix)]
        ", terminated by signal".to_string()
    }
}

#[cfg(unix)]
pub(super) fn request_graceful_stop(pid: u32) -> std::io::Result<()> {
    let res = unsafe { libc::kill(pid as i32, libc::SIGINT) };
    if res == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
pub(super) fn request_graceful_stop(_pid: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Graceful stop is only supported on Unix platforms",
    ))
}

#[cfg(unix)]
fn signal_name(signal: i32) -> &'static str {
    match signal {
        libc::SIGINT => "SIGINT",
        libc::SIGTERM => "SIGTERM",
        libc::SIGKILL => "SIGKILL",
        libc::SIGHUP => "SIGHUP",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGABRT => "SIGABRT",
        libc::SIGALRM => "SIGALRM",
        _ => "unknown",
    }
}
