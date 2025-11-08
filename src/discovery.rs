use crate::models::{AudioDevice, AudioDeviceKind, OutputChoice, WindowChoice};
use serde_json::Value;
use std::process::Command;

pub fn detect_outputs() -> Result<Vec<OutputChoice>, String> {
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

pub fn detect_audio_devices() -> Result<Vec<AudioDevice>, String> {
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

pub fn detect_windows() -> Result<Vec<WindowChoice>, String> {
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
