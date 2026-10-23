#!/usr/bin/env bash
# 保留开发版的 macOS 应用身份和系统通知权限，继续使用 Tauri/Vite 热重载。
# 仅创建 target/ 中的开发容器，无需生成 DMG 或安装到“应用程序”。
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir/src-tauri"
command_name="${1:-}"
shift || true
if [[ "$command_name" != run ]]; then
  exec cargo "$command_name" "$@"
fi

build_args=()
target_dir="${CARGO_TARGET_DIR:-$root_dir/src-tauri/target}"
profile=debug
target=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --) shift; break ;;
    --release) profile=release ;;
    --target) target="${2:?missing target}" ;;
    --target=*) target="${1#--target=}" ;;
  esac
  build_args+=("$1")
  shift
done
cargo build ${build_args[@]+"${build_args[@]}"}
if [[ -n "$target" ]]; then target_dir="$target_dir/$target"; fi
binary_dir="$target_dir/$profile"
dev_app="$binary_dir/TDM Downloader Dev.app"
mkdir -p "$dev_app/Contents/MacOS" "$dev_app/Contents/Resources"
cp "$binary_dir/tdm-downloader" "$dev_app/Contents/MacOS/tdm-downloader"
cp icons/icon.icns "$dev_app/Contents/Resources/icon.icns"
version="$(node -p 'require("../package.json").version')"
cat > "$dev_app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.tdm.downloader.dev</string>
<key>CFBundleName</key><string>TDM Downloader Dev</string>
<key>CFBundleDisplayName</key><string>TDM Downloader Dev</string>
<key>CFBundleExecutable</key><string>tdm-downloader</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLSchemes</key><array><string>tdm</string></array></dict></array>
</dict></plist>
EOF
/usr/bin/codesign --force --sign - --identifier com.tdm.downloader.dev "$dev_app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$dev_app"
echo "运行开发版：$dev_app"
exec "$dev_app/Contents/MacOS/tdm-downloader" "$@"
