pub const COMMON_VIDEO_CODECS: [(&str, &str); 5] = [
    ("H.264 (CPU, libx264)", "libx264"),
    ("H.264 (VAAPI)", "h264_vaapi"),
    ("H.265 / HEVC (VAAPI)", "hevc_vaapi"),
    ("VP9 (libvpx-vp9)", "libvpx-vp9"),
    ("Animated GIF", "gif"),
];

pub const COMMON_AUDIO_CODECS: [(&str, &str); 3] = [
    ("AAC (aac)", "aac"),
    ("Opus (libopus)", "libopus"),
    ("FLAC", "flac"),
];

pub const COMMON_AUDIO_BACKENDS: [(&str, &str); 3] = [
    ("PulseAudio / PipeWire", "pulse"),
    ("ALSA", "alsa"),
    ("JACK", "jack"),
];

pub const COMMON_OUTPUT_FORMATS: [(&str, &str); 5] = [
    ("MP4 (H.264)", "mp4"),
    ("MKV (Matroska)", "mkv"),
    ("WebM (VP9/Opus)", "webm"),
    ("GIF (animated)", "gif"),
    ("MOV (QuickTime)", "mov"),
];
