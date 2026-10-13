# Minimalist List

Minimalist List is a native desktop task list built with `egui` and `eframe`, inspired by MinimaList. It runs without an account or hosted service.

<img src="assets/ui.png" alt="Minimalist List on Windows in the default Charcoal palette, showing tasks, subtasks, and completion" width="320">

*Default Charcoal appearance on Windows, with sample tasks and an expanded checklist.*

## Install and run

Install [Rust](https://rust-lang.org/tools/install/) 1.95 or newer on Linux, macOS, or Windows. On Windows, install the Visual Studio C++ build tools when prompted by the Rust installer. On macOS, install Xcode Command Line Tools (`xcode-select --install`) if they are not already present. On Ubuntu, install the native windowing and OpenGL build dependencies:

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

If Rust is already installed through rustup, update it and build from the repository directory:

```bash
rustup update stable
rustc +stable --version
cargo +stable build --locked --release
```

Confirm that the reported version is at least 1.95. Cargo's [`rust-version`](https://doc.rust-lang.org/cargo/reference/rust-version.html) rejects older compilers; it does not upgrade them. The explicit [`+stable`](https://rust-lang.github.io/rustup/overrides.html#toolchain-override-shorthand) selects the updated toolchain even if an older directory override exists.

On macOS or Linux, if `rustup` is missing or `+stable` fails, check which installation supplies the commands:

```bash
command -v rustc cargo rustup
```

A separate installation, such as Homebrew's Rust, must be upgraded through its package manager, or replaced on `PATH` by the rustup installation.

Check the version without opening a window:

```bash
cargo run -- --version
```

Build and run an optimized binary with:

```bash
cargo build --release
./target/release/minimalist-list
```

On Windows, run `.\target\release\minimalist-list.exe` instead.

`Cargo.lock` is committed, so these commands use the dependency versions shipped with the application.

To install for the current user on Windows, run from PowerShell:

```powershell
.\scripts\install-windows.ps1
```

On Linux:

```bash
bash scripts/install-linux.sh
```

On macOS:

```bash
bash scripts/install-macos.sh
```

These scripts build the release binary and enable startup at sign-in. Windows and Linux get an app-menu shortcut; macOS gets `~/Applications/Minimalist List.app`, which runs as a menu-bar utility without a Dock icon. Sign-in starts hidden; an ordinary fresh launch shows the window. Close or minimize hides the window, and the tray menu provides **Show Minimalist List** and **Quit**. Quit the running app before reinstalling. On Linux this requires an X11 or XWayland session with a desktop tray host; without one, the window opens normally.

Startup entries use the installed binary's absolute path and do not require changing `PATH`. See [startup and tray behavior](docs/usage.md#startup-and-system-tray) to disable startup or launch manually. macOS installation and menu-bar behavior require native validation; cross-platform checks alone do not establish them.
The app and tray use the same checklist shape on Windows, macOS, and Linux;
macOS renders the tray icon using the menu bar's appearance. Linux Wayland
desktops use the installed desktop entry and icon for the app launcher and switcher.

## Start using it

Open a list and type `Buy groceries` into the field at the top. Enter adds the task at the top of the list and returns focus to the field. See [list management](docs/usage.md#manage-lists) to create more lists.

Press Backspace (`Delete` on macOS) when task entry is empty to return to Lists. Use arrows to choose a list and Enter to open it.

Press `Ctrl+/` (`Cmd+/` on macOS) to see the keyboard shortcuts in the app.

## Documentation

- [Tasks, subtasks, search, history, and appearance](docs/usage.md)
- [Workspace folders, settings paths, and file sync](docs/storage.md)
- [Source layout, checks, and GUI inspection](docs/dev.md)
