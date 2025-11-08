use std::process::Command;

pub enum SimpleAction {
    ListOutputs,
    Version,
    Help,
}

impl SimpleAction {
    pub fn args(&self) -> Vec<&'static str> {
        match self {
            SimpleAction::ListOutputs => vec!["--list-output"],
            SimpleAction::Version => vec!["--version"],
            SimpleAction::Help => vec!["--help"],
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            SimpleAction::ListOutputs => "wf-recorder --list-output",
            SimpleAction::Version => "wf-recorder --version",
            SimpleAction::Help => "wf-recorder --help",
        }
    }
}

pub struct SimpleCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status_code: Option<i32>,
}

pub fn run_simple_command(args: &[&str]) -> Result<SimpleCommandOutput, String> {
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
