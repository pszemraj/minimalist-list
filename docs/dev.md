# Development notes

## Shortcut boundary

All current shortcuts are application-scoped. `Ctrl+N` opens Quick add and `Ctrl+F` opens Find only while the Minimalist List window has keyboard focus.

System-wide capture is intentionally deferred. It would require a separate design for application lifecycle, single-instance behavior, platform support, and a non-conflicting shortcut. A future global shortcut must not reuse the application-scoped `Ctrl+N` binding. There is currently no tray process, background daemon, launcher capture mode, or global hotkey registration.

## Source layout

Keep source files below 1,000 lines when a behavior-level split is natural. No source file may reach 1,500 lines. Prefer modules based on actual responsibilities over generic layers or arbitrary line-count splits.

The `app` module keeps core state and navigation in `src/app.rs`; its child modules own overview, list, task-detail, settings, overlay, and shared visual behavior.

## Verification

Before committing a change, run:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
