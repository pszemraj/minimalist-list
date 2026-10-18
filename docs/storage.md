# Storage and sync

## Workspace location

Select a folder through **Settings -> Workspace -> Choose folder**, or enter a path and press Enter or **Use typed path**. The active folder is shown above the field. Any writable folder works, including a temporary directory. Selecting a folder opens its lists without moving the previous workspace's files. A blank typed path does nothing. Pending task edits and list renames save before switching; a failed save keeps the original workspace open for recovery.

Paths beginning with `~/` are expanded. Relative paths are resolved from the process working directory and stored as absolute runtime paths. Without a saved selection, the app uses its [platform default](#default-directories).

The Settings field accepts paths with spaces, including paths surrounded by matching single or double quotes. On macOS and Linux it also accepts shell escapes for spaces and punctuation, as produced by dropping a folder into a terminal, such as `~/Dropbox\ \(Personal\)/minimalist-task-lists`. Folder-picker results and saved paths are used literally.

For multiple computers, select the same shared subfolder once on each computer. Each installation remembers its own local path, even when the sync root is different. Dropbox and other file-sync services work with these ordinary files; no provider integration is needed.

Use `--data-dir` to select a workspace for one launch:

```bash
minimalist-list --data-dir "$HOME/Sync/minimalist-list"
```

`MINIMALIST_LIST_WORKSPACE` provides the same launch-only behavior through the environment:

```bash
MINIMALIST_LIST_WORKSPACE="$HOME/Sync/minimalist-list" cargo run
```

Workspace selection follows this order:

1. `--data-dir`
2. `MINIMALIST_LIST_WORKSPACE`
3. The path saved through Settings
4. The platform default

The flag and environment variable do not replace the saved setting. Choosing a folder in the application does. Run `minimalist-list --help` for the command summary.

If the selected workspace is temporarily unavailable at startup, its saved path is retained. The visible warning offers **Retry workspace** or **Choose workspace…**. Restore the original folder and retry to reopen it; navigating a temporary fallback does not replace the selection.

## Default directories

Appearance, pinning, and workspace preferences are stored in the local settings file on each computer. Explicit nonempty `XDG_CONFIG_HOME` and `XDG_DATA_HOME` override the native settings and default data roots on every platform, respectively. The table shows defaults when those overrides are absent.

| Platform | Default workspace | Local settings file |
| --- | --- | --- |
| Linux | `${XDG_DATA_HOME:-$HOME/.local/share}/minimalist-list/` | `${XDG_CONFIG_HOME:-$HOME/.config}/minimalist-list/settings.json` |
| macOS | `~/Library/Application Support/minimalist-list/` | `~/Library/Application Support/minimalist-list/settings.json` |
| Windows | `%LOCALAPPDATA%\minimalist-list\` | `%APPDATA%\minimalist-list\settings.json` |

When the Windows environment variables are absent or empty, paths fall back to `AppData\Local` and `AppData\Roaming` beneath the home directory. An absent or empty `USERPROFILE` falls back to `HOME`.

Earlier versions used the Linux-style XDG paths on macOS and Windows too. If the
native default workspace directory does not exist, an existing
`${XDG_DATA_HOME:-$HOME/.local/share}/minimalist-list/` is used. Independently, if
the native settings file does not exist, an existing
`${XDG_CONFIG_HOME:-$HOME/.config}/minimalist-list/settings.json` remains the
settings file for both loading and saving. Legacy lookup uses `HOME`, including
on Windows, as earlier versions did. No files are copied or moved. Fresh installs
use the native defaults, and launch overrides and saved workspace selections
keep the precedence shown above.

`window.ron`, beside `settings.json`, stores native window geometry and framework state. Local `instances/` files coordinate launches for each workspace; they do not belong in a synced list folder. Unfinished entry drafts and deletion Undo are session-only and are not written into list JSON.

## File layout

The workspace contains one JSON file per list and no shared index:

```text
minimalist-list/
+-- lists/
    +-- errands--1f0f73de-57a8-4c8c-a2ae-3aa5efdc4571.json
    +-- projects--70d0f50d-8d45-4d24-88ce-bf82cbfab48a.json
```

New filenames combine a safe, bounded list title with its stable UUID. Renaming a list updates its filename after saving its contents; name collisions never replace another file. Existing UUID-only filenames remain readable and ordinary saves keep their paths. The filename UUID is authoritative.

Default list order comes from `created_at_unix`, with the UUID as a stable tie-breaker. Manual tile order and temporary list/task sort modes are local workspace-scoped settings. Task JSON always retains manual task order, including while a sorted view is active.

## File-sync behavior

Edits to different lists write different files, so file-sync services can transfer them independently.

> [!IMPORTANT]
> Minimalist List does not merge simultaneous changes to the same list. Let one device finish syncing before editing that list on another device.

If files contain the same list UUID, a canonical descriptive filename wins over a legacy UUID-only filename, which wins over arbitrary conflict-copy names. Unselected copies remain untouched and the application reports them.

Every edit writes and flushes a temporary sibling file, then replaces the destination. The app checks disk every five seconds and reloads externally changed list files at the next scan. Reloads wait until task or child editing and dragging finish, and while unsaved list changes need recovery. A sync service controls when a file arrives on another computer.

A failed write leaves its changes in memory, shows a visible warning, and offers **Retry save**. Restore folder access and retry before exiting or switching workspaces. Quick add instead retains its draft and destination until its own retry succeeds. Moving a task writes the destination before removing the source and attempts to roll back both lists if either write fails; the two list files are not a single atomic transaction.

## JSON format

List files are pretty-printed and hand-editable. `format_version` is currently `1`.

| Object | Fields |
| --- | --- |
| List | `format_version`, `id`, `title`, `created_at_unix`, `accent`, `tasks`, `archive` |
| Task | `id`, `text`, `completed`, `created_at_unix`, `completed_at_unix`, `subtasks`, optional `context` |
| Subtask | `id`, `text`, `completed` |
| Archive entry | `task`, `archived_at_unix` |

Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.

Missing `context` loads as empty; an empty context is omitted when writing. Context follows a task through Move, Undo, and History. Older application versions do not preserve this new field when rewriting a task.
