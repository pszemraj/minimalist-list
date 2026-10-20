# AGENTS.md - Minimalist List

This file is intentionally tracked. Edit it only with the user's explicit permission for that request.
- The user's current instructions take precedence over this file.
- In turn, this file takes precendence over any user-level AGENTS, CLAUDE, etc files.

## Workspace boundaries

- Run commands from the repository root and use relative paths for repository file operations.
- Keep test workspaces, settings, screenshots, and logs under the ignored `target/` directory.
- Never inspect or modify personal settings, credentials, or data in `~/.ssh`, `~/.gitconfig`, `~/.config`, or `~/.local/share`.
- Installed compilers, dependencies, and validation tools may be used normally. Do not modify system configuration.
- Isolate GUI instances with separate `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, and `--data-dir` paths. Set `TMPDIR` for local tests so temporary fixtures also stay under `target/`.
- Do not interact with the user's running application or task files during tests.

## Project map

This repository has one native application, `minimalist-list`. There is no server, separate CLI client, `crates/` workspace, or LocalPaste tooling.

Read `README.md` and `docs/{dev,usage,storage}.md` before runtime changes.
Application state lives in `src/app.rs`; screen and interaction modules live in `src/app/`.
`src/model.rs` defines JSON data and `src/storage.rs` handles settings and list files.

```bash
cargo run
cargo run -- --help
cargo run -- --version
cargo build --locked --workspace --all-targets --all-features
```

## Scope and code

- Implement the requested behavior with the simplest working change. Do not add speculative features, abstractions, safeguards, or tooling.
- Compilation is not validation: exercise changed behavior and inspect the saved state.
- Preserve the current ordinary JSON format and file-sync behavior unless the user explicitly requests a change.
- Keep Rust files below 1,000 lines. Split by actual responsibilities rather than arbitrary line counts.
- Fix warnings rather than suppressing them. Use the existing codebase's level of error handling and documentation.
- Do not commit generated fixtures, screenshots, logs, or caches. List any new tracked files in the final summary.
- Keep prerequisites, installation, verification, and a first-use example directly in the README.
- Use the machine's `misc` environment for Python tools. Do not put environment-manager choices or local environment names in tracked documentation.
- Use Context7 for library API or migration documentation and the Semble skill for behavioral code search when relevant. Exact identifiers and known files can be searched with `rg`.

## Appearance changes

Preserve the established appearance and defaults unless the user requests a visual change. Bug fixes, refactoring, and accessibility work do not authorize a redesign. If a fix requires a broader visual change, explain its effect and get explicit approval before implementing it.

Use the [README screenshots](README.md) as the visual baseline: faint translucent card and panel tints, headers without backing panels, and the existing typography and spacing. Do not make shared surfaces opaque, add header backgrounds, or change default opacity as an incidental fix. Scope legibility fixes to the affected control.

For authorized visual changes, compare before and after with the same settings and window size, follow the [native appearance checks](docs/dev.md#interaction-checks), and report untested platforms. Do not replace screenshots or weaken visual regressions to accommodate an unapproved redesign.

## Validation before runtime commits

Start with the full check when changing Rust/runtime behavior:

```bash
cargo check --locked --workspace --all-targets --all-features
```

Set up local test directories without changing `HOME`:

```bash
mkdir -p target/agent-checks/tmp target/agent-checks/config target/agent-checks/data
export TMPDIR="$PWD/target/agent-checks/tmp"
export XDG_CONFIG_HOME="$PWD/target/agent-checks/config"
export XDG_DATA_HOME="$PWD/target/agent-checks/data"
```

Run the toolchain loop before committing a completed runtime change:

```bash
cargo fmt --all --check
cargo test --locked
cargo test --locked --features eframe/inspection
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo build --locked --workspace --all-targets --all-features
rustdoc-checker src --strict
git diff --check
```

Run the installed Markdown link checker after documentation changes.
For docs or repository metadata alone, use checks targeted to those changes.
For code movement, inspect for duplicate implementations and abandoned symbols; do not add an audit subsystem.
Temporary tests that read or write the same settings file must run serially or use separate settings directories.

## Native GUI validation

Use the optional egui inspector against an isolated application instance:

```bash
EGUI_INSPECTION=127.0.0.1:5741 cargo run --locked --features eframe/inspection -- --data-dir target/agent-checks/workspace
```

Use a distinct inspection port and settings directory for each instance. Close only test instances when finished.
GPU rendering is allowed when authorized by the user. For CPU rendering on Linux, set `LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe` before launching.

For changed GUI behavior, record the actual actions and expected visible state:

- Type a task and press Enter: the task is saved and entry keeps focus. Click its title, edit, and submit.
- Add multiple subtasks with Enter: each saves and subtask entry keeps focus. Toggle completion using pointer and keyboard.
- Press Ctrl+N: Quick add opens; choose a list and submit with Enter. Press Ctrl+F: find active and archived text, select with arrows, and open with Enter. Escape closes overlays.
- Reorder unequal-height wrapped rows and check saved order. Check horizontal completion and action-reveal gestures.
- Press Ctrl+Tab or Ctrl+Shift+Tab during a drag: navigation cancels the drag and the application remains usable. Clear completed with Ctrl+Shift+Backspace during a drag: tasks enter History without a crash.
- Check Scroll and Wrap, narrow windows and dialogs, large fonts, keyboard focus, pinning, decorations, and opacity endpoints over bright and dark backgrounds.
- Stop and restart the test instance: tasks, completion, order, appearance, and the selected workspace persist.
- When storage changes, use two instances on one workspace: create, complete, reorder, and externally replace a list file; verify the second instance reloads after the polling interval. Inline edits and drags defer reloads.
- Switch workspace folders: the selected folder opens, the old folder's files remain intact, and the selected path survives restart.

On macOS use Command for application shortcuts. Validate platform-specific paths, shortcuts, and transparency on the native OS; Linux results alone do not establish macOS or Windows behavior. Report checks that cannot be run.

## Git and completion

- Use a feature branch for implementation work and make atomic Conventional Commits at logical completion points after validation.
- Commit verified work locally without waiting for another request. Include companion documentation with its behavior change when appropriate.
- Stage changes to this file only with the user's explicit permission for that request. Never stage personal configuration, test data, or generated artifacts.
- Never push without an explicit request authorizing that push.
- Never merge locally or remotely, enable auto-merge, or create releases or tags. The user performs GitHub squash merges.
- Do not make minor or major version changes without specific approval. Keep the application version unchanged when requested.
- Do not change CI or deployment configuration without explicit approval. Local validation does not authorize new CI jobs, tests, or matrices.
- Before reporting completion, inspect the staged changes, commit the verified scope, and confirm the working tree is clean apart from deliberately untracked or ignored local files.

## Work loop

Read relevant docs -> inspect source -> make the scoped change -> full check -> formatting, tests, Clippy, build, rustdoc -> relevant native flows and saved-state/restart checks -> inspect diff -> local Conventional Commit -> concise result and any validation limits.

Reuse completed validation while its source and behavior remain unchanged. Repeat checks when a new change, failure, or unresolved concern warrants it.
