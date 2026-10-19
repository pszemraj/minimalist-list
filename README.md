# Minimalist List

Minimalist List is a native desktop task list built with `egui` and `eframe`, inspired by MinimaList. It runs without an account or hosted service.

<p align="center">
  <img src="assets/ui.png" alt="Minimalist List on Windows in the default Charcoal palette, showing tasks, subtasks, and completion" width="360">
  <br>
  <sub>Charcoal theme on Windows · Tasks and subtasks</sub>
</p>

## Install and run

Requires [Rust 1.95+](https://rustup.rs/), Visual Studio C++ build tools on Windows, and Xcode Command Line Tools on macOS (`xcode-select --install`). Ubuntu build dependencies:

```bash
sudo apt install \
  build-essential \
  pkg-config \
  libxkbcommon-dev \
  libwayland-dev \
  libx11-dev \
  libgl1-mesa-dev
```

Clone the repository and run the application:

```bash
git clone https://github.com/pszemraj/minimalist-list.git
cd minimalist-list
cargo run
```

For desktop installation and startup at sign-in, run the installer for your platform.

Windows (PowerShell):

```powershell
.\scripts\install-windows.ps1
```

Linux:

```bash
bash scripts/install-linux.sh
```

macOS:

```bash
bash scripts/install-macos.sh
```

Windows and Linux add an app-menu shortcut; macOS installs `~/Applications/Minimalist List.app`. The installers enable startup at sign-in. Quit before reinstalling. See [startup and tray behavior](docs/usage.md#startup-and-system-tray) for window controls and disabling startup.

Verify the checkout with `cargo test --locked`; `cargo run --locked -- --version` checks the executable. The [development guide](docs/dev.md#verification) lists the full checks.

## Start using it

Open a list and type `Buy groceries` into the field at the top. Enter saves the task and returns focus to the field. In Manual order, new tasks appear at the top. See [list management](docs/usage.md#manage-lists) to create more lists.

Right-click a task and choose **Add context…** to attach [notes and links](docs/usage.md#add-task-context).

Use [keyboard navigation](docs/usage.md#keyboard-shortcuts) to work without a mouse, or the [global reveal shortcut](docs/usage.md#global-reveal-shortcut) to show the running app from another application.

## Documentation

- [Tasks, subtasks, search, history, and appearance](docs/usage.md)
- [Workspace folders, settings paths, and file sync](docs/storage.md)
- [Source layout, checks, and GUI inspection](docs/dev.md)

Bundled bold fonts: [sources](assets/fonts/SOURCES.txt), [Ubuntu Font Licence](assets/fonts/Ubuntu-LICENSE.txt), and [Hack licenses](assets/fonts/Hack-LICENSE.txt).
