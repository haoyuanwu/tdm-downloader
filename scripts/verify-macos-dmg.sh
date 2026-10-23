#!/usr/bin/env bash
# 校验用户实际安装的 DMG 内容，避免只校验构建目录中的可执行文件。
set -euo pipefail

image="${1:?用法：verify-macos-dmg.sh <DMG> <arm64|x86_64> <版本号>}"
expected_arch="${2:?缺少预期架构}"
expected_version="${3:?缺少预期版本号}"
mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/tdm-dmg-check.XXXXXX")"
mounted=false
cleanup() {
  if [[ "$mounted" == true ]]; then hdiutil detach "$mount_dir" >/dev/null; fi
  rmdir "$mount_dir"
}
trap cleanup EXIT

hdiutil attach -readonly -nobrowse -noautoopen -mountpoint "$mount_dir" "$image" >/dev/null
mounted=true
app_dir="$mount_dir/TDM Downloader.app"
plist="$app_dir/Contents/Info.plist"
[[ -f "$plist" ]] || { echo "DMG 内缺少 TDM Downloader.app" >&2; exit 1; }
executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"
version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")"
actual_arch="$(lipo -archs "$app_dir/Contents/MacOS/$executable")"
[[ "$actual_arch" == "$expected_arch" ]] || {
  echo "DMG 内程序架构错误：预期 ${expected_arch}，实际 $actual_arch" >&2
  exit 1
}
[[ "$version" == "$expected_version" ]] || {
  echo "DMG 内程序版本错误：预期 ${expected_version}，实际 $version" >&2
  exit 1
}
codesign --verify --deep --strict --verbose=2 "$app_dir"
safari_app="$app_dir/Contents/Resources/safari/TDM Safari Connector.app"
[[ -f "$safari_app/Contents/MacOS/TDM Safari Connector" ]] || { echo "DMG 缺少 Safari 扩展安装器" >&2; exit 1; }
lipo -verify_arch arm64 x86_64 "$safari_app/Contents/MacOS/TDM Safari Connector"
lipo -verify_arch arm64 x86_64 "$safari_app/Contents/PlugIns/TDM Safari Connector Extension.appex/Contents/MacOS/TDM Safari Connector Extension"
codesign --verify --deep --strict "$safari_app"
echo "DMG 内容校验通过：版本 ${version}，架构 ${actual_arch}，完整应用签名有效"
