# Minimalist List

Minimalist List is a native desktop task list built with `egui` and `eframe`, inspired by MinimaList. It runs without an account or hosted service.

<img src="assets/ui.png" alt="UI screenshot" width="260">

## Install and run

Install [Rust](https://rust-lang.org/tools/install/) 1.95 or newer on Linux, macOS, or Windows. On Windows, install the Visual Studio C++ build tools when prompted by the Rust installer. On Ubuntu, install the native windowing and OpenGL build dependencies:

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

To install the binary and optional Linux desktop entry for the current user:

```bash
cargo install --locked --path .
mkdir -p ~/.local/share/applications
cp minimalist-list.desktop ~/.local/share/applications/
```

The desktop entry expects `minimalist-list` to be available on the desktop session's `PATH`.

## Start using it

Open a list and type `Buy groceries` into the field at the top. Enter adds the task at the top of the list and returns focus to the field. See [list management](docs/usage.md#manage-lists) to create more lists.

## Documentation

- [Tasks, subtasks, search, history, and appearance](docs/usage.md)
- [Workspace folders, settings paths, and file sync](docs/storage.md)
- [Source layout, checks, and GUI inspection](docs/dev.md)
