# Minimalist List

Minimalist List is a native desktop to-do application written in Rust with `egui` and `eframe`. It is a compiled binary: there is no JavaScript, Electron, webview, embedded browser, account system, database, daemon, or cloud service.

The interface borrows the useful parts of MinimaList's interaction style—low chrome, typography-led controls, horizontal task gestures, a card overview, and interpolated movement—while behaving like a desktop application rather than pretending to be a phone UI.

## Build and run

The project requires Rust 1.92 or newer. On Ubuntu, install the native windowing/OpenGL build prerequisites once:

```bash
sudo apt install \
  build-essential \
  pkg-config \
  libxkbcommon-dev \
  libwayland-dev \
  libx11-dev \
  libgl1-mesa-dev
```

From the unzipped project directory:

```bash
cargo run
```

For an optimized binary:

```bash
cargo build --release
./target/release/minimalist-list
```

`Cargo.lock` is committed because this is an application, so both commands use the dependency versions shipped with the project.

An optional local desktop-entry installation is:

```bash
cargo install --path .
mkdir -p ~/.local/share/applications
cp minimalist-list.desktop ~/.local/share/applications/
```

The supplied desktop entry expects `minimalist-list` to be available on the desktop session's `PATH`.

## Interaction

The top field owns keyboard-first task creation within a list. It receives focus when a list opens, Enter adds the task at the top, and focus returns to the field immediately.

`Ctrl+N` opens Quick add from any screen while the application is focused. Its list selector defaults to the last successful capture destination, Enter adds one task and closes the overlay, and Escape cancels. The header's `add` action opens the same overlay.

`Ctrl+F` or the header's `find` action searches task and subtask text across every list, including completed tasks and History. Use Up and Down to select a result and Enter to open it; the owning task is scrolled into view and briefly highlighted.

Use the small circle at the left of a task to complete or reactivate it; it is a real keyboard-focusable control, so Tab followed by Space or Enter works as well. Click task text to edit it inline. The checklist and delete controls at the right become prominent on hover and remain keyboard-focusable. Drag a task vertically to reorder it. Drag right past the threshold for the same complete/reactivate action, with the strike-through following the drag. Drag left to reveal the original flat Edit and Delete actions, then drag right to close them.

Double-click a list title or use the back chevron to return to the card overview. `Ctrl+Tab` and `Ctrl+Shift+Tab` move directly between lists. Each overview card has its own settings action for renaming, recoloring, or deleting that list. The `pin`/`pinned` label in the header toggles always-on-top mode. Settings can also remove native window decorations; in that mode, drag the title to move the window and use the visible `x` control to close it.

Motion is interpolated rather than switched abruptly: screens slide, fade, and settle into place; list palettes cross-fade; overview cards lift under the pointer; newly added tasks expand into the list; deleted and cleared tasks fade and collapse; neighboring rows move out of the way during reordering; and the history and subtask sections open as clipped accordions.

Completed tasks can be moved into the restorable History section with "Clear completed" or `Ctrl+Shift+Backspace`. Each list has its own flat color theme. Typography settings control proportional versus monospace text, weight, size, and row spacing.

## Configuration

Settings are written as human-readable JSON at:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/minimalist-list/settings.json
```

All user-facing options save immediately.

| Setting                  |                            Default | Purpose                                                   |
| ------------------------ | ---------------------------------: | --------------------------------------------------------- |
| `workspace_path`         | XDG data directory described below | Root directory containing the `lists/` folder             |
| `always_on_top`          |                            `false` | Keeps the native window above ordinary windows            |
| `window_decorations`     |                             `true` | Enables the window manager's title bar and borders        |
| `font`                   |                           `"Sans"` | Uses proportional (`"Sans"`) or monospace (`"Mono"`) text |
| `font_size`              |                             `19.0` | Main task text size in egui points                        |
| `bold_text`              |                            `false` | Uses heavier interface text                               |
| `row_padding`            |                             `12.0` | Extra vertical space in each task row                     |
| `last_list_id`           |             `null` on first launch | Internal UUID used to reopen the last active list         |
| `last_capture_list_id`   |             `null` on first launch | Internal UUID used for the Quick add destination          |

Change the data location through Settings -> Workspace with the native folder chooser or the typed path field. Paths beginning with `~/` are expanded, and relative paths are resolved from the process's working directory before being stored as an absolute runtime location.

For a launch-only or launcher-level override, set `MINIMALIST_LIST_WORKSPACE`:

```bash
MINIMALIST_LIST_WORKSPACE="$HOME/Dropbox/minimalist-list" cargo run
```

The environment variable takes precedence for that launch without replacing the workspace saved in `settings.json`. Choosing a new folder in the Settings screen is explicit and does update the saved workspace.

## Data location and Dropbox behavior

Task data belongs in an XDG data directory, not a cache. The default workspace root is:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/minimalist-list/
```

Its layout is deliberately flat:

```text
minimalist-list/
+-- lists/
    +-- 1f0f73de-57a8-4c8c-a2ae-3aa5efdc4571.json
    +-- 70d0f50d-8d45-4d24-88ce-bf82cbfab48a.json
    +-- ...
```

There is exactly one JSON file per list and no shared index file. List order is derived from each file's `created_at_unix` value with the UUID as a stable tie-breaker. The UUID filename is authoritative for a canonical list file.

This means editing list 3 on machine A and list 5 on machine B writes two independent files. Dropbox can sync them without a shared-state collision. Concurrent edits to the same list are intentionally left to Dropbox. If Dropbox creates a conflict copy containing the same list UUID, the canonical UUID-named file is loaded first; the conflict copy is left untouched and the application reports it instead of deleting or rewriting it.

Every committed edit is auto-saved. A save writes a hidden temporary sibling, flushes it, and renames it over the destination, so an external syncer normally observes a complete JSON document rather than a partially written file. The workspace is rescanned roughly every 800 ms, and a list is reloaded when its file changes outside the application. There is no lock file, database, sync protocol, background daemon, or attempt to resolve provider conflicts.

## List JSON format

The files are pretty-printed and intended to remain understandable and hand-editable. A representative file is:

```json
{
  "format_version": 1,
  "id": "1f0f73de-57a8-4c8c-a2ae-3aa5efdc4571",
  "title": "Work",
  "created_at_unix": 1784400000,
  "accent": "Mint",
  "tasks": [
    {
      "id": "b47654ca-ec56-46e5-9c4b-d983741f299a",
      "text": "Review benchmark results",
      "completed": false,
      "created_at_unix": 1784400123,
      "completed_at_unix": null,
      "subtasks": [
        {
          "id": "6107d685-f4a3-4050-9b65-1c62592289d7",
          "text": "Check regressions",
          "completed": false
        }
      ]
    }
  ],
  "archive": []
}
```

`archive` contains objects with a complete `task` value and an `archived_at_unix` timestamp. Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.

## Project structure

```text
Cargo.toml                 Package and crates.io dependencies
Cargo.lock                 Locked application dependency graph
docs/dev.md                Development boundaries and verification commands
minimalist-list.desktop    Optional Linux desktop entry
src/main.rs                Native eframe window setup
src/app.rs                 Core app state, navigation, syncing, and eframe wiring
src/app/                   Overview, lists, settings, overlays, and shared UI modules
src/model.rs               Serializable task/list data model
src/storage.rs             XDG paths, per-list loading, polling, and atomic writes
```

The application intentionally does not implement accounts, priorities, projects, due dates, reminders, a tray icon, a server, system-wide hotkeys, GTK/libadwaita theming, or compositor-specific GNOME Shell widget behavior.
