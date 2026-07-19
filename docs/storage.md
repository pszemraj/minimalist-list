# Storage and sync

## Workspace location

The default workspace is an XDG data directory:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/minimalist-list/
```

Change it through **Settings -> Workspace** with the native folder chooser or typed path field. Paths beginning with `~/` are expanded. Relative paths are resolved from the process working directory and stored as absolute runtime paths.

Set `MINIMALIST_LIST_WORKSPACE` for a launch-only override:

```bash
MINIMALIST_LIST_WORKSPACE="$HOME/Dropbox/minimalist-list" cargo run
```

The environment variable does not replace the workspace saved in settings. Choosing a folder in the application does.

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

Edits to different lists write different files, so file-sync services can transfer them independently. Concurrent edits to the same list are left to the sync provider. If Dropbox creates a conflict copy with the same list UUID, the canonical UUID-named file wins. The conflict copy remains untouched and the application reports it.

Every edit is saved automatically through a temporary sibling file that is flushed and renamed over the destination. The workspace is rescanned roughly every 800 ms, and externally changed list files are reloaded. There is no lock file, database, sync protocol, background daemon, or automatic conflict merge.

## JSON format

List files are pretty-printed and hand-editable. `format_version` is currently `1`.

| Object | Fields |
| --- | --- |
| List | `format_version`, `id`, `title`, `created_at_unix`, `accent`, `tasks`, `archive` |
| Task | `id`, `text`, `completed`, `created_at_unix`, `completed_at_unix`, `subtasks` |
| Subtask | `id`, `text`, `completed` |
| Archive entry | `task`, `archived_at_unix` |

Timestamps are Unix seconds. `accent` is one of `Mint`, `Charcoal`, `Crimson`, `Sand`, `Sky`, or `Lavender`.
