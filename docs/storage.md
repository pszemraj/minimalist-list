# Storage and sync

## Workspace location

On Linux, the default workspace is an XDG data directory:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/minimalist-list/
```

On macOS it is `~/Library/Application Support/minimalist-list/`; on Windows it is `%LOCALAPPDATA%\minimalist-list\` (falling back to the home directory's `AppData\Local`).

Change it through **Settings -> Workspace** with the native folder chooser or typed path field. The active folder is shown above the field. Any writable folder works, including a temporary directory. Selecting a folder opens its lists without moving the previous workspace's files. Paths beginning with `~/` are expanded. Relative paths are resolved from the process working directory and stored as absolute runtime paths.

> [!TIP]
> For multiple computers, select the same shared subfolder once on each computer. Each installation remembers its own local path, even when the sync root is different. Minimalist List uses ordinary files and does not require a provider-specific integration.

Use `--data-dir` to select a workspace for one launch:

```bash
minimalist-list --data-dir "$HOME/Sync/minimalist-list"
cargo run -- --data-dir "$HOME/Sync/minimalist-list"
```

`MINIMALIST_LIST_WORKSPACE` provides the same launch-only behavior through the environment:

```bash
MINIMALIST_LIST_WORKSPACE="$HOME/Sync/minimalist-list" cargo run
```

Workspace selection follows this order:

1. `--data-dir`
2. `MINIMALIST_LIST_WORKSPACE`
3. The path saved through Settings
4. The XDG default shown above

The flag and environment variable do not replace the saved setting. Choosing a folder in the application does. Run `minimalist-list --help` for the command summary or `minimalist-list --version` for the installed version.

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

Every edit is saved automatically through a temporary sibling file that is flushed and renamed over the destination. While the app is running, the workspace is rescanned every five seconds, and externally changed list files are reloaded. Reloads wait until inline editing or dragging finishes. A sync service controls when a file arrives on another computer; the polling interval starts from the local file's arrival. There is no lock file, database, sync protocol, background daemon, or automatic conflict merge.

Appearance, pinning, and workspace preferences stay local to each computer in `settings.json`: Linux uses `${XDG_CONFIG_HOME:-$HOME/.config}/minimalist-list/`, macOS uses `~/Library/Application Support/minimalist-list/`, and Windows uses `%APPDATA%\minimalist-list\` (falling back to `AppData\Roaming` in the home directory).

## JSON format

List files are pretty-printed and hand-editable. `format_version` is currently `1`.

| Object | Fields |
| --- | --- |
| List | `format_version`, `id`, `title`, `created_at_unix`, `accent`, `tasks`, `archive` |
| Task | `id`, `text`, `completed`, `created_at_unix`, `completed_at_unix`, `subtasks` |
| Subtask | `id`, `text`, `completed` |
| Archive entry | `task`, `archived_at_unix` |

Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.
