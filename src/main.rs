mod actions;
mod app;
mod config;
mod constants;
mod discovery;
mod models;

use app::RecorderApp;
use eframe::NativeOptions;

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
