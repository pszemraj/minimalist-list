# Development notes

## Source layout

```text
src/main.rs                Native eframe window setup
src/app.rs                 Application state, navigation, syncing, and eframe wiring
src/app/list.rs            List screen and task interactions
src/app/overlays.rs        Quick add and cross-list search
src/app/overview.rs        List-card overview
src/app/settings.rs        Workspace, appearance, and list settings
src/app/task_details.rs    Subtasks, deletion, clearing, and history
src/app/task_text.rs       Title wrapping, hover scrolling, and reorder geometry
src/app/theme.rs           Accent palettes and shared rendering helpers
src/model.rs               Serializable list and task model
src/storage.rs             Platform paths, list files, polling, and atomic writes
```

Keep source files below 1,000 lines when a behavior-level split is natural. No source file may reach 1,500 lines. Prefer modules based on actual responsibilities over generic layers or arbitrary line-count splits.

## Shortcut boundary

The [current shortcuts](usage.md#find-and-navigate) operate inside the focused application. System-wide capture would require separate decisions about application lifecycle, single-instance behavior, platform support, and shortcut conflicts.

## Verification

Run the checks before committing:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
rustdoc-checker src --strict
```

For a live GUI inspection session, build with the optional eframe inspection feature and enable its local port:

```bash
EGUI_INSPECTION=1 cargo run --features eframe/inspection
```
