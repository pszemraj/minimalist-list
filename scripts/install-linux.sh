#!/usr/bin/env bash
# Install for the current user and start in the tray at sign-in.
set -eu
repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
config_home=${XDG_CONFIG_HOME:-$HOME/.config}
install_root="$data_home/minimalist-list"

cargo install --locked --path "$repository" --root "$install_root" --force

mkdir -p "$data_home/applications" "$data_home/icons/hicolor/scalable/apps" \
  "$config_home/autostart"
cp "$repository/assets/minimalist-list.svg" "$data_home/icons/hicolor/scalable/apps/"

# Desktop entries unescape strings before parsing Exec; this is not shell quoting.
desktop_exec=$(printf '%s' "$install_root/bin/minimalist-list" | \
  sed 's/\\/\\\\\\\\/g; s/"/\\\\"/g; s/[$`]/\\\\&/g; s/%/%%/g')
write_entry() {
  cat > "$1" <<EOF
[Desktop Entry]
Type=Application
Name=Minimalist List
Comment=Native minimalist task lists
Exec="$desktop_exec"$2
Icon=minimalist-list
Terminal=false
Categories=Utility;Office;
StartupNotify=false
EOF
}
write_entry "$data_home/applications/minimalist-list.desktop" ''
write_entry "$config_home/autostart/minimalist-list.desktop" ' --start-hidden'

printf '%s\n' 'Installed Minimalist List. Open it from your app menu; it starts in the tray at your next sign-in.'
