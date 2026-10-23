#!/usr/bin/env bash
# 直接创建磁盘映像，不挂载可写卷、不调用 Finder/AppleScript。
set -euo pipefail

app_dir="${1:?用法：package-macos-dmg.sh <应用.app> <输出.dmg>}"
output_dmg="${2:?缺少输出 DMG 路径}"
[[ -f "$app_dir/Contents/Info.plist" ]] || { echo "找不到应用：$app_dir" >&2; exit 1; }
[[ "$output_dmg" == *.dmg ]] || { echo "输出文件必须为 .dmg" >&2; exit 1; }
mkdir -p "$(dirname "$output_dmg")"
output_dmg="$(cd "$(dirname "$output_dmg")" && pwd)/$(basename "$output_dmg")"
stage_dir="$(mktemp -d "${TMPDIR:-/tmp}/tdm-dmg-stage.XXXXXX")"
temporary_dmg="${output_dmg%.dmg}.building.dmg"
cleanup() {
  rm -rf "$stage_dir"
  rm -f "$temporary_dmg"
}
trap cleanup EXIT

ditto --norsrc --noextattr "$app_dir" "$stage_dir/TDM Downloader.app"
ln -s /Applications "$stage_dir/Applications"
hdiutil create -quiet -ov -volname "TDM Downloader" -srcfolder "$stage_dir" \
  -fs HFS+ -format UDZO "$temporary_dmg"
mv -f "$temporary_dmg" "$output_dmg"
echo "已在后台生成：$output_dmg"
