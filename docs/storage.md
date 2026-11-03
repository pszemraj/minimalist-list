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

Choosing a folder in the application saves it for the next launch. Run `cargo run --locked -- --help` for the command summary.

If the selected workspace is unavailable at startup, the app offers **Retry workspace** and **Choose workspace...**. The same screen appears if the open workspace's `lists` folder disappears, becomes unreadable, or loses all its list files during a scan. Restore access and retry to reopen it with the current selection.

The app creates missing folders and starts with a list when opening an empty platform default, launch override, or folder explicitly chosen in Settings. Other saved folders must already contain list files. This protects an unmounted or still-syncing workspace from accidental initialization.

## Default directories

Appearance, pinning, and workspace preferences are stored in the local settings file on each computer. Explicit nonempty `XDG_CONFIG_HOME` and `XDG_DATA_HOME` override the native settings and default data roots on every platform, respectively. The table shows defaults when those overrides are absent.

| Platform | Default workspace | Local settings file |
| --- | --- | --- |
| Linux | `~/.local/share/minimalist-list/` | `~/.config/minimalist-list/settings.json` |
| macOS | `~/Library/Application Support/minimalist-list/` | `~/Library/Application Support/minimalist-list/settings.json` |
| Windows | `%LOCALAPPDATA%\minimalist-list\` | `%APPDATA%\minimalist-list\settings.json` |

When the Windows environment variables are absent or empty, paths fall back to `AppData\Local` and `AppData\Roaming` beneath the home directory. An absent or empty `USERPROFILE` falls back to `HOME`.

If the native defaults do not exist on macOS or Windows, the app uses an existing `~/.local/share/minimalist-list/` workspace or `~/.config/minimalist-list/settings.json` settings file. Each fallback is independent; the settings file is used for both loading and saving. These legacy paths use `HOME`, including on Windows. No files are copied or moved. Launch overrides and saved workspace selections keep the precedence shown above.

`window.ron`, beside `settings.json`, stores native window geometry and framework state. Local `instances/` files coordinate [repeated launches](usage.md#startup-and-system-tray) for each workspace.

`settings.lock` serializes preference saves across running instances. Each save merges changed preferences into the file's current values, including valid values recovered from a partially malformed file.

Malformed settings show a startup warning. Each preference that still parses is kept and any malformed one uses its default; a file that is unreadable or not a JSON object uses all defaults. Before replacing the file, a preference save preserves the original bytes beside it as `settings.invalid-<UUID>.json`. File read errors remain save errors. Unchanged preferences leave the settings file untouched; continuous appearance adjustments save after a short pause and flush when leaving the application.

## File layout

The workspace contains one JSON file per list and no shared index:

```text
minimalist-list/
+-- lists/
    +-- errands--1f0f73de-57a8-4c8c-a2ae-3aa5efdc4571.json
    +-- projects--70d0f50d-8d45-4d24-88ce-bf82cbfab48a.json
```

New filenames combine a safe, bounded list title with its stable UUID. Renaming a list saves its contents before updating its filename; name collisions never replace another file. If the filename update fails, the contents remain at the previous path with a warning. Quitting, switching workspaces, and reloads remain available. After resolving the filename problem, change the title again to retry; submitting an unchanged title does nothing. Existing UUID-only filenames remain readable and ordinary saves keep their paths. The filename UUID is authoritative.

Default list order comes from `created_at_unix`, with the UUID as a stable tie-breaker. The app remembers tile order and the chosen list and task sort modes locally for each workspace. The `tasks` array keeps Manual order for [every sort mode](usage.md#complete-reorder-and-remove-tasks). Preferences for deleted lists are removed when that workspace is loaded.

## File-sync behavior

Sync services transfer the [per-list files](#file-layout) independently.

> [!IMPORTANT]
> Minimalist List does not merge simultaneous changes to the same list. Let one device finish syncing before editing that list on another device.

A list save writes and flushes a temporary sibling file before replacing the destination. It compares the destination with its last loaded or saved bytes, including immediately before replacement, even when the timestamp and size are unchanged. A timestamp-only change permits saving. If the bytes changed, the external file stays in place and the local edit is kept for [recovery](#save-recovery). A change between the final check and replacement is still possible.

On startup and workspace loading, the app removes recognized unlocked temporary files older than a day.

The app checks disk every five seconds and reloads externally changed list files at the next scan. Reloads wait until task or child editing and dragging finish, while any task context dialog is open, and while unsaved list changes need recovery. A sync service controls when a file arrives on another computer.

### Duplicate list files

If files contain the same list UUID, filenames with a recognized UUID take precedence over arbitrary conflict-copy names. Among recognized filenames, the most recently modified file wins. Timestamp ties prefer a descriptive filename matching its title, then filename order. Unselected copies remain untouched and the application reports them. This selection does not merge conflicting edits. If another readable file has the same list identity, resolve the duplicate files before deleting the list.

### Save recovery

Before quitting or switching workspaces, the app saves inline edits, list title changes, open context drafts, and accepted archive/delete actions. Hiding also saves open editors and list names. A failed write retains changes in memory and keeps the window and workspace available. Restore folder access and choose **Retry save**.

A failed [Quick add](usage.md#quick-add) keeps its typed draft, any pasted batch, and destination, even when Find hides it. **Retry save** saves a failed batch on its own and leaves any separately typed text in the entry. Task context keeps its editor open with the draft. External-edit conflicts offer local-copy recovery. Resolve or cancel the failed edit before quitting or switching workspaces.

If another device changed the list, choose **Keep local copy and reload** in the save warning or failed editor. It saves the local contents in a separate **(local copy)** list with new list, task, and child IDs, then reloads the original. The copy includes current inline edits, editable context, active or suspended Quick add drafts, and an unfinished list-name change for that list. Both versions remain ordinary JSON files. Resolve each conflicting list before quitting or switching workspaces.

Moving a task writes the destination before removing the source and attempts to roll back both lists if either write fails; the two list files are not a single atomic transaction.

## Local backups

Saves, Clear History, and list deletion keep earlier list files in `backups/` beside the [local settings file](#default-directories), outside the workspace. Each workspace has a subfolder named by a 16-digit hexadecimal key derived from its path. **Settings -> Workspace -> Open backups folder** opens the active workspace's subfolder, creating it if needed.

```text
backups/<workspace key>/
+-- snapshots/<list UUID>/<unix seconds>.json
+-- deleted/<unix seconds>-<list filename>
```

Before a save replaces a list file, its previous contents are copied to `snapshots/`. A save writes no snapshot while the newest one for that list is under an hour old, and the newest 100 snapshots per list are kept; other files in the folder are never removed. Each snapshot is the file as it was before a save, so edits that have not been saved yet are not captured, and the latest saved version is the list file itself. If a snapshot cannot be written, saving continues and the app warns once per session.

**Clear History** saves pending edits first, then takes a snapshot even when the newest one is under an hour old. If either step fails, the archived tasks stay in History.

**Delete list** moves the list file to `deleted/`. For legacy UUID-only filenames or names too long to take a timestamp prefix, the backup uses a bounded descriptive filename that preserves the list UUID. For a list opened through a symbolic link, the app saves a regular copy of its target and removes the link from the workspace. The app retains each deleted backup until you remove it.

To restore a snapshot, save any pending edits and quit the instance using the workspace. Replace the current list file's contents with the snapshot and keep the current filename, including its UUID. Reopen the app to load the restored list.

To restore a deleted list, copy its file from `deleted/` back into the workspace's `lists/` folder. The list returns with its original identity at the next scan. You can remove the leading `<unix seconds>-` from the filename. If an older backup has a `<unix seconds>-<UUID>.json` filename, remove the timestamp prefix so the filename UUID is used.

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
