# Storage and sync

## Workspace location

Select a folder through **Settings -> Workspace -> Choose folder**, or enter a path and press Enter or **Use typed path**. The active folder is shown above the field. Any writable folder works, including a temporary directory. Selecting a folder opens its lists without moving the previous workspace's files.

Paths beginning with `~/` are expanded. Relative paths are resolved from the process working directory and stored as absolute runtime paths. Without a saved selection, the app uses its [platform default](#default-directories).

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

## Default directories

Appearance, pinning, and workspace preferences are stored in the local settings file on each computer.

| Platform | Default workspace | Local settings file |
| --- | --- | --- |
| Linux | `${XDG_DATA_HOME:-$HOME/.local/share}/minimalist-list/` | `${XDG_CONFIG_HOME:-$HOME/.config}/minimalist-list/settings.json` |
| macOS | `~/Library/Application Support/minimalist-list/` | `~/Library/Application Support/minimalist-list/settings.json` |
| Windows | `%LOCALAPPDATA%\minimalist-list\` | `%APPDATA%\minimalist-list\settings.json` |

When the Windows environment variables are absent, paths fall back to `AppData\Local` and `AppData\Roaming` beneath the home directory.

## File layout

The workspace contains one JSON file per list and no shared index:

```text
minimalist-list/
+-- lists/
    +-- 1f0f73de-57a8-4c8c-a2ae-3aa5efdc4571.json
    +-- 70d0f50d-8d45-4d24-88ce-bf82cbfab48a.json
```

List order comes from `created_at_unix`, with the UUID as a stable tie-breaker. The UUID filename is authoritative for a canonical list file.

## File-sync behavior

Edits to different lists write different files, so file-sync services can transfer them independently.

> [!IMPORTANT]
> Minimalist List does not merge simultaneous changes to the same list. Let one device finish syncing before editing that list on another device.

If a sync service creates a conflict copy containing the same list UUID, the canonical UUID-named file wins. The conflict copy remains untouched and the application reports it.

Every edit writes and flushes a temporary sibling file, then replaces the destination. The app checks disk every five seconds and reloads externally changed list files at the next scan. Reloads wait until inline editing or dragging finishes. A sync service controls when a file arrives on another computer.

## JSON format

List files are pretty-printed and hand-editable. `format_version` is currently `1`.

| Object | Fields |
| --- | --- |
| List | `format_version`, `id`, `title`, `created_at_unix`, `accent`, `tasks`, `archive` |
| Task | `id`, `text`, `completed`, `created_at_unix`, `completed_at_unix`, `subtasks` |
| Subtask | `id`, `text`, `completed` |
| Archive entry | `task`, `archived_at_unix` |

Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.
