#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"
build_dir="$root_dir/src-tauri/target/safari-extension"
xcodebuild -project 'browser-extension/safari-project/TDM Safari Connector/TDM Safari Connector.xcodeproj' -scheme 'TDM Safari Connector' -configuration Debug -derivedDataPath "$build_dir" ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO CODE_SIGN_IDENTITY=- CODE_SIGNING_ALLOWED=YES build
mkdir -p release/safari
ditto "$build_dir/Build/Products/Debug/TDM Safari Connector.app" 'release/safari/TDM Safari Connector.app'
codesign --verify --deep --strict 'release/safari/TDM Safari Connector.app'
echo "已生成：$root_dir/release/safari/TDM Safari Connector.app"
