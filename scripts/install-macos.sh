#!/usr/bin/env bash
# Install a menu-bar utility for the current user and start it at sign-in.
set -eu
repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
staging="$repository/target/macos-install"
app="$HOME/Applications/Minimalist List.app"
agents="$HOME/Library/LaunchAgents"
label=io.github.pszemraj.minimalist-list

cargo install --locked --path "$repository" --root "$staging" --force
mkdir -p "$app/Contents/MacOS" "$agents"
cp "$staging/bin/minimalist-list" "$app/Contents/MacOS/minimalist-list"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>minimalist-list</string>
  <key>CFBundleIdentifier</key><string>$label</string>
  <key>CFBundleName</key><string>Minimalist List</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
EOF

# ProgramArguments are literal strings; only XML characters need escaping.
binary_xml=$(printf '%s' "$app/Contents/MacOS/minimalist-list" | \
  sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')
cat > "$agents/$label.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>ProgramArguments</key>
  <array>
    <string>$binary_xml</string>
    <string>--start-hidden</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
</dict>
</plist>
EOF

printf '%s\n' 'Installed ~/Applications/Minimalist List.app. It starts in the menu bar at your next sign-in.'
