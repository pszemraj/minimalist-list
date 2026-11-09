# Storage and sync

## Workspace location

Select a folder through **Settings -> Workspace -> Choose folder**, or enter a path and press Enter or **Use typed path**. The active folder is shown above the field. Any writable folder works, including a temporary directory. Selecting a folder opens its lists without moving the previous workspace's files. A blank typed path does nothing. Pressing Enter without changing the path leaves the current list open; explicitly applying the folder reloads it. Pending edits follow [save recovery](#save-recovery).

**Workspace -> Open workspace folder** opens the active folder in Finder, Explorer, or the system file manager. Selecting a populated `lists` folder opens its parent workspace unless the selected folder is already a workspace of its own.

Paths beginning with `~/` are expanded. Relative paths are resolved from the process working directory and stored as absolute runtime paths.

The Settings field accepts paths with spaces, including paths surrounded by matching single or double quotes. On macOS and Linux it also accepts shell escapes for spaces and punctuation, as produced by dropping a folder into a terminal, such as `~/Dropbox\ \(Personal\)/minimalist-task-lists`. Folder-picker results, saved paths, and launch overrides preserve surrounding spaces. Workspace paths must be valid UTF-8.

For multiple computers, select the same shared subfolder once on each computer. Each installation remembers its own local path, even when the sync root is different. Dropbox and other file-sync services work with these ordinary files; no provider integration is needed.

Use `--data-dir` to select a workspace for one launch:

```bash
cargo run --locked -- --data-dir "$HOME/Sync/minimalist-list"
```

Or set `MINIMALIST_LIST_WORKSPACE` for that launch:

```bash
MINIMALIST_LIST_WORKSPACE="$HOME/Sync/minimalist-list" cargo run --locked
```

Workspace selection follows this order:

1. `--data-dir`
2. `MINIMALIST_LIST_WORKSPACE`
3. The path saved through Settings
4. The platform default

Choosing a folder in the application saves it for the next launch. Run `cargo run --locked -- --help` for the command summary.

If the selected workspace is unavailable, the app offers **Retry workspace** and **Choose workspace...**. This can happen at startup or if the open workspace's `lists` folder disappears, becomes unreadable, or loses all its files. When access returns, a scan reloads accessible lists and keeps pending edits available for [recovery](#save-recovery). The saved list selection and ordering preferences survive the interruption.

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

Default list order comes from `created_at_unix`, with the UUID as a stable tie-breaker. The app remembers tile order and the chosen list and task sort modes locally for each workspace. By status keeps the previous setting value `"IncompleteFirst"`. The `tasks` array keeps Manual order for [every sort mode](usage.md#complete-reorder-and-remove-tasks). Tile positions and sort modes survive temporarily missing files. **Delete list** removes that list's local preferences.

## File-sync behavior

Sync services transfer the [per-list files](#file-layout) independently.

> [!IMPORTANT]
> Minimalist List does not merge simultaneous changes to the same list. Let one device finish syncing before editing that list on another device.

A list save writes and flushes a temporary sibling file before replacing the destination. It compares the destination with its last loaded or saved bytes, including immediately before replacement, even when the timestamp and size are unchanged. A timestamp-only change permits saving. If the bytes changed, the external file stays in place and the local edit is kept for [recovery](#save-recovery). A change between the final check and replacement is still possible.

On startup and workspace loading, the app removes recognized unlocked temporary files older than a day.

The app checks for external changes every five seconds. Reloading waits for inline edits, drags, and ordinary task context editing. If a context save fails or a conflict needs attention, other lists can reload while the affected list and draft stay in memory. If reloading a file fails, the app warns and keeps its last loaded contents if available. A sync service controls when a file arrives on another computer.

### Duplicate list files

If files contain the same list UUID, filenames with a recognized UUID take precedence over arbitrary conflict-copy names. Among recognized filenames, the most recently modified file wins. Timestamp ties prefer a descriptive filename matching its title, then filename order.

A conflicting copy can contain changes missing from your lists. Choose **Recover as separate list** to import it with independent list, task, and subtask IDs and keep the original file in local backups. The imported list syncs like any other list. Resolve duplicate files before deleting a list that shares their identity.

After importing a conflicting copy or keeping a local copy, a summary shows tasks found only in either version, changed tasks, and differences in list details or Manual order.

### Save recovery

Before quitting or switching workspaces, the app saves inline edits, list title changes, open context drafts, and accepted archive/delete actions. Hiding also saves open editors and list names. A failed write keeps the window open and the changes in memory until you resolve the write. If the app is waiting for the folder, restore access and choose **Retry save**.

A failed [Quick add](usage.md#quick-add) keeps its typed draft, any pasted batch, and destination, even when Find hides it. **Retry save** saves a failed batch on its own and leaves any separately typed text in the entry. A failed context save keeps its editor open with the draft.

When Quick add is the only local change to a list, it retries once against the latest file. For a conflict that needs attention, **Keep local copy and reload** saves a separate **(local copy)** list with new list, task, and child IDs. **Discard all unsaved changes** keeps the unsaved list in local backups and reloads the file on disk when available. Both actions preserve inline edits, editable context, active or suspended Quick add drafts, and an unfinished list-name change for that list. Discard is available for failed list writes, including when the workspace folder is unavailable. If the backup fails, your changes stay in memory.

Moving a task writes the destination before removing the source and attempts to roll back both lists if either write fails; the two list files are not a single atomic transaction.

## Local backups

Saves, Clear History, and list deletion keep earlier list files in `backups/` beside the [local settings file](#default-directories). On macOS, the default workspace contains `settings.json` and `backups/` alongside `lists/`. If you choose a separate Dropbox workspace, its backups stay with the local settings.

Each workspace has a subfolder named by a 16-digit hexadecimal key derived from its path. **Settings -> Workspace -> Open backups folder** opens the active workspace's subfolder, creating it if needed.

```text
backups/<workspace key>/
+-- snapshots/<list UUID>/<unix seconds>-<sequence>.json
+-- deleted/<unix seconds>-<title>--<list UUID>.json
+-- conflicted/<recovery UUID>/<original filename>
```

Before a save replaces a list file, its previous contents are copied to `snapshots/`. A save writes no snapshot while the latest backup for that list is under an hour old. Retention keeps up to 100 snapshots per list in creation order, using the sequence number even if the clock changes. Other files in the folder stay in place. If a snapshot cannot be written, saving continues and the app warns once per session.

**Clear History** saves pending edits first, then takes a snapshot even when the newest one is under an hour old. If either step fails, the archived tasks stay in History.

**Delete list** moves the list file to `deleted/` under a timestamped filename built from its title and UUID. For a list opened through a symbolic link, the app saves a regular copy of its target and removes the link from the workspace. The app retains each deleted backup until you remove it.

The `conflicted/` folder holds the original files for imported conflicting copies and the unsaved lists saved by **Discard all unsaved changes**.

To restore a snapshot or a discarded list, save any pending edits and quit the instance using the workspace. Replace the current list file's contents with the backup and keep the current filename, including its UUID. If the list file is gone, copy the discarded list back into `lists/`. Reopen the app to load the restored list.

To restore a deleted list, copy its file from `deleted/` back into the workspace's `lists/` folder. The list returns with its original identity at the next scan. You can remove the leading `<unix seconds>-` from the filename.

## JSON format

List files are pretty-printed and hand-editable.

| Object | Fields |
| --- | --- |
| List | `format_version`, `id`, `title`, `created_at_unix`, `accent`, `tasks`, `archive` |
| Task | `id`, `text`, `completed`, `created_at_unix`, `completed_at_unix`, `subtasks`, optional `context` and `started` |
| Subtask | `id`, `text`, `completed` |
| Archive entry | `task`, `archived_at_unix` |

Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.

Missing `context` loads as empty; an empty context is omitted when writing. See [task context](usage.md#add-task-context) for editing and copying notes.

`completed: true` marks a task Done. For an open task, `started: true` means In progress and `started: false` means Not started. If `started` is absent or null, any checked subtask implies In progress. The app omits the key when that value is unset. A string or number in `started` makes the list unreadable, as with other task fields of the wrong type.

### Format compatibility

The app supports list format `1` and checks that version before decoding task contents. Unsupported files remain on disk and produce a warning naming the file and supported version. Update the app on that computer before editing the affected list. Other supported lists can still open.

Saving an unsupported in-memory list is also refused. If another app replaces an open list with an unsupported file, the last loaded contents may remain visible, but the replacement cannot be overwritten, renamed, or deleted through that cached list.

Within the supported format, unknown JSON fields are retained on lists, tasks, subtasks, and archive entries. Editing, moving, or archiving a task keeps those fields with the objects that survive the operation. Deleting an object removes its fields too; restoring a task from History removes its archive entry, including fields belonging only to that entry.

Version 0.4.0 keeps a task's `started` field without showing its [In progress](usage.md#complete-reorder-and-remove-tasks) mark. Versions before 0.4.0 drop the field when they save the list, so update every device that shares a workspace.

Files containing the JSON key `$serde_json::private::Number` remain on disk with a warning because the JSON parser can interpret objects with that key as numbers.

Optional additions with defaults can keep the same format when existing editing behavior remains valid. Changes to field types, required structure, or behavior that older writers must understand require a new format and an explicit conversion from supported older formats. This includes new values for closed enums such as `accent`, since older readers reject unknown values. There is currently no older list format to migrate. Opening a workspace does not upgrade or rewrite all of its files. See [file-sync rules](#file-sync-behavior) for simultaneous edits.
