#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"
bash scripts/build-safari-extension.sh
output_dir="$root_dir/release/browser-native"
mkdir -p "$output_dir"
sdk_path="$(xcrun --sdk macosx --show-sdk-path)"
xcrun swiftc browser-extension/native/main.swift -sdk "$sdk_path" \
  -target "arm64-apple-macosx12.0" -O -framework AppKit \
  -o "$output_dir/tdm-browser-bridge"
codesign --force --sign - "$output_dir/tdm-browser-bridge"
codesign --verify --strict "$output_dir/tdm-browser-bridge"
