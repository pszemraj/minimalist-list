# Development notes

## Source layout

```text
src/main.rs                Native eframe window setup
src/app.rs                 Application state, navigation, syncing, and eframe wiring
src/app/list.rs            List screen and task interactions
src/app/overlays.rs        Quick add and cross-list search
src/app/overview.rs        List-card overview
src/app/settings.rs        Workspace, appearance, and list settings
src/app/task_details.rs    Subtasks, deletion, clearing, and history
src/app/task_text.rs       Title wrapping and hover scrolling
src/app/theme.rs           Accent palettes and shared rendering helpers
src/app/window.rs          Tray hiding, restoration, and shutdown
src/model.rs               Serializable list and task model
src/storage.rs             Platform paths, list files, polling, and atomic writes
src/tray.rs                Native tray icon and menu actions
```

Keep source files below 1,000 lines and split by actual responsibilities rather than generic layers or arbitrary line counts.

## Verification

Run the checks before committing:

```bash
cargo check --locked --workspace --all-targets --all-features
cargo fmt --all --check
cargo test --locked
cargo test --locked --features eframe/inspection
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo build --locked --workspace --all-targets --all-features
rustdoc-checker src --strict
```

## GUI inspection

On Linux, use separate settings and workspace directories to keep GUI trials away from everyday tasks:

```bash
mkdir -p target/gui-check/config target/gui-check/data
XDG_CONFIG_HOME="$PWD/target/gui-check/config" \
XDG_DATA_HOME="$PWD/target/gui-check/data" \
EGUI_INSPECTION=127.0.0.1:5741 \
cargo run --locked --features eframe/inspection -- --data-dir target/gui-check/workspace
```

Attach the GUI inspector to port 5741. Use a distinct port and settings directory for a second instance. XDG overrides apply to Linux; see [platform paths](storage.md#default-directories) for other systems.

Exercise the [task interactions](usage.md), both title modes, narrow dialogs, large fonts, pinned mode, and opacity over bright and dark backgrounds. Inspect saved JSON after mutations, restart to check persistence, and repeat the [shared-folder flows](storage.md#file-sync-behavior) with two instances. Check paths, shortcuts, and transparency on native macOS and Windows as well as Linux.

Close and minimize the test window: it should disappear from the taskbar while its tray icon remains. Repeat with decorations disabled and while editing a task; hiding should save the edit and allow external-file reloads while hidden. Restore it through the icon's Show action, then Quit while hidden and verify that the process and icon disappear and pending clear/delete actions are saved.

Repeat without a tray host: the normal backend and close/minimize behavior should remain, with no startup warning. Close during an inline edit or pending clear/delete and inspect the saved JSON after restart. If the tray host disappears while hidden, the window should restore at the next scan.
