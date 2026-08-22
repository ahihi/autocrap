#!/bin/bash
# Builds Autocrap.app into the repository root.
#
# The bundle resolves the binary and the configs relative to its own location, so it
# must stay in the repository root to work. Drag it to the Dock from there; the Dock
# keeps a reference to that path rather than a copy.
set -euo pipefail

packaging_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$packaging_dir/../.." && pwd)"
bundle="$repo_root/Autocrap.app"

echo "building autocrap..."
( cd "$repo_root" && cargo build --release )

echo "building the menu bar app..."
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"

swiftc -O "$packaging_dir/AutocrapMenu.swift" \
    -framework AppKit \
    -o "$bundle/Contents/MacOS/Autocrap"

cp "$packaging_dir/Info.plist" "$bundle/Contents/Info.plist"
cp "$packaging_dir/AppIcon.icns" "$bundle/Contents/Resources/AppIcon.icns"

# nudge Finder into noticing the icon rather than showing a generic one
touch "$bundle"

echo
echo "built $bundle"
echo "drag it to the Dock to keep it there."
