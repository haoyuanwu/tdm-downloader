#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
extension_id="${1:?用法：bash scripts/install-chrome-bridge.sh <Chrome扩展ID> [TDM应用路径]}"
[[ "$extension_id" =~ ^[a-p]{32}$ ]] || { echo "Chrome 扩展 ID 应为 32 位 a-p 字母" >&2; exit 1; }
app_path="${2:-/Applications/TDM Downloader.app}"
[[ -d "$app_path" ]] || { echo "找不到应用：$app_path" >&2; exit 1; }
/usr/libexec/PlistBuddy -c 'Print :CFBundleURLTypes' "$app_path/Contents/Info.plist" >/dev/null || { echo "此版本 TDM 尚不支持浏览器接管，请先运行新版" >&2; exit 1; }
bridge_dir="$HOME/Library/Application Support/TDM Browser Bridge"
manifest_dir="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
mkdir -p "$bridge_dir" "$manifest_dir"
xcrun swiftc "$root_dir/browser-extension/native/main.swift" -o "$bridge_dir/tdm-browser-bridge" -framework AppKit
codesign --force --sign - "$bridge_dir/tdm-browser-bridge"
printf '%s\n' "$app_path" > "$bridge_dir/app-path.txt"
python3 - "$extension_id" "$bridge_dir" "$manifest_dir" <<'PY'
import json, sys
from pathlib import Path
extension, bridge, destination = sys.argv[1:]
manifest = {'name':'com.tdm.downloader.bridge','description':'TDM browser download connector','path':str(Path(bridge)/'tdm-browser-bridge'),'type':'stdio','allowed_origins':['chrome-extension://'+extension+'/']}
(Path(destination)/'com.tdm.downloader.bridge.json').write_text(json.dumps(manifest,indent=2))
PY
echo "连接器已安装。Chrome 请重新加载扩展后测试下载。"
