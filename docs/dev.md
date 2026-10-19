# Development notes

## Source layout

```text
src/main.rs                Native eframe setup and launch arguments
src/instance.rs            Workspace instance claims and window reveal requests
src/macos.rs               Native app-bundle reopen delegate
src/hotkey.rs              Native global reveal shortcut registration and wakeups
src/linux.rs               X11 cursor setup before the native event loop
src/linux_cursor.rs        XSettings cursor parsing and scale selection
src/linux_taskbar.rs       EWMH state preservation and taskbar request payloads
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

## Verification

Isolate settings and temporary fixtures before running checks. These explicit XDG overrides apply on every platform:

```bash
mkdir -p target/agent-checks/tmp target/agent-checks/config target/agent-checks/data
export TMPDIR="$PWD/target/agent-checks/tmp"
export XDG_CONFIG_HOME="$PWD/target/agent-checks/config"
export XDG_DATA_HOME="$PWD/target/agent-checks/data"
```

For runtime changes, run the checks before committing:

```bash
cargo check --locked --workspace --all-targets --all-features
cargo fmt --all --check
cargo test --locked
cargo test --locked --features eframe/inspection
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo build --locked --workspace --all-targets --all-features
rustdoc-checker src --strict
git diff --check
```

For documentation-only changes, check Markdown links and whitespace.

## GUI inspection

Use separate settings and workspace directories to keep GUI trials away from everyday tasks:

```bash
mkdir -p target/gui-check/tmp target/gui-check/config/minimalist-list target/gui-check/data
if [ ! -e target/gui-check/config/minimalist-list/settings.json ]; then
  printf '%s\n' '{"global_shortcut":""}' > target/gui-check/config/minimalist-list/settings.json
fi
TMPDIR="$PWD/target/gui-check/tmp" \
XDG_CONFIG_HOME="$PWD/target/gui-check/config" \
XDG_DATA_HOME="$PWD/target/gui-check/data" \
EGUI_INSPECTION=127.0.0.1:5741 \
cargo run --locked --features eframe/inspection -- --data-dir target/gui-check/workspace
```

Attach the GUI inspector to port 5741. Use a distinct port and `XDG_CONFIG_HOME` for a second instance, including when testing two instances against one workspace. Otherwise the second launch reveals the first instance. Keep `TMPDIR`, settings, data, screenshots, and logs under `target/`; leave `HOME` unchanged. See [platform paths](storage.md#default-directories).

### Running alongside an installed copy

The fresh test profile above disables the global shortcut so it cannot capture the installed app's binding. Disable the shortcut in existing test profiles too. For an explicit global-shortcut check, use a different chord or quit the regular instance first, then disable the test binding afterward. Keep environment overrides local to the test launch rather than exporting them into a shell used for ordinary builds or installation.

For macOS automation, reuse one development bundle and identifier, separate from the installed app, and launch each by its full path. Rebuilding or reinstalling does not launch the installed copy or stop an older process. Both windows can have the same title, so verify the executable path and selected test workspace before interacting.

Spotlight can index generated `.app` bundles under the ignored `target/` directory, including copies inside installer fixtures. After testing, archive any bundles needed for reuse and remove their extracted `.app` directories. Restore the same development name, identifier, and path for the next check. A separate settings directory protects task data but does not prevent Spotlight from launching the test copy.

Finish every GUI check, including a user-assisted keyboard check, by **quitting the test instance and confirming that its process has exited**. Closing its window only hides it in the tray; it continues to own its shortcut and instance claim. Disable any test shortcut, clean up generated bundles, and open the installed copy explicitly when returning to everyday use.

### Interaction checks

Check paths, shortcuts, accessibility, and transparency on each native platform; cross-compilation and plist parsing do not validate native behavior.

| Area | Actions and checks |
| --- | --- |
| [Tasks and navigation](usage.md#capture-and-edit-tasks) | Exercise pointer and keyboard flows. Inspect saved JSON after edits, Undo, archive, and Move, then restart. |
| [Lists](usage.md#manage-lists) and [task sorting](usage.md#complete-reorder-and-remove-tasks) | Drag unequal-height tiles and rows in Manual order, try every sort, then return to Manual. Add, edit, complete, move, delete, undo, and restore tasks while sorted; check stored order and restart persistence. Rename a list and inspect its [filename](storage.md#file-layout). |
| [Context and copy](usage.md#add-task-context) | Save and cancel multiline notes, inspect archived context, and copy from rows and editors. Compare clipboard text with current title, context, and child edits. Repeat in a narrow window and through save failure/retry. |
| [Subtask previews](usage.md#capture-and-edit-tasks) | Enable previews, collapse separate parents, and open the full checklist. Toggle completion and check saved state. |
| [Layout and appearance](usage.md#window-and-appearance-settings) | Test both title modes, 320- and 520-point windows, maximum text size in both fonts and weights, pinning, and opacity over bright and dark backgrounds. Use long text, explicit newlines, and many History rows; completion, Delete, Add, and Restore must remain reachable. Restart to check window placement. |
| [Shared folders](storage.md#file-sync-behavior) | Use two isolated instances against one workspace. Edit and externally replace list files, including while inline editing, dragging, viewing context, and hidden. Switch workspace folders, use Open workspace folder, then restart to check the selection. |
| [Tray and shutdown](usage.md#startup-and-system-tray) | Close and minimize where supported, with decorations on and off. Hide during an inline edit, restore through the tray, then Quit while hidden. Confirm pending clear/delete actions saved and the process and icon exited. Repeat without a tray host and with the host disappearing while hidden. |
| Launch coordination | Test ordinary and `--start-hidden` launches, then repeat each with the same workspace and settings. Test app-menu and sign-in entries with installation paths containing spaces; inspect the startup entry and verify sign-in through a real desktop login. Windows release builds must open without a console. |
| Taskbar and window state | On Windows and X11, check visible and hidden windows, repeated reveal and global shortcut activation, and tray-host removal and restoration. Test both pinned and unpinned states after every reveal, including while another application has focus; check maximization and preserve unrelated X11 state flags. |
| macOS bundle | Follow the [installation commands](../README.md#install-and-run), then run `plutil -lint` on both generated plists. Test close, disabled minimize, menu-bar Show and Quit, Finder or `open` reopening, and hidden sign-in without focus theft. Repeat with decorations disabled, a workspace path containing spaces, and an inline edit before hiding. Restart to check the task and workspace selection. Verify light/dark menu bars and Command shortcuts on Apple Silicon and Intel Macs. |
| [Save recovery](storage.md#file-sync-behavior) | Obstruct an isolated list directory, then add/edit a task and try Quick add. Check retained content, retry, and exactly one saved copy after restart. Repeat with unavailable startup storage, a pending rename before shutdown or workspace switching, and Move failures at destination and source writes. Deleting a dirty list must clear its retry state. |
