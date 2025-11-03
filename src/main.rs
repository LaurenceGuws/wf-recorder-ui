use chrono::Local;
use eframe::{App, Frame, NativeOptions, egui};
use egui::{Color32, RichText, Spinner, TextEdit, Image};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::time::{Duration, Instant};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const COMMON_VIDEO_CODECS: [(&str, &str); 5] = [
    ("H.264 (CPU, libx264)", "libx264"),
    ("H.264 (VAAPI)", "h264_vaapi"),
    ("H.265 / HEVC (VAAPI)", "hevc_vaapi"),
    ("VP9 (libvpx-vp9)", "libvpx-vp9"),
    ("Animated GIF", "gif"),
];

const COMMON_AUDIO_CODECS: [(&str, &str); 3] = [
    ("AAC (aac)", "aac"),
    ("Opus (libopus)", "libopus"),
    ("FLAC", "flac"),
];

const COMMON_AUDIO_BACKENDS: [(&str, &str); 3] = [
    ("PulseAudio / PipeWire", "pulse"),
    ("ALSA", "alsa"),
    ("JACK", "jack"),
];

const COMMON_OUTPUT_FORMATS: [(&str, &str); 5] = [
    ("MP4 (H.264)", "mp4"),
    ("MKV (Matroska)", "mkv"),
    ("WebM (VP9/Opus)", "webm"),
    ("GIF (animated)", "gif"),
    ("MOV (QuickTime)", "mov"),
];

fn main() {
    let native_options = NativeOptions::default();
    if let Err(err) = eframe::run_native(
        "wf-recorder UI",
        native_options,
        Box::new(|_cc| Box::new(RecorderApp::new())),
    ) {
        eprintln!("Failed to start wf-recorder UI: {err}");
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    CaptureBasics,
    VideoEncoding,
    AudioRecording,
    ToolsDiagnostics,
}

struct RecorderApp {
    config: RecorderConfig,
    status: RecorderStatus,
    current_section: Section,
    log_entries: Arc<Mutex<Vec<LogEntry>>>,
    log_buffer: Arc<Mutex<String>>,
    log_dirty: Arc<AtomicBool>,
    log_display: String,
    last_error: Option<String>,
    last_action_output: Option<ActionOutput>,
    last_recording_summary: Option<String>,
    available_outputs: Vec<OutputChoice>,
    outputs_loading: bool,
    outputs_error: Option<String>,
    outputs_receiver: Option<Receiver<Result<Vec<OutputChoice>, String>>>,
    available_windows: Vec<WindowChoice>,
    windows_loading: bool,
    windows_error: Option<String>,
    windows_receiver: Option<Receiver<Result<Vec<WindowChoice>, String>>>,
    available_audio_devices: Vec<AudioDevice>,
    audio_devices_loading: bool,
    audio_devices_error: Option<String>,
    audio_devices_receiver: Option<Receiver<Result<Vec<AudioDevice>, String>>>,
}

impl App for RecorderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.poll_process();
        self.poll_async_tasks();

        if matches!(self.status, RecorderStatus::Running(_)) {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        egui_extras::install_image_loaders(ctx);
        if self.outputs_loading {
            ctx.request_repaint_after(Duration::from_millis(300));
        }

        egui::SidePanel::left("sidebar")
            .resizable(false)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Sections");
                });
                ui.separator();
                ui.add_space(10.0);

                let sections = [
                    (Section::CaptureBasics, "Capture Basics"),
                    (Section::VideoEncoding, "Video Encoding"),
                    (Section::AudioRecording, "Audio Recording"),
                    (Section::ToolsDiagnostics, "Tools & Diagnostics"),
                ];


                // Style the buttons
                ui.style_mut().visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
                ui.style_mut().visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
                ui.style_mut().visuals.widgets.active.rounding = egui::Rounding::same(8.0);
                ui.style_mut().spacing.button_padding = egui::Vec2::new(12.0, 12.0);
                ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(245);
                ui.style_mut().visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(220);
                ui.style_mut().visuals.widgets.active.bg_fill = egui::Color32::from_rgb(50, 100, 200); // Darker blue for better contrast
                ui.style_mut().text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(16.0)); // Make text bigger

                for (i, (section, label)) in sections.iter().enumerate() {
                    let selected = self.current_section == *section;
                    let icon_image = match i {
                        0 => Image::new(egui::ImageSource::Bytes {
                            uri: "bytes://capture.png".into(),
                            bytes: include_bytes!("../assets/icons/png/capture.png").as_slice().into(),
                        }),
                        1 => Image::new(egui::ImageSource::Bytes {
                            uri: "bytes://encoding.png".into(),
                            bytes: include_bytes!("../assets/icons/png/encoding.png").as_slice().into(),
                        }),
                        2 => Image::new(egui::ImageSource::Bytes {
                            uri: "bytes://audio.png".into(),
                            bytes: include_bytes!("../assets/icons/png/audio.png").as_slice().into(),
                        }),
                        3 => Image::new(egui::ImageSource::Bytes {
                            uri: "bytes://tools.png".into(),
                            bytes: include_bytes!("../assets/icons/png/tools.png").as_slice().into(),
                        }),
                        _ => Image::new(egui::ImageSource::Bytes {
                            uri: "bytes://capture.png".into(),
                            bytes: include_bytes!("../assets/icons/png/capture.png").as_slice().into(),
                        }),
                    };
                    let mut button = egui::Button::image_and_text(
                        icon_image.fit_to_exact_size(egui::Vec2::splat(28.0)),
                        *label
                    );
                    if selected {
                        button = button.fill(egui::Color32::from_rgb(50, 100, 200));
                    }
                    if ui.add_sized([ui.available_width(), 50.0], button).clicked() {
                        self.current_section = *section;
                    }
                    ui.add_space(5.0);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.heading("");
            ui.label(
                "Configure wf-recorder with friendly names, sensible defaults, and live feedback.",
            );
            ui.separator();

            self.recording_controls(ui);

            if let Some(err) = &self.last_error {
                ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                ui.separator();
            }

            if let RecorderStatus::Running(process) = &self.status {
                let elapsed = process.started_at.elapsed().as_secs_f32();
                ui.colored_label(
                    Color32::from_rgb(120, 210, 255),
                    format!(
                        "Recording… {:.1}s elapsed. Stop when you’re ready.",
                        elapsed
                    ),
                );
            } else if let Some(summary) = &self.last_recording_summary {
                ui.colored_label(Color32::LIGHT_GREEN, summary);
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    match self.current_section {
                        Section::CaptureBasics => self.general_section(ui),
                        Section::VideoEncoding => self.video_section(ui),
                        Section::AudioRecording => self.audio_section(ui),
                        Section::ToolsDiagnostics => self.advanced_section(ui),
                    }
                    self.action_buttons(ui);
                    self.command_preview(ui);
                    self.log_view(ui);
                });
        });
    }
}


impl RecorderApp {
    fn new() -> Self {
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
        };
        app.request_output_refresh();
        app.request_window_refresh();
        app.request_audio_refresh();

        app
    }

    fn general_section(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        egui::Grid::new("general_grid")
        .num_columns(2)
        .spacing([20.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
                        label_with_help(
                            ui,
                            "File template",
                            "Set -f/--file. Supports $timestamp and $format placeholders.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.file_template)
                                .desired_width(f32::INFINITY)
                                .hint_text("~/Videos/wfrecording/$timestamp.$format"),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "File format",
                            "Replaces $format in the template and determines the default extension.",
                        );
                        let selected_format_label = COMMON_OUTPUT_FORMATS
                            .iter()
                            .find(|(_, value)| *value == self.config.file_format)
                            .map(|(label, _)| (*label).to_string())
                            .unwrap_or_else(|| self.config.file_format.clone());
                        egui::ComboBox::from_id_source("output_format_combo")
                            .selected_text(selected_format_label)
                            .show_ui(ui, |ui| {
                                for (label, value) in COMMON_OUTPUT_FORMATS.iter() {
                                    ui.selectable_value(
                                        &mut self.config.file_format,
                                        (*value).to_string(),
                                        *label,
                                    );
                                }
                            });
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Resolved path",
                            "Shows the file path that will be used when recording starts.",
                        );
                        match self.config.preview_output_file() {
                            Ok(path) => {
                                ui.label(RichText::new(path).monospace());
                            }
                            Err(err) => {
                                ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                            }
                        }
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Capture source",
                            "Choose whether to record an entire display, a specific window, or a custom area.",
                        );
                        let previous_mode = self.config.capture_mode;
                        let mode_label = match self.config.capture_mode {
                            CaptureMode::Screen => "Entire screen",
                            CaptureMode::Window => "Specific window",
                            CaptureMode::Area => "Custom area",
                        };
                        egui::ComboBox::from_id_source("capture_mode_combo")
                            .selected_text(mode_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.config.capture_mode,
                                    CaptureMode::Screen,
                                    "Entire screen",
                                );
                                ui.selectable_value(
                                    &mut self.config.capture_mode,
                                    CaptureMode::Window,
                                    "Specific window",
                                );
                                ui.selectable_value(
                                    &mut self.config.capture_mode,
                                    CaptureMode::Area,
                                    "Custom area",
                                );
                            });
                        if previous_mode != self.config.capture_mode
                            && matches!(self.config.capture_mode, CaptureMode::Window)
                        {
                            self.request_window_refresh();
                        }
                        ui.end_row();

                 match self.config.capture_mode {
                            CaptureMode::Screen => {
                                label_with_help(
                                    ui,
                                    "Screen output",
                                    "Select the output (-o/--output) to capture. Leave on All displays for the compositor default.",
                                );
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let display_label = if self.config.output.is_empty() {
                                            "All displays".to_string()
                                        } else {
                                            self.available_outputs
                                                .iter()
                                                .find(|entry| entry.value == self.config.output)
                                                .map(|entry| entry.label.clone())
                                                .unwrap_or_else(|| self.config.output.clone())
                                        };
                                        let combo = egui::ComboBox::from_id_source("output_combo")
                                            .selected_text(display_label)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut self.config.output,
                                                    String::new(),
                                                    "All displays",
                                                );
                                                for entry in &self.available_outputs {
                                                    let response = ui.selectable_value(
                                                        &mut self.config.output,
                                                        entry.value.clone(),
                                                        entry.label.as_str(),
                                                    );
                                                    response.on_hover_text(
                                                        format!("Name: {}", entry.value),
                                                    );
                                                }
                                            });
                                        combo.response.on_hover_text(
                                            "Options come from `wf-recorder --list-output`. Use Refresh to rescan.",
                                        );
        if self.outputs_loading {
                                            ui.add(Spinner::new());
                                        } else if ui.small_button("Refresh").clicked() {
                                            self.request_output_refresh();
                                        }
                                    });
                                    ui.add(
                                        TextEdit::singleline(&mut self.config.output)
                                            .desired_width(f32::INFINITY)
                                            .hint_text("Custom output name"),
                                    )
                                    .on_hover_text(
                                        "Override the selection if you need to paste an output name manually.",
                                    );
                                });
                                ui.end_row();

                                if !self.config.output.is_empty()
                                    && !self.available_outputs.is_empty()
                                    && !self
                                        .available_outputs
                                        .iter()
                                        .any(|entry| entry.value == self.config.output)
                                {
                                    ui.label("");
                                    ui.colored_label(
                                        Color32::from_rgb(255, 200, 120),
                                        format!(
                                            "Display '{}' is not in the detected list. Refresh to update.",
                                            self.config.output
                                        ),
                                    );
                                    ui.end_row();
                                }

                                if let Some(err) = &self.outputs_error {
                                    ui.label("");
                                    ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                                    ui.end_row();
                                }
                            }
                            CaptureMode::Window => {
                                label_with_help(
                                    ui,
                                    "Window",
                                    "Pick a window. Its geometry will be passed to wf-recorder.",
                                );
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let selected_label = self
                                            .available_windows
                                            .iter()
                                            .find(|entry| entry.id == self.config.selected_window_id)
                                            .map(|entry| entry.label.clone())
                                            .unwrap_or_else(|| "Select a window".to_string());
                                        let combo = egui::ComboBox::from_id_source("window_combo")
                                            .selected_text(selected_label)
                                            .show_ui(ui, |ui| {
                                                for entry in &self.available_windows {
                                                    if ui
                                                        .selectable_value(
                                                            &mut self.config.selected_window_id,
                                                            entry.id.clone(),
                                                            entry.label.as_str(),
                                                        )
                                                        .changed()
                                                     {
                                                         self.config.selected_window_geometry =
                                                             entry.geometry.clone();
                                                     }
                                                 }
                                             });
                                        combo.response.on_hover_text(
                                            "Windows detected via swaymsg/hyprctl. Use Refresh if the list looks stale.",
                                        );
                                        if self.windows_loading {
                                            ui.add(Spinner::new());
                                        } else if ui.small_button("Refresh").clicked() {
                                            self.request_window_refresh();
                                        }
                                    });
                                    if self.available_windows.is_empty() && !self.windows_loading {
                                        ui.label("No windows detected yet. Try Refresh or change workspaces.");
                                    }
                                });
                                ui.end_row();

                                label_with_help(
                                    ui,
                                    "Window geometry",
                                    "Geometry we will pass via -g/--geometry when recording this window.",
                                );
                                if self.config.selected_window_geometry.is_empty() {
                                    ui.label("Select a window to populate its geometry.");
                                } else {
                                    ui.label(
                                        RichText::new(&self.config.selected_window_geometry)
                                            .monospace(),
                                    );
                                }
                                ui.end_row();

                                if let Some(err) = &self.windows_error {
                                    ui.label("");
                                    ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                                    ui.end_row();
                                }
                            }
                            CaptureMode::Area => {
                                label_with_help(
                                    ui,
                                    "Area geometry",
                                    "Fills -g/--geometry. Format: x,y WxH.",
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.config.area_geometry)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("e.g. 100,200 1920x1080"),
                                );
                                ui.end_row();

                                label_with_help(
                                    ui,
                                    "Interactive selector",
                                    "Launch slurp to choose an area (or window) and copy the geometry.",
                                );
                                ui.horizontal(|ui| {
                                    if ui.button("Use slurp").clicked() {
                                        self.run_slurp_for_geometry();
                                    }
                                    ui.label("Requires `slurp` to be installed.");
                                });
                                ui.end_row();
                            }
                        }
                        label_with_help(
                            ui,
                            "File handling",
                            "Adds -y/--overwrite to replace files without asking first.",
                        );
                        ui.checkbox(&mut self.config.overwrite, "Overwrite if the file exists");
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Debug logging",
                            "Adds -l/--log and streams wf-recorder stdout/stderr into the log panel.",
                        );
                         ui.checkbox(&mut self.config.log_enabled, "Show encoder log");
                         ui.end_row();
            });

                 match self.config.capture_mode {
                     CaptureMode::Screen => {
                         if self.available_outputs.is_empty() && !self.outputs_loading {
                             ui.label("No displays detected yet. Use Refresh to try again.");
                         }
                     }
                     CaptureMode::Window => {
                         if self.available_windows.is_empty() && !self.windows_loading {
                             ui.label("No windows detected yet. Use Refresh to rescan.");
                         }
                     }
                      CaptureMode::Area => {}
                  }
    }

    fn video_section(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("video_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                        label_with_help(
                            ui,
                            "Video codec",
                            "Sets -c/--codec. Choose a preset or type a custom encoder name.",
                        );
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let selected = if self.config.codec.is_empty() {
                                    "Auto (wf-recorder default)".to_owned()
                                } else {
                                    self.config.codec.clone()
                                };
                                egui::ComboBox::from_id_source("codec_combo")
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.config.codec,
                                            String::new(),
                                            "Auto (wf-recorder default)",
                                        );
                                        for (label, value) in COMMON_VIDEO_CODECS {
                                            ui.selectable_value(
                                                &mut self.config.codec,
                                                value.to_string(),
                                                label.to_owned(),
                                            );
                                        }
                                    });
                            });
                            ui.add(
                                TextEdit::singleline(&mut self.config.codec)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Custom codec (e.g. libx264)"),
                            );
                        });
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Constant framerate",
                            "Sets -r/--framerate. Leave blank for variable frame rate.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.framerate)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g. 60"),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Max B-frames",
                            "Sets -b/--bframes. Controls encoder lookahead for codecs that support B-frames.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.bframes)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Buffer rate hint",
                            "Sets -B/--buffrate. Hint for encoders expecting a constant FPS.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.buffrate)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "FFmpeg filter chain",
                            "Sets -F/--filter. Apply ffmpeg filters before encoding (e.g. scale filters).",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.filter)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g. scale=1280:720"),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Encoding device",
                            "Sets -d/--device. Choose a VAAPI/ hardware encoder node (e.g. /dev/dri/renderD128).",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.encoding_device)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Pixel format",
                            "Sets -x/--pixel-format. Converts frames before encoding.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.pixel_format)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g. yuv420p"),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Muxer override",
                            "Sets -m/--muxer to pick the container inside ffmpeg explicitly.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.muxer)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g. matroska"),
                        );
                        ui.end_row();

                        render_param_editor(
                            ui,
                            "Codec parameters",
                            "Adds -p/--codec-param entries (format: key=value). Useful for presets like `preset=slow`.",
                            &mut self.config.codec_params,
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "DMA-BUF fallback",
                            "Adds --no-dmabuf to force CPU copies when GPU dma-buf sharing is unreliable.",
                        );
                        ui.checkbox(
                            &mut self.config.no_dmabuf,
                            "Force CPU copy (--no-dmabuf)",
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Damage tracking",
                            "Adds -D/--no-damage to capture every frame for a constant frame rate output.",
                        );
                        ui.checkbox(
                            &mut self.config.no_damage,
                            "Disable damage tracking (--no-damage)",
                        );
                        ui.end_row();
                    });

                match self.config.capture_mode {
                    CaptureMode::Screen => {
                        if self.available_outputs.is_empty() && !self.outputs_loading {
                            ui.label("No displays detected yet. Use Refresh to try again.");
                        }
                    }
                    CaptureMode::Window => {
                        if self.available_windows.is_empty() && !self.windows_loading {
                            ui.label("No windows detected yet. Use Refresh to rescan.");
                        }
                    }
                    CaptureMode::Area => {}
                }
    }

    fn audio_section(&mut self, ui: &mut egui::Ui) {
        let speakers: Vec<AudioDevice> = self
                    .available_audio_devices
                    .iter()
                    .filter(|d| matches!(d.kind, AudioDeviceKind::Speaker))
                    .cloned()
                    .collect();
                let microphones: Vec<AudioDevice> = self
                    .available_audio_devices
                    .iter()
                    .filter(|d| matches!(d.kind, AudioDeviceKind::Microphone))
                    .cloned()
                    .collect();

                let previous_audio_mode = self.config.audio_mode;
                let mut refresh_audio = false;
                ui.horizontal(|ui| {
                    ui.label("Mode");
                    ui.selectable_value(&mut self.config.audio_mode, AudioMode::None, "Muted");
                    ui.selectable_value(&mut self.config.audio_mode, AudioMode::System, "Speakers");
                    ui.selectable_value(
                        &mut self.config.audio_mode,
                        AudioMode::Microphone,
                        "Microphone",
                    );
                    ui.selectable_value(&mut self.config.audio_mode, AudioMode::Both, "Both");
                    if ui.small_button("Refresh devices").clicked() {
                        refresh_audio = true;
                    }
                    if self.audio_devices_loading {
                        ui.add(Spinner::new());
                    }
                });
                if previous_audio_mode != self.config.audio_mode
                    && matches!(
                        self.config.audio_mode,
                        AudioMode::System | AudioMode::Microphone | AudioMode::Both
                    )
                {
                    refresh_audio = true;
                }
                if refresh_audio {
                    self.request_audio_refresh();
                }
                if let Some(err) = &self.audio_devices_error {
                    ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                }
                self.config.audio_enabled = !matches!(self.config.audio_mode, AudioMode::None);

                ui.add_space(6.0);
                match self.config.audio_mode {
                    AudioMode::None => {
                        ui.label("Audio capture is disabled.");
                    }
                    AudioMode::System => {
                        render_audio_device_picker(
                            ui,
                            "Speaker output",
                            speakers.as_slice(),
                            &mut self.config.selected_speaker_device,
                            "No playback devices detected. Try Refresh.",
                        );
                    }
                    AudioMode::Microphone => {
                        render_audio_device_picker(
                            ui,
                            "Microphone",
                            microphones.as_slice(),
                            &mut self.config.selected_microphone_device,
                            "No microphones detected. Try Refresh.",
                        );
                    }
                    AudioMode::Both => {
                        render_audio_device_picker(
                            ui,
                            "Speaker output",
                            speakers.as_slice(),
                            &mut self.config.selected_speaker_device,
                            "No playback devices detected. Try Refresh.",
                        );
                        ui.add_space(4.0);
                        render_audio_device_picker(
                            ui,
                            "Microphone",
                            microphones.as_slice(),
                            &mut self.config.selected_microphone_device,
                            "No microphones detected. Try Refresh.",
                        );
                    }
                }

                ui.add_space(8.0);
                egui::CollapsingHeader::new("Advanced audio options")
                    .default_open(false)
                    .show(ui, |ui| {
                        egui::Grid::new("audio_advanced_grid")
                            .num_columns(2)
                            .spacing([20.0, 8.0])
                            .striped(true)
                            .show(ui, |ui| {
                                label_with_help(
                                    ui,
                                    "Backend",
                                    "Sets --audio-backend. wf-recorder defaults to PulseAudio when available.",
                                );
                                let selected = if self.config.audio_backend.is_empty() {
                                    "Auto (wf-recorder default)".to_owned()
                                } else {
                                    self.config.audio_backend.clone()
                                };
                                egui::ComboBox::from_id_source("audio_backend_combo")
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.config.audio_backend,
                                            String::new(),
                                            "Auto (wf-recorder default)",
                                        );
                                        for (label, value) in COMMON_AUDIO_BACKENDS {
                                            ui.selectable_value(
                                                &mut self.config.audio_backend,
                                                value.to_string(),
                                                label.to_owned(),
                                            );
                                        }
                                    });
                                ui.end_row();

                                label_with_help(
                                    ui,
                                    "Manual device override",
                                    "Optional extra --audio=DEVICE argument. Useful when the dropdowns do not list your sink/source.",
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.config.audio_device)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Leave empty to use the selected devices above"),
                                );
                                ui.end_row();

                                label_with_help(
                                    ui,
                                    "Audio codec",
                                    "Sets -C/--audio-codec. Leave blank for wf-recorder default.",
                                );
                                let selected = if self.config.audio_codec.is_empty() {
                                    "Auto (wf-recorder default)".to_owned()
                                } else {
                                    self.config.audio_codec.clone()
                                };
                                egui::ComboBox::from_id_source("audio_codec_combo")
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.config.audio_codec,
                                            String::new(),
                                            "Auto (wf-recorder default)",
                                        );
                                        for (label, value) in COMMON_AUDIO_CODECS {
                                            ui.selectable_value(
                                                &mut self.config.audio_codec,
                                                value.to_string(),
                                                label.to_owned(),
                                            );
                                        }
                                    });
                                ui.add(
                                    TextEdit::singleline(&mut self.config.audio_codec)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Custom audio codec (e.g. aac)"),
                                );
                                ui.end_row();

                                label_with_help(
                                    ui,
                                    "Sample rate (Hz)",
                                    "Sets -R/--sample-rate. Common values: 48000 or 44100.",
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.config.sample_rate)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("48000"),
                                );
                                ui.end_row();

                                label_with_help(
                                    ui,
                                    "Sample format",
                                    "Sets -X/--sample-format. Use `ffmpeg -sample_fmts` for options.",
                                );
                                ui.add(
                                    TextEdit::singleline(&mut self.config.sample_format)
                                        .desired_width(f32::INFINITY),
                                );
                                ui.end_row();

                                render_param_editor(
                                    ui,
                                    "Audio codec parameters",
                                    "Adds -P/--audio-codec-param entries (format: key=value).",
                                    &mut self.config.audio_codec_params,
                                );
                                ui.end_row();
                            });
                    });
    }

    fn advanced_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Run quick helper commands without leaving the app:");
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button("List displays")
                        .on_hover_text("Runs wf-recorder --list-output and shows the output.")
                        .clicked()
                    {
                        self.invoke_simple_action(SimpleAction::ListOutputs);
                    }
                    if ui
                        .button("Show version")
                        .on_hover_text("Runs wf-recorder --version.")
                        .clicked()
                    {
                        self.invoke_simple_action(SimpleAction::Version);
                    }
                    if ui
                        .button("Open help")
                        .on_hover_text("Runs wf-recorder --help.")
                        .clicked()
                    {
                        self.invoke_simple_action(SimpleAction::Help);
                    }
                });

                if let Some(output) = &self.last_action_output {
                    ui.add_space(8.0);
                    ui.label(RichText::new(&output.title).strong());
                    if let Some(status) = output.status_code {
                        ui.label(format!("Exit code: {status}"));
                    }
                    if !output.stdout.is_empty() {
                        show_readonly_text(ui, "Command stdout", &output.stdout, 8);
                    }
                    if !output.stderr.is_empty() {
                        show_readonly_text(ui, "Command stderr", &output.stderr, 6);
                    }
                    if let Some(err) = &output.error_message {
                        ui.colored_label(Color32::RED, err);
                    }
                }
    }

    fn action_buttons(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Reset to defaults")
                .on_hover_text("Restore the recommended settings in one click.")
                .clicked()
            {
                self.config = RecorderConfig::default();
                self.request_output_refresh();
                self.request_window_refresh();
            }
            if ui
                .button("Clear log")
                .on_hover_text("Remove all captured wf-recorder stdout/stderr lines.")
                .clicked()
            {
                if let Ok(mut logs) = self.log_entries.lock() {
                    logs.clear();
                }
                if let Ok(mut text) = self.log_buffer.lock() {
                    text.clear();
                }
                self.log_display.clear();
                self.log_dirty.store(true, Ordering::Relaxed);
            }
            if ui
                .button("Clear messages")
                .on_hover_text("Dismiss status summaries and helper command output.")
                .clicked()
            {
                self.last_action_output = None;
                self.last_recording_summary = None;
                self.last_error = None;
            }
        });
    }

    fn command_preview(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label("Command preview");
        let preview_timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        match self.config.build_command_args(Some(preview_timestamp)) {
            Ok((args, _path)) => {
                let mut preview = shell_preview(args);
                ui.add(
                    TextEdit::multiline(&mut preview)
                        .code_editor()
                        .desired_rows(2)
                        .interactive(false),
                );
            }
            Err(err) => {
                ui.colored_label(Color32::from_rgb(255, 120, 120), err);
            }
        }
    }

    fn recording_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        let mut start_requested = false;
        let mut stop_requested = false;
        let mut force_requested = false;

        match &self.status {
            RecorderStatus::Idle => {
                let start_button = egui::Button::new("Start recording")
                    .min_size(egui::vec2(ui.available_width().min(320.0), 40.0));
                if ui
                    .add(start_button)
                    .on_hover_text("Launch wf-recorder with the options above.")
                    .clicked()
                {
                    start_requested = true;
                }
            }
            RecorderStatus::Running(process) => {
                let elapsed = process.started_at.elapsed().as_secs_f32();
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(255, 235, 140),
                        format!("Recording in progress ({elapsed:.1}s elapsed)"),
                    );
                    ui.label(format!("Saving to {}", process.output_file));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button("Stop recording")
                        .on_hover_text("Send SIGINT so wf-recorder finishes cleanly.")
                        .clicked()
                    {
                        stop_requested = true;
                    }
                    if ui
                        .button("Force stop")
                        .on_hover_text("Send SIGKILL. Use only if wf-recorder is stuck.")
                        .clicked()
                    {
                        force_requested = true;
                    }
                });
            }
        }

        if start_requested {
            self.start_recording();
        }
        if stop_requested {
            self.stop_recording();
        }
        if force_requested {
            self.force_stop_recording();
        }
    }

    fn log_view(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        egui::CollapsingHeader::new("wf-recorder log")
            .default_open(false)
            .show(ui, |ui| {
                if self.log_dirty.swap(false, Ordering::Relaxed) {
                    if let Ok(buffer) = self.log_buffer.lock() {
                        self.log_display.clone_from(&buffer);
                    }
                }

                if ui
                    .button("Copy log")
                    .on_hover_text("Copy the entire log to the clipboard.")
                    .clicked()
                {
                    ui.output_mut(|output| output.copied_text = self.log_display.clone());
                }

                if self.log_display.is_empty() {
                    ui.label("No log output captured yet.");
                    return;
                }

                let mut edited = false;
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(220.0)
                    .show(ui, |ui| {
                        let response = ui.add(
                            TextEdit::multiline(&mut self.log_display)
                                .code_editor()
                                .desired_rows(10)
                                .desired_width(f32::INFINITY)
                                .interactive(false)
                                .lock_focus(true),
                        );
                        if response.changed() {
                            edited = true;
                        }
                    });
                if edited {
                    self.log_dirty.store(true, Ordering::Relaxed);
                }
            });
    }

    fn start_recording(&mut self) {
        if matches!(self.status, RecorderStatus::Running(_)) {
            return;
        }

        if matches!(self.config.capture_mode, CaptureMode::Screen)
            && !self.config.output.trim().is_empty()
            && !self.available_outputs.is_empty()
            && !self
                .available_outputs
                .iter()
                .any(|entry| entry.value == self.config.output)
        {
            self.last_error = Some(format!(
                "Display '{}' is not available. Click Refresh to update the list.",
                self.config.output
            ));
            return;
        }

        if matches!(self.config.capture_mode, CaptureMode::Screen)
            && self.config.output.trim().is_empty()
        {
            self.last_error = Some("Select a display to record or leave Screen mode.".to_string());
            return;
        }

        if matches!(self.config.capture_mode, CaptureMode::Window)
            && (self.config.selected_window_id.is_empty()
                || self.config.selected_window_geometry.is_empty())
        {
            self.last_error = Some("Pick a window to record before starting.".to_string());
            return;
        }

        let (args, output_file) = match self.config.build_command_args(None) {
            Ok(tuple) => tuple,
            Err(err) => {
                self.last_error = Some(err);
                return;
            }
        };
        if args.iter().any(|arg| arg.trim().is_empty()) {
            self.last_error = Some(
                "Command contains empty argument entries. Please adjust the configuration."
                    .to_string(),
            );
            return;
        }

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
                self.last_error = None;
                self.last_recording_summary = None;
                self.log_entries = Arc::new(Mutex::new(Vec::new()));
                self.log_buffer = Arc::new(Mutex::new(String::new()));
                self.log_dirty = Arc::new(AtomicBool::new(true));
                self.log_display.clear();

                let stdout_reader = child.stdout.take().map(|stdout| {
                    spawn_reader(
                        stdout,
                        self.log_entries.clone(),
                        self.log_buffer.clone(),
                        self.log_dirty.clone(),
                        LogSource::Stdout,
                    )
                });
                let stderr_reader = child.stderr.take().map(|stderr| {
                    spawn_reader(
                        stderr,
                        self.log_entries.clone(),
                        self.log_buffer.clone(),
                        self.log_dirty.clone(),
                        LogSource::Stderr,
                    )
                });
                self.status = RecorderStatus::Running(RecorderProcess {
                    child,
                    stdout_join: stdout_reader,
                    stderr_join: stderr_reader,
                    started_at: Instant::now(),
                    output_file,
                });
            }
            Err(err) => {
                self.last_error = Some(format!("Failed to start wf-recorder: {err}"));
            }
        }
    }

    fn stop_recording(&mut self) {
        let pid = match &self.status {
            RecorderStatus::Running(process) => process.child.id(),
            RecorderStatus::Idle => return,
        };
        match request_graceful_stop(pid) {
            Ok(_) => self.last_error = None,
            Err(err) => {
                self.last_error = Some(format!(
                    "Failed to send SIGINT to wf-recorder: {err}. Attempting force stop."
                ));
                self.force_stop_recording();
            }
        }
    }

    fn force_stop_recording(&mut self) {
        if let RecorderStatus::Running(process) = &mut self.status {
            if let Err(err) = process.child.kill() {
                self.last_error = Some(format!("Failed to force-stop wf-recorder: {err}"));
            }
        }
    }

    fn poll_process(&mut self) {
        let current_status = std::mem::replace(&mut self.status, RecorderStatus::Idle);
        self.status = match current_status {
            RecorderStatus::Running(mut process) => match process.child.try_wait() {
                Ok(Some(status)) => {
                    process.finish();
                    let duration = process.started_at.elapsed();
                    let summary = format!(
                        "Saved to {}\nwf-recorder exited after {:.1} seconds{}",
                        process.output_file,
                        duration.as_secs_f32(),
                        format_exit_status(status)
                    );
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

    fn invoke_simple_action(&mut self, action: SimpleAction) {
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

    fn request_output_refresh(&mut self) {
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

    fn request_window_refresh(&mut self) {
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

    fn request_audio_refresh(&mut self) {
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

    fn poll_async_tasks(&mut self) {
        let maybe_result = if let Some(receiver) = self.outputs_receiver.as_ref() {
            match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Lost connection to display detection task.".to_string()))
                }
            }
        } else {
            None
        };

        if let Some(result) = maybe_result {
            self.outputs_loading = false;
            self.outputs_receiver = None;
            match result {
                Ok(mut outputs) => {
                    outputs.sort_by(|a, b| a.label.cmp(&b.label));
                    outputs.dedup_by(|a, b| a.value == b.value);
                    self.available_outputs = outputs;
                    if self.available_outputs.is_empty() {
                        self.outputs_error =
                            Some("wf-recorder did not report any outputs.".to_string());
                    } else {
                        self.outputs_error = None;
                        if self.config.output.is_empty() {
                            self.config.output = self.available_outputs[0].value.clone();
                        }
                    }
                }
                Err(err) => {
                    self.outputs_error = Some(err);
                }
            }
        }

        let maybe_audio = if let Some(receiver) = self.audio_devices_receiver.as_ref() {
            match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Lost connection to audio detection task.".to_string()))
                }
            }
        } else {
            None
        };

        if let Some(result) = maybe_audio {
            self.audio_devices_loading = false;
            self.audio_devices_receiver = None;
            match result {
                Ok(mut devices) => {
                    devices.sort_by(|a, b| a.description.cmp(&b.description));
                    self.available_audio_devices = devices;

                    if matches!(self.config.audio_mode, AudioMode::System | AudioMode::Both)
                        && self.config.selected_speaker_device.trim().is_empty()
                    {
                        if let Some(device) = self
                            .available_audio_devices
                            .iter()
                            .find(|d| matches!(d.kind, AudioDeviceKind::Speaker))
                        {
                            self.config.selected_speaker_device = device.name.clone();
                        }
                    }

                    if matches!(
                        self.config.audio_mode,
                        AudioMode::Microphone | AudioMode::Both
                    ) && self.config.selected_microphone_device.trim().is_empty()
                    {
                        if let Some(device) = self
                            .available_audio_devices
                            .iter()
                            .find(|d| matches!(d.kind, AudioDeviceKind::Microphone))
                        {
                            self.config.selected_microphone_device = device.name.clone();
                        }
                    }

                    if self.available_audio_devices.is_empty() {
                        if matches!(self.config.audio_mode, AudioMode::None) {
                            self.audio_devices_error = None;
                        } else {
                            self.audio_devices_error = Some(
                                "Could not discover any PulseAudio / PipeWire sources.".to_string(),
                            );
                        }
                    } else {
                        self.audio_devices_error = None;
                    }
                }
                Err(err) => {
                    self.audio_devices_error = Some(err);
                    self.available_audio_devices.clear();
                }
            }
        }

        let maybe_windows = if let Some(receiver) = self.windows_receiver.as_ref() {
            match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Lost connection to window detection task.".to_string()))
                }
            }
        } else {
            None
        };

        if let Some(result) = maybe_windows {
            self.windows_loading = false;
            self.windows_receiver = None;
            match result {
                Ok(mut windows) => {
                    windows.sort_by(|a, b| a.label.cmp(&b.label));
                    windows.dedup_by(|a, b| a.geometry == b.geometry && a.id == b.id);
                    self.available_windows = windows;
                    if !self
                        .available_windows
                        .iter()
                        .any(|entry| entry.id == self.config.selected_window_id)
                    {
                        self.config.selected_window_id.clear();
                        self.config.selected_window_geometry.clear();
                    }
                    if self.available_windows.is_empty() {
                        self.windows_error = Some(
                            "Could not discover any windows via swaymsg or hyprctl.".to_string(),
                        );
                    } else {
                        self.windows_error = None;
                    }
                }
                Err(err) => {
                    self.windows_error = Some(err);
                    self.available_windows.clear();
                    self.config.selected_window_id.clear();
                    self.config.selected_window_geometry.clear();
                }
            }
        }
    }

    fn run_slurp_for_geometry(&mut self) {
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
}

impl Default for RecorderApp {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptureMode {
    Screen,
    Window,
    Area,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AudioMode {
    None,
    System,
    Microphone,
    Both,
}


#[derive(Clone)]
struct RecorderConfig {
    capture_mode: CaptureMode,
    audio_mode: AudioMode,
    audio_enabled: bool,
    audio_device: String,
    audio_backend: String,
    audio_codec: String,
    audio_codec_params: Vec<ParamEntry>,
    sample_rate: String,
    sample_format: String,
    codec: String,
    codec_params: Vec<ParamEntry>,
    framerate: String,
    bframes: String,
    buffrate: String,
    encoding_device: String,
    pixel_format: String,
    muxer: String,
    output: String,
    filter: String,
    area_geometry: String,
    selected_window_id: String,
    selected_window_geometry: String,
    selected_speaker_device: String,
    selected_microphone_device: String,
    file_template: String,
    file_format: String,
    no_dmabuf: bool,
    no_damage: bool,
    log_enabled: bool,
    overwrite: bool,
}

impl RecorderConfig {
    fn build_command_args(
        &self,
        timestamp_override: Option<String>,
    ) -> Result<(Vec<String>, String), String> {
        let mut args = Vec::new();

        let mut audio_targets: Vec<String> = Vec::new();
        match self.audio_mode {
            AudioMode::None => {}
            AudioMode::System => {
                let device = if !self.selected_speaker_device.trim().is_empty() {
                    self.selected_speaker_device.trim().to_string()
                } else if !self.audio_device.trim().is_empty() {
                    self.audio_device.trim().to_string()
                } else {
                    String::new()
                };
                audio_targets.push(device);
            }
            AudioMode::Microphone => {
                let device = if !self.selected_microphone_device.trim().is_empty() {
                    self.selected_microphone_device.trim().to_string()
                } else if !self.audio_device.trim().is_empty() {
                    self.audio_device.trim().to_string()
                } else {
                    String::new()
                };
                audio_targets.push(device);
            }
            AudioMode::Both => {
                let speaker = if !self.selected_speaker_device.trim().is_empty() {
                    self.selected_speaker_device.trim().to_string()
                } else if !self.audio_device.trim().is_empty() {
                    self.audio_device.trim().to_string()
                } else {
                    String::new()
                };
                let microphone = if !self.selected_microphone_device.trim().is_empty() {
                    self.selected_microphone_device.trim().to_string()
                } else {
                    String::new()
                };
                audio_targets.push(speaker);
                audio_targets.push(microphone);
            }
        }

        if !audio_targets.is_empty() {
            for device in audio_targets {
                if device.trim().is_empty() {
                    args.push("--audio".to_string());
                } else {
                    args.push(format!("--audio={}", device.trim()));
                }
            }
            push_arg(&mut args, "--audio-backend", &self.audio_backend);
            push_arg(&mut args, "--audio-codec", &self.audio_codec);
            for entry in &self.audio_codec_params {
                if let Some(combined) = entry.format() {
                    args.push("--audio-codec-param".to_string());
                    args.push(combined);
                }
            }
            push_arg(&mut args, "--sample-rate", &self.sample_rate);
            push_arg(&mut args, "--sample-format", &self.sample_format);
        }

        push_arg(&mut args, "--codec", &self.codec);
        for entry in &self.codec_params {
            if let Some(combined) = entry.format() {
                args.push("--codec-param".to_string());
                args.push(combined);
            }
        }

        push_arg(&mut args, "--framerate", &self.framerate);
        push_arg(&mut args, "--bframes", &self.bframes);
        push_arg(&mut args, "--buffrate", &self.buffrate);
        push_arg(&mut args, "--device", &self.encoding_device);
        push_arg(&mut args, "--pixel-format", &self.pixel_format);
        push_arg(&mut args, "--muxer", &self.muxer);
        push_arg(&mut args, "--filter", &self.filter);

        match self.capture_mode {
            CaptureMode::Screen => {
                push_arg(&mut args, "--output", &self.output);
            }
            CaptureMode::Window => {
                let geometry = self.selected_window_geometry.trim();
                if geometry.is_empty() {
                    return Err(
                        "Select a window from the list before starting the recording.".to_string(),
                    );
                }
                args.push("--geometry".to_string());
                args.push(geometry.to_string());
            }
            CaptureMode::Area => {
                let geometry = self.area_geometry.trim();
                if geometry.is_empty() {
                    return Err(
                        "Enter an area geometry (e.g. 100,200 1920x1080) or use the selector."
                            .to_string(),
                    );
                }
                args.push("--geometry".to_string());
                args.push(geometry.to_string());
            }
        }

        let output_file = self.resolve_output_file_with_timestamp(timestamp_override)?;
        args.push("--file".to_string());
        args.push(output_file.clone());

        if self.no_dmabuf {
            args.push("--no-dmabuf".to_string());
        }
        if self.no_damage {
            args.push("--no-damage".to_string());
        }
        if self.log_enabled {
            args.push("--log".to_string());
        }
        if self.overwrite {
            args.push("--overwrite".to_string());
        }

        Ok((args, output_file))
    }

    fn preview_output_file(&self) -> Result<String, String> {
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        self.resolve_output_file_with_timestamp(Some(timestamp))
    }

    fn resolve_output_file_with_timestamp(
        &self,
        timestamp_override: Option<String>,
    ) -> Result<String, String> {
        let template = self.file_template.trim();
        if template.is_empty() {
            return Err("Please provide a file template".to_string());
        }

        let timestamp = timestamp_override
            .unwrap_or_else(|| Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
        let format = if self.file_format.trim().is_empty() {
            "mp4".to_string()
        } else {
            self.file_format.trim().to_string()
        };

        let mut resolved = template.replace("$timestamp", &timestamp);
        resolved = resolved.replace("$format", &format);

        if let Some(index) = resolved.find('$') {
            return Err(format!(
                "Unknown placeholder starting at position {}. Supported: $timestamp, $format.",
                index
            ));
        }

        Ok(expand_home(resolved))
    }
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            capture_mode: CaptureMode::Screen,
            audio_mode: AudioMode::System,
            audio_enabled: true,
            audio_device: String::new(),
            audio_backend: String::new(),
            audio_codec: String::new(),
            audio_codec_params: Vec::new(),
            sample_rate: "48000".to_string(),
            sample_format: String::new(),
            codec: "libx264".to_string(),
            codec_params: Vec::new(),
            framerate: String::new(),
            bframes: String::new(),
            buffrate: String::new(),
            encoding_device: String::new(),
            pixel_format: String::new(),
            muxer: String::new(),
            output: String::new(),
            filter: String::new(),
            area_geometry: String::new(),
            selected_window_id: String::new(),
            selected_window_geometry: String::new(),
            selected_speaker_device: String::new(),
            selected_microphone_device: String::new(),
            file_template: "~/Videos/wfrecording/$timestamp.$format".to_string(),
            file_format: "mp4".to_string(),
            no_dmabuf: false,
            no_damage: false,
            log_enabled: true,
            overwrite: false,
        }
    }
}

#[derive(Default, Clone)]
struct ParamEntry {
    key: String,
    value: String,
}

impl ParamEntry {
    fn format(&self) -> Option<String> {
        let key = self.key.trim();
        let value = self.value.trim();
        if key.is_empty() && value.is_empty() {
            None
        } else if key.is_empty() {
            Some(value.to_string())
        } else if value.is_empty() {
            Some(key.to_string())
        } else {
            Some(format!("{}={}", key, value))
        }
    }
}

#[derive(Default)]
enum RecorderStatus {
    #[default]
    Idle,
    Running(RecorderProcess),
}

struct RecorderProcess {
    child: Child,
    stdout_join: Option<std::thread::JoinHandle<()>>,
    stderr_join: Option<std::thread::JoinHandle<()>>,
    started_at: Instant,
    output_file: String,
}

impl RecorderProcess {
    fn finish(&mut self) {
        if let Some(handle) = self.stdout_join.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_join.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
struct LogEntry {
    source: LogSource,
    line: String,
}

#[derive(Clone, Copy)]
enum LogSource {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OutputChoice {
    value: String,
    label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AudioDeviceKind {
    Speaker,
    Microphone,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AudioDevice {
    name: String,
    description: String,
    kind: AudioDeviceKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowChoice {
    id: String,
    label: String,
    geometry: String,
}

struct ActionOutput {
    title: String,
    stdout: String,
    stderr: String,
    status_code: Option<i32>,
    error_message: Option<String>,
}

enum SimpleAction {
    ListOutputs,
    Version,
    Help,
}

impl SimpleAction {
    fn args(&self) -> Vec<&'static str> {
        match self {
            SimpleAction::ListOutputs => vec!["--list-output"],
            SimpleAction::Version => vec!["--version"],
            SimpleAction::Help => vec!["--help"],
        }
    }

    fn title(&self) -> &'static str {
        match self {
            SimpleAction::ListOutputs => "wf-recorder --list-output",
            SimpleAction::Version => "wf-recorder --version",
            SimpleAction::Help => "wf-recorder --help",
        }
    }
}

struct SimpleCommandOutput {
    stdout: String,
    stderr: String,
    status_code: Option<i32>,
}

fn run_simple_command(args: &[&str]) -> Result<SimpleCommandOutput, String> {
    let output = Command::new("wf-recorder")
        .args(args)
        .output()
        .map_err(|err| format!("Failed to run wf-recorder {}: {err}", args.join(" ")))?;

    let status_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(SimpleCommandOutput {
        stdout,
        stderr,
        status_code,
    })
}

fn detect_outputs() -> Result<Vec<OutputChoice>, String> {
    let output = Command::new("wf-recorder")
        .arg("--list-output")
        .output()
        .map_err(|err| format!("Failed to run `wf-recorder --list-output`: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`wf-recorder --list-output` exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut outputs: Vec<OutputChoice> = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Available outputs") {
            continue;
        }

        let content = if let Some((_, rest)) = trimmed.split_once(". ") {
            rest.trim()
        } else {
            trimmed
        };

        let (raw_name, raw_description) = if let Some(name_pos) = content.find("Name:") {
            let after_name = &content[name_pos + "Name:".len()..];
            let mut parts = after_name.split(" Description:");
            let name = parts.next().unwrap_or("").trim();
            let description = parts.next().unwrap_or("").trim();
            (name.to_string(), description.to_string())
        } else if let Some(desc_pos) = content.find(" Description:") {
            let name = content[..desc_pos].trim();
            let description = content[desc_pos + " Description:".len()..].trim();
            (name.to_string(), description.to_string())
        } else {
            (content.trim().to_string(), String::new())
        };

        let name = raw_name.trim().to_string();
        if name.is_empty() {
            continue;
        }

        let description = raw_description.trim().to_string();

        let label = if description.is_empty() {
            name.clone()
        } else {
            format!("{name} - {description}")
        };

        outputs.push(OutputChoice { value: name, label });
    }

    outputs.sort_by(|a, b| a.label.cmp(&b.label));
    outputs.dedup_by(|a, b| a.value == b.value);

    Ok(outputs)
}

fn detect_audio_devices() -> Result<Vec<AudioDevice>, String> {
    match detect_audio_devices_with_pactl() {
        Ok(devices) => Ok(devices),
        Err(err) => Err(err),
    }
}

fn detect_audio_devices_with_pactl() -> Result<Vec<AudioDevice>, String> {
    let output = Command::new("pactl")
        .args(["list", "sources"])
        .output()
        .map_err(|err| format!("pactl not available: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "pactl exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    let mut current_name = String::new();
    let mut current_description = String::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Source #") {
            push_audio_device(&mut devices, &mut current_name, &mut current_description);
        } else if let Some(rest) = trimmed.strip_prefix("Name: ") {
            current_name = rest.to_string();
        } else if let Some(rest) = trimmed.strip_prefix("Description: ") {
            current_description = rest.to_string();
        }
    }
    push_audio_device(&mut devices, &mut current_name, &mut current_description);

    if devices.is_empty() {
        return Err("pactl did not report any audio sources.".to_string());
    }

    Ok(devices)
}

fn push_audio_device(devices: &mut Vec<AudioDevice>, name: &mut String, description: &mut String) {
    if name.is_empty() {
        return;
    }

    let desc = if description.is_empty() {
        name.clone()
    } else {
        description.clone()
    };

    let kind = classify_audio_device(name, &desc);
    devices.push(AudioDevice {
        name: name.clone(),
        description: desc,
        kind,
    });

    name.clear();
    description.clear();
}

fn classify_audio_device(name: &str, description: &str) -> AudioDeviceKind {
    let name_lower = name.to_ascii_lowercase();
    let desc_lower = description.to_ascii_lowercase();

    if name_lower.contains("monitor")
        || desc_lower.contains("monitor")
        || desc_lower.contains("speaker")
    {
        AudioDeviceKind::Speaker
    } else if name_lower.contains("microphone")
        || name_lower.contains("input")
        || desc_lower.contains("microphone")
    {
        AudioDeviceKind::Microphone
    } else {
        AudioDeviceKind::Other
    }
}

fn detect_windows() -> Result<Vec<WindowChoice>, String> {
    let mut attempts = Vec::new();

    match detect_sway_windows() {
        Ok(list) if !list.is_empty() => return Ok(list),
        Ok(_) => attempts.push("swaymsg returned no windows".to_string()),
        Err(err) => attempts.push(err),
    }

    match detect_hypr_windows() {
        Ok(list) if !list.is_empty() => return Ok(list),
        Ok(_) => attempts.push("hyprctl returned no windows".to_string()),
        Err(err) => attempts.push(err),
    }

    if attempts.is_empty() {
        Err("No supported window backends detected (swaymsg or hyprctl).".to_string())
    } else {
        Err(attempts.join(". "))
    }
}

fn detect_sway_windows() -> Result<Vec<WindowChoice>, String> {
    let output = Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .map_err(|err| format!("swaymsg not available: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "swaymsg exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("Failed to parse sway tree: {err}"))?;

    let mut windows = Vec::new();
    collect_sway_windows(&value, &mut windows);
    Ok(windows)
}

fn collect_sway_windows(node: &Value, out: &mut Vec<WindowChoice>) {
    if let Some(nodes) = node.get("nodes").and_then(Value::as_array) {
        for child in nodes {
            collect_sway_windows(child, out);
        }
    }
    if let Some(nodes) = node.get("floating_nodes").and_then(Value::as_array) {
        for child in nodes {
            collect_sway_windows(child, out);
        }
    }

    let window_id = node
        .get("window")
        .and_then(Value::as_i64)
        .filter(|id| *id != 0);
    if window_id.is_none() {
        return;
    }

    let rect = node.get("rect").and_then(Value::as_object);
    let (x, y, w, h) = match rect {
        Some(rect) => {
            let x = rect
                .get("x")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                .round() as i32;
            let y = rect
                .get("y")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                .round() as i32;
            let w = rect
                .get("width")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                .round() as i32;
            let h = rect
                .get("height")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                .round() as i32;
            (x, y, w, h)
        }
        None => (0, 0, 0, 0),
    };

    if w <= 0 || h <= 0 {
        return;
    }

    let title = node
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("Untitled");
    let app = node
        .get("app_id")
        .and_then(Value::as_str)
        .or_else(|| {
            node.get("window_properties")
                .and_then(Value::as_object)
                .and_then(|props| props.get("class"))
                .and_then(Value::as_str)
        })
        .unwrap_or("window");

    let label = if title.is_empty() {
        app.to_string()
    } else {
        format!("{} - {}", app, title)
    };

    let geometry = format!("{x},{y} {w}x{h}");
    let id = window_id.unwrap().to_string();
    out.push(WindowChoice {
        id,
        label,
        geometry,
    });
}

fn detect_hypr_windows() -> Result<Vec<WindowChoice>, String> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .map_err(|err| format!("hyprctl not available: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "hyprctl exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("Failed to parse hyprctl output: {err}"))?;
    let clients = value
        .as_array()
        .ok_or_else(|| "Unexpected hyprctl output".to_string())?;

    let mut windows = Vec::new();
    for client in clients {
        if client
            .get("mapped")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            == false
        {
            continue;
        }

        let at = client.get("at").and_then(Value::as_array);
        let size = client.get("size").and_then(Value::as_array);
        let (x, y) = match at {
            Some(coords) if coords.len() >= 2 => (
                coords[0].as_f64().unwrap_or_default().round() as i32,
                coords[1].as_f64().unwrap_or_default().round() as i32,
            ),
            _ => (0, 0),
        };
        let (w, h) = match size {
            Some(dim) if dim.len() >= 2 => (
                dim[0].as_f64().unwrap_or_default().round() as i32,
                dim[1].as_f64().unwrap_or_default().round() as i32,
            ),
            _ => (0, 0),
        };
        if w <= 0 || h <= 0 {
            continue;
        }

        let title = client
            .get("title")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Untitled");
        let class = client
            .get("class")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("window");

        let label = format!("{} - {}", class, title);
        let geometry = format!("{x},{y} {w}x{h}");
        let id = client
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        windows.push(WindowChoice {
            id,
            label,
            geometry,
        });
    }

    Ok(windows)
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

fn render_param_editor(ui: &mut egui::Ui, label: &str, help: &str, params: &mut Vec<ParamEntry>) {
    label_with_help(ui, label, help);
    ui.vertical(|ui| {
        let mut removal_index: Option<usize> = None;
        if params.is_empty() {
            ui.label("No parameters yet.");
        }
        for (idx, entry) in params.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut entry.key)
                        .desired_width(120.0)
                        .hint_text("key"),
                );
                ui.label("=");
                ui.add(
                    TextEdit::singleline(&mut entry.value)
                        .desired_width(140.0)
                        .hint_text("value"),
                );
                if ui.small_button("Remove").clicked() {
                    removal_index = Some(idx);
                }
            });
        }
        if let Some(idx) = removal_index {
            params.remove(idx);
        }
        if ui.small_button("Add parameter").clicked() {
            params.push(ParamEntry::default());
        }
    });
}

fn label_with_help(ui: &mut egui::Ui, title: &str, help: &str) -> egui::Response {
    let response = ui.label(RichText::new(title).strong());
    if help.is_empty() {
        response
    } else {
        response.on_hover_text(help)
    }
}

fn render_audio_device_picker(
    ui: &mut egui::Ui,
    label: &str,
    devices: &[AudioDevice],
    selection: &mut String,
    empty_message: &str,
) {
    label_with_help(
        ui,
        label,
        "Select a device or type a custom identifier (PulseAudio/PipeWire source name).",
    );
    let selected_label = if selection.trim().is_empty() {
        "Default (auto)".to_string()
    } else {
        devices
            .iter()
            .find(|d| d.name == selection.trim())
            .map(|d| d.description.clone())
            .unwrap_or_else(|| selection.clone())
    };

    let combo_id = format!("{}_combo", label);
    egui::ComboBox::from_id_source(combo_id)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            ui.selectable_value(selection, String::new(), "Default (auto)");
            for device in devices {
                ui.selectable_value(selection, device.name.clone(), device.description.as_str());
            }
        });

    ui.horizontal(|ui| {
        ui.label("Device id");
        ui.text_edit_singleline(selection)
            .on_hover_text("You can paste a custom source name here if it is not listed above.");
    });

    if devices.is_empty() {
        ui.colored_label(Color32::from_rgb(255, 200, 120), empty_message);
    }
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

fn push_arg(args: &mut Vec<String>, flag: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
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

fn expand_home(path: String) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            let mut buf = PathBuf::from(home);
            buf.push(rest);
            return buf.to_string_lossy().into_owned();
        }
    } else if path == "~" {
        if let Ok(home) = env::var("HOME") {
            return home;
        }
    }
    path
}

fn format_exit_status(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!(", exit code {code}")
    } else {
        ", terminated by signal".to_string()
    }
}

#[cfg(unix)]
fn request_graceful_stop(pid: u32) -> std::io::Result<()> {
    let res = unsafe { libc::kill(pid as i32, libc::SIGINT) };
    if res == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn request_graceful_stop(_pid: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Graceful stop is only supported on Unix platforms",
    ))
}

fn show_readonly_text(ui: &mut egui::Ui, label: &str, text: &str, rows: usize) {
    ui.collapsing(label, |ui| {
        let mut buffer = text.to_string();
        ui.add(
            TextEdit::multiline(&mut buffer)
                .code_editor()
                .desired_rows(rows)
                .interactive(false),
        );
    });
}
