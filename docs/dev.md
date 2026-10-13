# Development notes

## Source layout

```text
src/main.rs                          Native eframe window setup
src/app.rs                           Application state, navigation, syncing, and eframe wiring
src/app/keyboard.rs                  Keyboard commands and transient focus restoration
src/app/keyboard_levels.rs           Directional task, checklist, History, and creation navigation
src/app/keyboard_tests.rs            Headless keyboard interaction journeys
src/app/keyboard_focus_tests.rs      Focus restoration and Settings regressions
src/app/keyboard_reference_tests.rs  Shortcut reference interactions and native labels
src/app/keyboard_navigation_tests.rs Directional navigation, forms, Find, and Backspace journeys
src/app/list.rs                      List screen and task interactions
src/app/overlays.rs                  Quick add, cross-list search, and keyboard reference
src/app/overview.rs                  List-card overview
src/app/settings.rs                  Workspace, appearance, and list settings
src/app/task_details.rs              Subtasks, deletion, clearing, and history
src/app/task_text.rs                 Title wrapping and hover scrolling
src/app/theme.rs                     Accent palettes and shared rendering helpers
src/app/window.rs                    Tray hiding, restoration, and shutdown
src/model.rs                         Serializable list and task model
src/storage.rs                       Platform paths, list files, polling, and atomic writes
src/storage_tests.rs                 Storage and platform-path regression tests
src/tray.rs                          Native tray icon and menu actions
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

Run the installed binary with `--start-hidden`: it should remain in the tray without a taskbar window, then restore through **Show Minimalist List**. An ordinary launch should open the window. Test both shortcuts with an installation path containing spaces. On Windows, use the release build to verify that no console window is created. Inspect the generated startup entry; checking execution at sign-in requires a real desktop login.

Repeat without a tray host: the normal backend and close/minimize behavior should remain, with no startup warning. Close during an inline edit or pending clear/delete and inspect the saved JSON after restart. If the tray host disappears while hidden, the window should restore at the next scan.

On a Mac, install with `bash scripts/install-macos.sh`, then check both generated plists with `plutil -lint`. Open `~/Applications/Minimalist List.app`: the window and menu-bar icon should appear without a Dock icon or Terminal window. Close and minimize, then use **Show Minimalist List** to restore and focus the window; **Quit** should remove the icon and leave the app stopped. Sign out and back in: the icon should appear without opening a window or stealing focus. Repeat with decorations disabled, a workspace path containing spaces, and an inline edit before hiding; restart and inspect the saved task and workspace selection. Verify both light and dark menu-bar appearances and Command shortcuts. These behaviors require a native Mac; checking the Apple Silicon and Intel targets or parsing plist files on another OS does not validate them.
