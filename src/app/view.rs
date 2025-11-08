use eframe::egui::{self, Align2, Color32, Image, Key, RichText, Spinner, TextEdit};
use eframe::{App, Frame};

use crate::actions::SimpleAction;
use crate::config::{AudioMode, CaptureMode, ParamEntry, RecorderConfig};
use crate::constants::{
    COMMON_AUDIO_BACKENDS, COMMON_AUDIO_CODECS, COMMON_OUTPUT_FORMATS, COMMON_VIDEO_CODECS,
};
use crate::models::{AudioDevice, AudioDeviceKind, RecorderStatus};

use super::state::{RecorderApp, Section, SidebarState};

#[derive(Clone, Copy)]
enum SidebarIcon {
    Capture,
    Encoding,
    Audio,
    Tools,
}

fn sidebar_icon_source(icon: SidebarIcon, dark: bool) -> (&'static str, &'static [u8]) {
    match (icon, dark) {
        (SidebarIcon::Capture, true) => (
            "bytes://capture_white.png",
            include_bytes!("../../assets/icons/png/capture_white.png"),
        ),
        (SidebarIcon::Capture, false) => (
            "bytes://capture_black.png",
            include_bytes!("../../assets/icons/png/capture_black.png"),
        ),
        (SidebarIcon::Encoding, true) => (
            "bytes://encoding_white.png",
            include_bytes!("../../assets/icons/png/encoding_white.png"),
        ),
        (SidebarIcon::Encoding, false) => (
            "bytes://encoding_black.png",
            include_bytes!("../../assets/icons/png/encoding_black.png"),
        ),
        (SidebarIcon::Audio, true) => (
            "bytes://audio_white.png",
            include_bytes!("../../assets/icons/png/audio_white.png"),
        ),
        (SidebarIcon::Audio, false) => (
            "bytes://audio_black.png",
            include_bytes!("../../assets/icons/png/audio_black.png"),
        ),
        (SidebarIcon::Tools, true) => (
            "bytes://tools_white.png",
            include_bytes!("../../assets/icons/png/tools_white.png"),
        ),
        (SidebarIcon::Tools, false) => (
            "bytes://tools_black.png",
            include_bytes!("../../assets/icons/png/tools_black.png"),
        ),
    }
}

impl App for RecorderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.poll_process();
        self.poll_async_tasks();

        self.apply_theme(ctx);

        if self.status.is_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        egui_extras::install_image_loaders(ctx);
        if self.outputs_loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }

        if self.status.is_running() {
            let ctrl_c_pressed = ctx.input(|input| {
                input.key_pressed(Key::C) && (input.modifiers.command || input.modifiers.ctrl)
            });
            if ctrl_c_pressed {
                self.stop_recording();
            }
        }

        egui::TopBottomPanel::top("sidebar_controls")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let button_label = match self.sidebar_state {
                        SidebarState::Expanded => "Compact sidebar",
                        SidebarState::Compact => "Hide sidebar",
                        SidebarState::Hidden => "Show sidebar",
                    };
                    if ui.button(button_label).clicked() {
                        self.toggle_sidebar();
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.dark_theme, "Use dark theme")
                        .changed()
                    {
                        self.apply_theme(ctx);
                    }
                });
            });

        match self.sidebar_state {
            SidebarState::Expanded => {
                egui::SidePanel::left("sidebar_expanded")
                    .resizable(true)
                    .min_width(150.0)
                    .max_width(260.0)
                    .default_width(190.0)
                    .show(ctx, |ui| {
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| {
                            ui.heading("Sections");
                        });
                        ui.separator();
                        ui.add_space(10.0);

                        let sections = [
                            (
                                Section::CaptureBasics,
                                SidebarIcon::Capture,
                                "Capture Basics",
                            ),
                            (
                                Section::VideoEncoding,
                                SidebarIcon::Encoding,
                                "Video Encoding",
                            ),
                            (
                                Section::AudioRecording,
                                SidebarIcon::Audio,
                                "Audio Recording",
                            ),
                            (
                                Section::ToolsDiagnostics,
                                SidebarIcon::Tools,
                                "Tools & Diagnostics",
                            ),
                        ];

                        ui.style_mut().visuals.widgets.inactive.rounding =
                            egui::Rounding::same(8.0);
                        ui.style_mut().visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
                        ui.style_mut().visuals.widgets.active.rounding = egui::Rounding::same(8.0);
                        ui.style_mut().spacing.button_padding = egui::Vec2::new(12.0, 12.0);
                        ui.style_mut().visuals.widgets.inactive.bg_fill =
                            egui::Color32::from_gray(245);
                        ui.style_mut().visuals.widgets.hovered.bg_fill =
                            egui::Color32::from_gray(220);
                        ui.style_mut().visuals.widgets.active.bg_fill =
                            egui::Color32::from_rgb(50, 100, 200);
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));

                        for (section, icon_kind, label) in sections {
                            let selected = self.current_section == section;
                            let (uri, bytes) = sidebar_icon_source(icon_kind, self.dark_theme);
                            let icon_image = Image::new(egui::ImageSource::Bytes {
                                uri: uri.into(),
                                bytes: bytes.into(),
                            })
                            .fit_to_exact_size(egui::Vec2::splat(28.0));
                            let mut button = egui::Button::image_and_text(icon_image, label);
                            if selected {
                                button = button.fill(egui::Color32::from_rgb(50, 100, 200));
                            }
                            if ui.add_sized([ui.available_width(), 50.0], button).clicked() {
                                self.current_section = section;
                            }
                            ui.add_space(5.0);
                        }
                    });
            }
            SidebarState::Compact => {
                egui::SidePanel::left("sidebar_compact")
                    .resizable(false)
                    .width_range(52.0..=70.0)
                    .show(ctx, |ui| {
                        ui.add_space(6.0);
                        for (section, icon_kind, label) in [
                            (Section::CaptureBasics, SidebarIcon::Capture, "Capture"),
                            (Section::VideoEncoding, SidebarIcon::Encoding, "Encoding"),
                            (Section::AudioRecording, SidebarIcon::Audio, "Audio"),
                            (Section::ToolsDiagnostics, SidebarIcon::Tools, "Tools"),
                        ] {
                            let selected = self.current_section == section;
                            let (uri, bytes) = sidebar_icon_source(icon_kind, self.dark_theme);
                            let icon_image = Image::new(egui::ImageSource::Bytes {
                                uri: uri.into(),
                                bytes: bytes.into(),
                            })
                            .fit_to_exact_size(egui::Vec2::splat(26.0));
                            let mut button = egui::Button::image(icon_image);
                            if selected {
                                button = button.fill(egui::Color32::from_rgb(50, 100, 200));
                            }
                            if ui.add(button).on_hover_text(label).clicked() {
                                self.current_section = section;
                            }
                            ui.add_space(6.0);
                        }
                    });
            }
            SidebarState::Hidden => {}
        }

        if matches!(self.sidebar_state, SidebarState::Hidden) {
            egui::Area::new("sidebar_reveal".into())
                .anchor(Align2::LEFT_CENTER, [8.0, 0.0])
                .show(ctx, |ui| {
                    egui::Frame::window(&ui.style()).show(ui, |ui| {
                        if ui.button("▶").clicked() {
                            self.toggle_sidebar();
                        }
                    });
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.heading("");
            let controls_width = ui.available_width();
            self.recording_controls(ui, controls_width);
            ui.separator();

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
                .auto_shrink([true, false])
                .show(ui, |ui| {
                    let content_width = ui.available_width();
                    match self.current_section {
                        Section::CaptureBasics => self.general_section(ui, content_width),
                        Section::VideoEncoding => self.video_section(ui, content_width),
                        Section::AudioRecording => self.audio_section(ui, content_width),
                        Section::ToolsDiagnostics => self.advanced_section(ui, content_width),
                    }
                    self.action_buttons(ui, content_width);
                    if matches!(self.current_section, Section::ToolsDiagnostics) {
                        self.command_preview(ui, content_width);
                        self.log_view(ui, content_width);
                    }
                });
        });
    }
}

impl RecorderApp {
    fn apply_theme(&self, ctx: &egui::Context) {
        if self.dark_theme {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }
    }

    fn general_section(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        let field_width = (width * 0.65).max(width - 140.0).clamp(120.0, width);
        egui::Grid::new("general_grid")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                label_with_help(
                    ui,
                    "File template",
                    "Set -f/--file. Supports $timestamp and $format placeholders.",
                );
                ui.add(
                    TextEdit::singleline(&mut self.config.file_template)
                        .desired_width(field_width.min(ui.available_width()))
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
                    .width(field_width.min(ui.available_width()))
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
                    .width(field_width.min(ui.available_width()))
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
                            "Select the output (-o/--output) to capture. Leave on compositor default to let wf-recorder ask the compositor which head to record. Some compositors require an explicit choice.",
                        );
                        ui.vertical(|ui| {
                            let display_label = if self.config.output.is_empty() {
                                "Compositor default (auto)".to_string()
                            } else {
                                self.available_outputs
                                    .iter()
                                    .find(|entry| entry.value == self.config.output)
                                    .map(|entry| entry.label.clone())
                                    .unwrap_or_else(|| self.config.output.clone())
                            };
                            egui::ComboBox::from_id_source("output_combo")
                                .width(field_width.min(ui.available_width()))
                                .selected_text(display_label)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.config.output,
                                        String::new(),
                                        "Compositor default (auto)",
                                    );
                                    for entry in &self.available_outputs {
                                        let response = ui.selectable_value(
                                            &mut self.config.output,
                                            entry.value.clone(),
                                            entry.label.as_str(),
                                        );
                                        response
                                            .on_hover_text(format!("Name: {}", entry.value));
                                    }
                                });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if self.outputs_loading {
                                    ui.add(Spinner::new());
                                } else if ui.small_button("Refresh outputs").clicked() {
                                    self.request_output_refresh();
                                }
                                ui.add_space(8.0);
                                ui.add(
                                    TextEdit::singleline(&mut self.config.output)
                                        .desired_width(field_width.min(ui.available_width()))
                                        .hint_text("Custom name (from wf-recorder --list-output)"),
                                )
                                .on_hover_text(
                                    "Override the selection if you need to paste an output name manually.",
                                );
                            });
                            if self.available_outputs.len() > 1
                                && self.config.output.trim().is_empty()
                            {
                                ui.colored_label(
                                    Color32::from_rgb(255, 210, 120),
                                    "Tip: if the compositor refuses to start, pick a specific output above.",
                                );
                            }
                        });
                        ui.end_row();

                        if !self.config.output.is_empty()
                            && !self
                                .available_outputs
                                .iter()
                                .any(|entry| entry.value == self.config.output)
                        {
                            ui.colored_label(
                                Color32::from_rgb(255, 200, 120),
                                "The selected output was not found in the last scan.",
                            );
                            ui.end_row();
                        }
                    }
                    CaptureMode::Window => {
                        label_with_help(
                            ui,
                            "Window",
                            "Select a window to capture (-g/--geometry will be filled automatically).",
                        );
                        if self.windows_loading {
                            ui.horizontal(|ui| {
                                ui.add(Spinner::new());
                                ui.label("Loading windows…");
                            });
                        } else {
                            let selected_label = self
                                .available_windows
                                .iter()
                                .find(|w| w.id == self.config.selected_window_id)
                                .map(|w| w.label.clone())
                                .unwrap_or_else(|| "Select a window".to_string());
                            let combo_width = field_width.min(ui.available_width());
                            egui::ComboBox::from_id_source("window_combo")
                                .width(combo_width)
                                .selected_text(selected_label)
                                .show_ui(ui, |ui| {
                                    for window in &self.available_windows {
                                        if ui
                                            .selectable_label(
                                                self.config.selected_window_id == window.id,
                                                &window.label,
                                            )
                                            .clicked()
                                        {
                                            self.config.selected_window_id = window.id.clone();
                                            self.config.selected_window_geometry =
                                                window.geometry.clone();
                                        }
                                    }
                                });
                        }
                        if let Some(err) = &self.windows_error {
                            ui.colored_label(Color32::from_rgb(255, 120, 120), err);
                        }
                        if ui.small_button("Refresh windows").clicked() {
                            self.request_window_refresh();
                        }
                        ui.end_row();
                    }
                    CaptureMode::Area => {
                        label_with_help(
                            ui,
                            "Geometry",
                            "Define the capture area (format: x,y WIDTHxHEIGHT).",
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                TextEdit::singleline(&mut self.config.area_geometry)
                                    .desired_width(field_width.min(ui.available_width()))
                                    .hint_text("100,200 1920x1080"),
                            );
                            if ui.button("Select area").clicked() {
                                self.run_slurp_for_geometry();
                            }
                        });
                        ui.end_row();
                    }
                }
            });
    }

    fn video_section(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        let field_width = (width * 0.65).max(width - 140.0).clamp(120.0, width);
        egui::Grid::new("video_grid")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                label_with_help(
                    ui,
                    "Video codec",
                    "Sets -c/--codec. Pick a preset or type a custom encoder name.",
                );
                let codec_label = COMMON_VIDEO_CODECS
                    .iter()
                    .find(|(_, value)| *value == self.config.codec)
                    .map(|(label, _)| *label)
                    .unwrap_or("Custom");
                egui::ComboBox::from_id_source("codec_combo")
                    .width(field_width.min(ui.available_width()))
                    .selected_text(codec_label)
                    .show_ui(ui, |ui| {
                        for (label, value) in COMMON_VIDEO_CODECS {
                            ui.selectable_value(
                                &mut self.config.codec,
                                value.to_string(),
                                label.to_owned(),
                            );
                        }
                    });
                ui.add(
                    TextEdit::singleline(&mut self.config.codec)
                        .desired_width(field_width.min(ui.available_width()))
                        .hint_text("libx264"),
                );
                ui.end_row();

                label_with_help(
                    ui,
                    "Framerate",
                    "Sets -f/--framerate. Leave blank to let wf-recorder choose.",
                );
                ui.add(
                    TextEdit::singleline(&mut self.config.framerate)
                        .desired_width(field_width.min(ui.available_width()))
                        .hint_text("60"),
                );
                ui.end_row();

                ui.label(RichText::new("Extra codec params").strong());
                render_param_editor(
                    ui,
                    "Codec parameter",
                    "Adds -p/--codec-param entries (format: key=value).",
                    &mut self.config.codec_params,
                    field_width,
                );
                ui.end_row();

                label_with_help(
                    ui,
                    "Pixel format",
                    "Sets -x/--pixel-format. Leave blank for wf-recorder default.",
                );
                ui.add(
                    TextEdit::singleline(&mut self.config.pixel_format)
                        .desired_width(field_width.min(ui.available_width())),
                );
                ui.end_row();

                label_with_help(
                    ui,
                    "Muxer/Container",
                    "Sets -m/--muxer. Overridden automatically by --file format when omitted.",
                );
                ui.add(
                    TextEdit::singleline(&mut self.config.muxer)
                        .desired_width(field_width.min(ui.available_width())),
                );
                ui.end_row();

                label_with_help(
                    ui,
                    "VAAPI device",
                    "Sets -d/--device (for hardware accelerated encoders).",
                );
                ui.add(
                    TextEdit::singleline(&mut self.config.encoding_device)
                        .desired_width(field_width.min(ui.available_width()))
                        .hint_text("/dev/dri/renderD128"),
                );
                ui.end_row();

                label_with_help(
                    ui,
                    "Filters",
                    "Sets -F/--filter. Useful for scaling or overlays.",
                );
                ui.add(
                    TextEdit::singleline(&mut self.config.filter)
                        .desired_width(field_width.min(ui.available_width())),
                );
                ui.end_row();

                label_with_help(
                    ui,
                    "Extra flags",
                    "Toggle wf-recorder --no-dmabuf/--no-damage/--log/--overwrite switches.",
                );
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.config.no_dmabuf, "Disable DMA-BUF (--no-dmabuf)");
                    ui.checkbox(
                        &mut self.config.no_damage,
                        "Disable damage tracking (--no-damage)",
                    );
                    ui.checkbox(&mut self.config.log_enabled, "Enable log output (--log)");
                    ui.checkbox(
                        &mut self.config.overwrite,
                        "Overwrite existing files (--overwrite)",
                    );
                });
                ui.end_row();
            });
    }

    fn audio_section(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        let control_width = (width * 0.7).max(160.0).min(width);
        label_with_help(
            ui,
            "Audio mode",
            "Pick which sources to capture. wf-recorder will add --audio options accordingly.",
        );
        let previous_audio_mode = self.config.audio_mode;
        let mut refresh_audio = false;
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.config.audio_mode,
                AudioMode::None,
                "None (no audio)",
            );
            ui.selectable_value(&mut self.config.audio_mode, AudioMode::System, "System");
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
                let speakers: Vec<_> = self
                    .available_audio_devices
                    .iter()
                    .filter(|d| matches!(d.kind, AudioDeviceKind::Speaker))
                    .cloned()
                    .collect();
                render_audio_device_picker(
                    ui,
                    "Speaker output",
                    speakers.as_slice(),
                    &mut self.config.selected_speaker_device,
                    "No playback devices detected. Try Refresh.",
                    control_width,
                );
            }
            AudioMode::Microphone => {
                let microphones: Vec<_> = self
                    .available_audio_devices
                    .iter()
                    .filter(|d| matches!(d.kind, AudioDeviceKind::Microphone))
                    .cloned()
                    .collect();
                render_audio_device_picker(
                    ui,
                    "Microphone",
                    microphones.as_slice(),
                    &mut self.config.selected_microphone_device,
                    "No microphones detected. Try Refresh.",
                    control_width,
                );
            }
            AudioMode::Both => {
                let speakers: Vec<_> = self
                    .available_audio_devices
                    .iter()
                    .filter(|d| matches!(d.kind, AudioDeviceKind::Speaker))
                    .cloned()
                    .collect();
                let microphones: Vec<_> = self
                    .available_audio_devices
                    .iter()
                    .filter(|d| matches!(d.kind, AudioDeviceKind::Microphone))
                    .cloned()
                    .collect();
                render_audio_device_picker(
                    ui,
                    "Speaker output",
                    speakers.as_slice(),
                    &mut self.config.selected_speaker_device,
                    "No playback devices detected. Try Refresh.",
                    control_width,
                );
                ui.add_space(4.0);
                render_audio_device_picker(
                    ui,
                    "Microphone",
                    microphones.as_slice(),
                    &mut self.config.selected_microphone_device,
                    "No microphones detected. Try Refresh.",
                    control_width,
                );
            }
        }

        ui.add_space(8.0);
        egui::CollapsingHeader::new("Advanced audio options")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("audio_advanced_grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
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
                            .width(control_width.min(ui.available_width()))
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
                                .desired_width(control_width.min(ui.available_width()))
                                .hint_text("Leave empty to use the selected devices above"),
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Audio codec",
                            "Sets --audio-codec. Pick a preset or type the encoder name.",
                        );
                        let codec_label = COMMON_AUDIO_CODECS
                            .iter()
                            .find(|(_, value)| *value == self.config.audio_codec)
                            .map(|(label, _)| *label)
                            .unwrap_or("Custom");
                        egui::ComboBox::from_id_source("audio_codec_combo")
                            .width(control_width.min(ui.available_width()))
                            .selected_text(codec_label)
                            .show_ui(ui, |ui| {
                                for (label, value) in COMMON_AUDIO_CODECS {
                                    ui.selectable_value(
                                        &mut self.config.audio_codec,
                                        value.to_string(),
                                        label.to_owned(),
                                    );
                                }
                            });
                        ui.end_row();

                        let label = ui.label(RichText::new("Audio codec parameters").strong());
                        label.on_hover_text("Adds -P/--audio-codec-param entries (format: key=value).");
                        render_param_editor(
                            ui,
                            "",
                            "Adds -P/--audio-codec-param entries (format: key=value).",
                            &mut self.config.audio_codec_params,
                            control_width,
                        );
                        ui.end_row();

                        label_with_help(
                            ui,
                            "Sample rate (Hz)",
                            "Sets -R/--sample-rate. Common values: 48000 or 44100.",
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.config.sample_rate)
                                .desired_width(control_width.min(ui.available_width()))
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
                                .desired_width(control_width.min(ui.available_width())),
                        );
                        ui.end_row();
                    });
            });
    }

    fn advanced_section(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
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
                show_readonly_text(ui, "Command stdout", &output.stdout, 8, width);
            }
            if !output.stderr.is_empty() {
                show_readonly_text(ui, "Command stderr", &output.stderr, 6, width);
            }
            if let Some(err) = &output.error_message {
                ui.colored_label(Color32::RED, err);
            }
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
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
                self.log_dirty
                    .store(true, std::sync::atomic::Ordering::Relaxed);
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

    fn action_buttons(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let reset_width = ui.available_width().min(width).min(300.0);
            if ui
                .add_sized([reset_width, 30.0], egui::Button::new("Reset to defaults"))
                .on_hover_text("Restore the recommended settings in one click.")
                .clicked()
            {
                self.config = RecorderConfig::default();
                self.request_output_refresh();
                self.request_window_refresh();
            }
        });
    }

    fn command_preview(&self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        ui.separator();
        ui.label("Command preview");
        match self.build_command_preview() {
            Ok(mut preview) => {
                let preview_width = width.min(ui.available_width());
                ui.add(
                    TextEdit::multiline(&mut preview)
                        .code_editor()
                        .desired_rows(2)
                        .desired_width(preview_width)
                        .interactive(false),
                );
            }
            Err(err) => {
                ui.colored_label(Color32::from_rgb(255, 120, 120), err);
            }
        }
    }

    fn recording_controls(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        ui.separator();
        let mut start_requested = false;
        let mut stop_requested = false;
        let mut force_requested = false;

        match &self.status {
            RecorderStatus::Idle => {
                let available = ui.available_width();
                let button_width = available.min(width);
                let start_button = egui::Button::new("Start recording")
                    .min_size(egui::vec2(button_width, 40.0))
                    .wrap(true);
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
                        format!("Recording… {:.1}s elapsed", elapsed),
                    );
                    if ui
                        .button("Stop (Ctrl+C)")
                        .on_hover_text("Send SIGINT to wf-recorder for a graceful stop.")
                        .clicked()
                    {
                        stop_requested = true;
                    }
                    if ui
                        .button("Force stop (SIGKILL)")
                        .on_hover_text("Kill the process immediately if it does not stop.")
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

    fn log_view(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.set_width(width);
        ui.separator();
        ui.collapsing("wf-recorder log", |ui| {
            if self.log_display.is_empty() {
                ui.label("No log data yet. Start a recording to capture wf-recorder output.");
            } else {
                ui.horizontal(|ui| {
                    if ui.small_button("Copy all").clicked() {
                        ui.output_mut(|o| o.copied_text = self.log_display.clone());
                    }
                    ui.label("Text is selectable; use Copy all for the full buffer.");
                });
                ui.add_space(4.0);
                let mut preview = self.log_display.clone();
                let preview_width = width.min(ui.available_width());
                ui.add(
                    TextEdit::multiline(&mut preview)
                        .code_editor()
                        .desired_rows(12)
                        .desired_width(preview_width)
                        .interactive(false),
                );
            }
        });
    }
}

fn render_param_editor(
    ui: &mut egui::Ui,
    label: &str,
    help: &str,
    params: &mut Vec<ParamEntry>,
    width: f32,
) {
    if !label.is_empty() {
        label_with_help(ui, label, help);
    }
    let control_width = (width * 0.6).max(140.0).min(width);
    ui.vertical(|ui| {
        let mut removal_index: Option<usize> = None;
        if params.is_empty() {
            ui.label("No parameters yet.");
        }
        for (idx, entry) in params.iter_mut().enumerate() {
            ui.columns(2, |columns| {
                let left_width = columns[0].available_width().min(control_width);
                columns[0].add(
                    TextEdit::singleline(&mut entry.key)
                        .desired_width(left_width)
                        .hint_text("key"),
                );
                columns[1].with_layout(
                    egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                    |ui| {
                        ui.label("=");
                        let value_width = ui.available_width().min(control_width);
                        ui.add(
                            TextEdit::singleline(&mut entry.value)
                                .desired_width(value_width)
                                .hint_text("value"),
                        );
                        if ui.small_button("Remove").clicked() {
                            removal_index = Some(idx);
                        }
                    },
                );
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
    width: f32,
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

    let control_width = width.min(ui.available_width());
    let combo_id = format!("{}_combo", label);
    egui::ComboBox::from_id_source(combo_id)
        .width(control_width)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            ui.selectable_value(selection, String::new(), "Default (auto)");
            for device in devices {
                ui.selectable_value(selection, device.name.clone(), device.description.as_str());
            }
        });

    ui.horizontal(|ui| {
        ui.label("Device id");
        let input_width = control_width.min(ui.available_width());
        ui.add(TextEdit::singleline(selection).desired_width(input_width));
    });

    if devices.is_empty() {
        ui.colored_label(Color32::from_rgb(255, 200, 120), empty_message);
    }
}

fn show_readonly_text(ui: &mut egui::Ui, label: &str, text: &str, rows: usize, width: f32) {
    ui.collapsing(label, |ui| {
        let mut buffer = text.to_string();
        ui.add(
            TextEdit::multiline(&mut buffer)
                .code_editor()
                .desired_rows(rows)
                .desired_width(width.min(ui.available_width()))
                .interactive(false),
        );
    });
}
