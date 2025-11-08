use chrono::Local;
use std::env;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    Screen,
    Window,
    Area,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioMode {
    None,
    System,
    Microphone,
    Both,
}

#[derive(Clone)]
pub struct RecorderConfig {
    pub capture_mode: CaptureMode,
    pub audio_mode: AudioMode,
    pub audio_enabled: bool,
    pub audio_device: String,
    pub audio_backend: String,
    pub audio_codec: String,
    pub audio_codec_params: Vec<ParamEntry>,
    pub sample_rate: String,
    pub sample_format: String,
    pub codec: String,
    pub codec_params: Vec<ParamEntry>,
    pub framerate: String,
    pub bframes: String,
    pub buffrate: String,
    pub encoding_device: String,
    pub pixel_format: String,
    pub muxer: String,
    pub output: String,
    pub filter: String,
    pub area_geometry: String,
    pub selected_window_id: String,
    pub selected_window_geometry: String,
    pub selected_speaker_device: String,
    pub selected_microphone_device: String,
    pub file_template: String,
    pub file_format: String,
    pub no_dmabuf: bool,
    pub no_damage: bool,
    pub log_enabled: bool,
    pub overwrite: bool,
}

impl RecorderConfig {
    pub fn build_command_args(
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

    pub fn preview_output_file(&self) -> Result<String, String> {
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
pub struct ParamEntry {
    pub key: String,
    pub value: String,
}

impl ParamEntry {
    pub fn format(&self) -> Option<String> {
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

fn push_arg(args: &mut Vec<String>, flag: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        args.push(flag.to_string());
        args.push(value.to_string());
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
