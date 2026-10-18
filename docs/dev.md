# Development notes

## Source layout

```text
src/main.rs                Native eframe setup and launch arguments
src/instance.rs            Workspace instance claims and window reveal requests
src/macos.rs               Native app-bundle reopen delegate
src/hotkey.rs              Native global reveal shortcut registration and wakeups
src/linux.rs               X11 cursor setup before the native event loop
src/linux_cursor.rs        XSettings cursor parsing and scale selection
src/app.rs                 Application state, navigation, syncing, and eframe wiring
src/app/drafts.rs           Session drafts scoped to workspace, list, and task
src/app/persistence.rs      Save results, dirty-list retries, and visible status
src/app/keyboard.rs         Keyboard commands and transient focus restoration
src/app/keyboard_events.rs  Ordered editor input before screen-changing shortcuts
src/app/keyboard_levels.rs  Directional list, checklist, History, and form navigation
src/app/list.rs             Task entry, rows, editing, gestures, and ordering
src/app/overlays.rs         Quick add, Find, and keyboard reference
src/app/overview.rs         List cards and list creation
src/app/overview_order.rs   Manual tile order and native drag event handling
src/app/sorting.rs          Task view sorting and preserved manual storage order
src/app/context.rs          Task context editor and complete-task clipboard copy
src/app/desktop.rs          Global shortcut settings and workspace-folder opening
src/app/settings.rs         Global preferences and separate List options
src/app/task_details.rs     Measured checklists and read-only History
src/app/task_controls.rs    Task controls and optional collapsible child previews
src/app/task_operations.rs  Child editing, deletion Undo, moving, and archival
src/app/task_text.rs        Title wrapping and hover scrolling
src/app/theme.rs            Fonts, palettes, and shared rendering helpers
src/app/window.rs           Tray hiding, restoration, and shutdown
src/model.rs               Serializable list and task model
src/storage.rs             Paths, preference merging, list files, and atomic writes
src/tray.rs                Native tray icon and menu actions
```

Neighboring `*_tests.rs` files cover storage, real-widget keyboard and pointer journeys, failure recovery, launch coordination, and layout regressions.

Keep source files below 1,000 lines and split by actual responsibilities rather than generic layers or arbitrary line counts.

## Verification

Isolate settings and temporary fixtures before running checks. These explicit XDG overrides apply on every platform:

```bash
mkdir -p target/agent-checks/tmp target/agent-checks/config target/agent-checks/data
export TMPDIR="$PWD/target/agent-checks/tmp"
export XDG_CONFIG_HOME="$PWD/target/agent-checks/config"
export XDG_DATA_HOME="$PWD/target/agent-checks/data"
```

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

Use separate settings and workspace directories to keep GUI trials away from everyday tasks:

```bash
mkdir -p target/gui-check/tmp target/gui-check/config target/gui-check/data
TMPDIR="$PWD/target/gui-check/tmp" \
XDG_CONFIG_HOME="$PWD/target/gui-check/config" \
XDG_DATA_HOME="$PWD/target/gui-check/data" \
EGUI_INSPECTION=127.0.0.1:5741 \
cargo run --locked --features eframe/inspection -- --data-dir target/gui-check/workspace
```

Attach the GUI inspector to port 5741. Use a distinct port and `XDG_CONFIG_HOME` for a second instance, including when testing two instances against one workspace. Otherwise the second launch reveals the first instance. Keep `TMPDIR`, settings, data, screenshots, and logs under `target/`; leave `HOME` unchanged. See [platform paths](storage.md#default-directories).

Exercise the [task interactions](usage.md), both title modes, 320- and 520-point windows, maximum text size in both fonts and weights, pinned mode, and opacity over bright and dark backgrounds. Use long text, explicit newlines, and many History rows; every completion, Delete, Add, and Restore control must remain reachable. Inspect saved JSON after edits, Undo, archive, and Move; restart to check persistence and window placement. Repeat the [shared-folder flows](storage.md#file-sync-behavior) with two instances. Check paths, shortcuts, accessibility, and transparency on each native platform.

Close and minimize the test window where supported: it should disappear from the taskbar while its tray icon remains. Repeat with decorations disabled and while editing a task; hiding should save the edit and allow external-file reloads while hidden. Restore it through the icon's Show action, then Quit while hidden and verify that the process and icon disappear and pending clear/delete actions are saved.

Run the installed binary with `--start-hidden`: it should remain in the tray without a taskbar window, then restore through **Show Minimalist List**. An ordinary launch should open the window; repeating it for the same workspace and local settings should reveal the existing instance. A repeated hidden launch should remain quiet. Test both shortcuts with an installation path containing spaces. On Windows, use the release build to verify that no console window is created. Inspect the generated startup entry; checking execution at sign-in requires a real desktop login.

Repeat without a tray host: the normal backend and close/minimize behavior should remain, with no startup warning. Close during an inline edit or pending clear/delete and inspect the saved JSON after restart. If the tray host disappears while hidden, the window should restore at the next scan.

For macOS installation checks, run `bash scripts/install-macos.sh`, then check both generated plists with `plutil -lint`. Open `~/Applications/Minimalist List.app`: the window and menu-bar icon should appear without a Dock icon or Terminal window. Close the window and confirm that minimize is disabled, then use **Show Minimalist List** to restore and focus it. Hide it again and reopen the running bundle through Finder or `open`; the same window and process should return. **Quit** should remove the icon and leave the app stopped. Sign out and back in: the icon should appear without opening a window or stealing focus. Repeat with decorations disabled, a workspace path containing spaces, and an inline edit before hiding; restart and inspect the saved task and workspace selection. Verify both light and dark menu-bar appearances and Command shortcuts. These behaviors require a native Mac; checking the Apple Silicon and Intel targets or parsing plist files on another OS does not validate them.

For failure recovery, temporarily obstruct an isolated fixture's list directory, then add or edit a task and try Quick add. Verify the visible error, retained content, successful retry, and exactly one saved copy after restart. Repeat with startup workspace access, a pending list rename before shutdown or switching folders, and Move failures at both destination and source writes. Check that deleting a dirty list clears its retry state.
