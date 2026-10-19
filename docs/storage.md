# Storage and sync

## Workspace location

Select a folder through **Settings -> Workspace -> Choose folder**, or enter a path and press Enter or **Use typed path**. The active folder is shown above the field. Any writable folder works, including a temporary directory. Selecting a folder opens its lists without moving the previous workspace's files. A blank typed path does nothing. Pressing Enter without changing the path leaves the current list open; explicitly applying the folder reloads it. Pending edits follow [save recovery](#save-recovery).

**Workspace -> Open workspace folder** opens the active folder in Finder, Explorer, or the system file manager.

Paths beginning with `~/` are expanded. Relative paths are resolved from the process working directory and stored as absolute runtime paths. Without a saved selection, the app uses its [platform default](#default-directories).

The Settings field accepts paths with spaces, including paths surrounded by matching single or double quotes. On macOS and Linux it also accepts shell escapes for spaces and punctuation, as produced by dropping a folder into a terminal, such as `~/Dropbox\ \(Personal\)/minimalist-task-lists`. Folder-picker results and saved paths are used literally.

For multiple computers, select the same shared subfolder once on each computer. Each installation remembers its own local path, even when the sync root is different. Dropbox and other file-sync services work with these ordinary files; no provider integration is needed.

From the checkout, use `--data-dir` to select a workspace for one launch:

```bash
cargo run --locked -- --data-dir "$HOME/Sync/minimalist-list"
```

`MINIMALIST_LIST_WORKSPACE` provides the same launch-only behavior through the environment:

```bash
MINIMALIST_LIST_WORKSPACE="$HOME/Sync/minimalist-list" cargo run --locked
```

Workspace selection follows this order:

1. `--data-dir`
2. `MINIMALIST_LIST_WORKSPACE`
3. The path saved through Settings
4. The platform default

The flag and environment variable do not replace the saved setting. Choosing a folder in the application does. Run `cargo run --locked -- --help` for the command summary.

If the selected workspace is temporarily unavailable at startup, its saved path is retained. The recovery screen offers **Retry workspace** or **Choose workspace…** without opening another workspace's lists. Restore access and retry to reopen it. Retrying a launch-only override does not replace the saved selection; choosing another folder does.

## Default directories

Appearance, pinning, and workspace preferences are stored in the local settings file on each computer. Explicit nonempty `XDG_CONFIG_HOME` and `XDG_DATA_HOME` override the native settings and default data roots on every platform, respectively. The table shows defaults when those overrides are absent.

| Platform | Default workspace | Local settings file |
| --- | --- | --- |
| Linux | `~/.local/share/minimalist-list/` | `~/.config/minimalist-list/settings.json` |
| macOS | `~/Library/Application Support/minimalist-list/` | `~/Library/Application Support/minimalist-list/settings.json` |
| Windows | `%LOCALAPPDATA%\minimalist-list\` | `%APPDATA%\minimalist-list\settings.json` |

When the Windows environment variables are absent or empty, paths fall back to `AppData\Local` and `AppData\Roaming` beneath the home directory. An absent or empty `USERPROFILE` falls back to `HOME`.

If the native defaults do not exist on macOS or Windows, the app uses an existing `~/.local/share/minimalist-list/` workspace or `~/.config/minimalist-list/settings.json` settings file. Each fallback is independent; the settings file is used for both loading and saving. These legacy paths use `HOME`, including on Windows. No files are copied or moved. Launch overrides and saved workspace selections keep the precedence shown above.

`window.ron`, beside `settings.json`, stores native window geometry and framework state. Local `instances/` files coordinate [repeated launches](usage.md#startup-and-system-tray) for each workspace; they do not belong in a synced list folder.

`settings.lock` serializes local preference merges across running instances, preserving unrelated changes made by different workspaces.

Malformed settings show a startup warning and use defaults. The next successful preference save replaces the malformed file with the current preferences; file read errors remain save errors.

## File layout

The workspace contains one JSON file per list and no shared index:

```text
minimalist-list/
+-- lists/
    +-- errands--1f0f73de-57a8-4c8c-a2ae-3aa5efdc4571.json
    +-- projects--70d0f50d-8d45-4d24-88ce-bf82cbfab48a.json
```

New filenames combine a safe, bounded list title with its stable UUID. Renaming a list saves its contents before updating its filename; name collisions never replace another file. If the filename update fails, the contents stay saved at the old path and **Retry save** retries the rename. Existing UUID-only filenames remain readable and ordinary saves keep their paths. The filename UUID is authoritative.

Default list order comes from `created_at_unix`, with the UUID as a stable tie-breaker. Manual tile order and temporary list/task sort modes are local workspace-scoped settings. Task JSON always retains manual task order, including while a sorted view is active.

## File-sync behavior

Edits to different lists write different files, so file-sync services can transfer them independently.

> [!IMPORTANT]
> Minimalist List does not merge simultaneous changes to the same list. Let one device finish syncing before editing that list on another device.

Every edit writes and flushes a temporary sibling file, then replaces the destination. The app checks disk every five seconds and reloads externally changed list files at the next scan. Reloads wait until task or child editing and dragging finish, while any task context dialog is open, and while unsaved list changes or filename updates need recovery. A sync service controls when a file arrives on another computer.

### Duplicate list files

If files contain the same list UUID, filenames with a recognized UUID take precedence over arbitrary conflict-copy names. Among recognized filenames, the most recently modified file wins. Timestamp ties prefer a descriptive filename matching its title, then filename order. Unselected copies remain untouched and the application reports them. This selection does not merge conflicting edits. If another readable file has the same list identity, resolve the duplicate files before deleting the list.

### Save recovery

A failed write leaves its changes in memory, shows a visible warning, and offers **Retry save**. Pending task edits and list renames must save before exiting or switching workspaces; a failed save keeps the original workspace open for recovery. Restore folder access and retry. A failed Quick add retains its draft and destination and blocks shutdown or workspace switching until it is retried successfully or explicitly cancelled; task context keeps its editor open with the draft and a **Retry save** button. Moving a task writes the destination before removing the source and attempts to roll back both lists if either write fails; the two list files are not a single atomic transaction.

## JSON format

List files are pretty-printed and hand-editable. `format_version` is currently `1`.

| Object | Fields |
| --- | --- |
| List | `format_version`, `id`, `title`, `created_at_unix`, `accent`, `tasks`, `archive` |
| Task | `id`, `text`, `completed`, `created_at_unix`, `completed_at_unix`, `subtasks`, optional `context` |
| Subtask | `id`, `text`, `completed` |
| Archive entry | `task`, `archived_at_unix` |

Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.

Missing `context` loads as empty; an empty context is omitted when writing. Older versions without task-context support discard this field when rewriting a list. See [task context](usage.md#add-task-context) for editing and copying notes.
