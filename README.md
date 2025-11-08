# wf-recorder UI

Wayland-native desktop interface for `wf-recorder` that wraps common recording, encoding, and audio options in a friendlier workflow. The app is built with `egui`/`eframe` and keeps recorder configuration, discovery, and UI state isolated under `src/` for easy hacking.

## Features
- Sidebar-driven layout with Capture, Encoding, Audio, and Tools sections plus optional compact/hidden modes.
- Light/dark theme toggle, contextual icons, and keyboard-driven stop (`Ctrl+C`/`Cmd+C`) while recording.
- Discovers common audio devices/backends, exposes codec/container presets, and surfaces diagnostics helpers inline.
- Ships static asset bundle under `assets/` and Wayland recorder manpage notes under `docs/`.

## Download
Grab the latest beta build from the releases page:

```
https://github.com/LaurenceGuws/wf-recorder-ui/releases/download/beta/wf-recorder-ui-0.1.0-amd64
```

After downloading:

```bash
chmod +x wf-recorder-ui-0.1.0-amd64
./wf-recorder-ui-0.1.0-amd64
```

The binary targets x86_64 Linux Wayland compositors (tested on sway). Ensure `wf-recorder` is installed and on your `PATH`.

## Build From Source
Prerequisites:
- Rust toolchain (edition 2024) with `cargo`.
- Wayland compositor and `wf-recorder` runtime for end-to-end testing.

Clone the repo and use the standard workflow:

```bash
cargo check
cargo clippy --all-targets --all-features
cargo fmt
cargo run --release
```

`cargo run --release` launches the UI with optimized settings that better reflect production performance. During development you can also keep `cargo check` running in watch mode for faster iteration.

## Project Layout
- `src/main.rs` wires up `eframe` and bootstraps discovery/actions.
- Domain modules (`src/actions.rs`, `src/config.rs`, `src/discovery.rs`, `src/models.rs`, `src/constants.rs`) keep recorder logic separate from presentation.
- UI state & widgets live under `src/app/` (`state.rs`, `view.rs`, `mod.rs`) for targeted unit tests.
- Assets live in `assets/`, and long-form references such as `docs/wf-recorder-manpage.txt` stay in `docs/`.

## Testing
Unit tests sit alongside their modules inside `#[cfg(test)]` blocks. Run the full set with:

```bash
cargo test
```

Add `-- --nocapture` whenever you need to inspect recorder/stdout interactions.

## Contributing
- Keep modules focused and prefer free functions for thin wrappers.
- Document non-obvious config defaults directly in `config.rs`.
- Before sending a PR, run the commands listed under “Build From Source” and attach UI screenshots if visuals changed.
- Follow the existing MIT license (`LICENSE`) for any contributions; include `Refs #<id>` footers when filing PRs.

## License
This project is available under the MIT License. See `LICENSE` for details.
