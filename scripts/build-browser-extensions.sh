#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"
bash scripts/build-safari-extension.sh
output_dir="$root_dir/release/browser-native"
mkdir -p "$output_dir"
sdk_path="$(xcrun --sdk macosx --show-sdk-path)"
for architecture in arm64 x86_64; do
  xcrun swiftc browser-extension/native/main.swift -sdk "$sdk_path" \
    -target "${architecture}-apple-macosx12.0" -O -framework AppKit \
    -o "$output_dir/bridge-$architecture"
done
lipo -create "$output_dir/bridge-arm64" "$output_dir/bridge-x86_64" -output "$output_dir/tdm-browser-bridge"
codesign --force --sign - "$output_dir/tdm-browser-bridge"
codesign --verify --strict "$output_dir/tdm-browser-bridge"
