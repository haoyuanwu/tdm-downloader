#!/usr/bin/env bash
# 本地打包：仅构建 Apple 芯片 arm64 版本。
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

if [[ "$(uname -s)" != Darwin ]]; then echo "此脚本只能在 macOS 上运行" >&2; exit 1; fi
architecture="${1:-arm64}"
case "$architecture" in
  arm64) targets=(aarch64-apple-darwin); labels=(arm64) ;;
  *) echo "仅支持 Apple 芯片 arm64；用法：npm run package:macos [-- arm64]" >&2; exit 1 ;;
esac
[[ $# -le 1 ]] || { echo "参数过多" >&2; exit 1; }
for command_name in node npm rustup xcodebuild xcrun codesign hdiutil lipo ditto shasum; do
  command -v "$command_name" >/dev/null || { echo "缺少命令：$command_name，请先安装 Node.js、Rust 和完整 Xcode。" >&2; exit 1; }
done
version="$(node -p 'require("./package.json").version')"
[[ "$version" == "$(node -p 'require("./src-tauri/tauri.conf.json").version')" ]] || { echo "App 版本号不一致" >&2; exit 1; }
[[ -d node_modules ]] || npm ci
rustup target add "${targets[@]}"

echo "正在构建 TDM ${version}，架构：${labels[*]}"
npm run build
bash scripts/build-browser-extensions.sh
output_dir="$root_dir/release/packages"
mkdir -p "$output_dir"
checksums=()
for index in "${!targets[@]}"; do
  target="${targets[$index]}"
  label="${labels[$index]}"
  npm run tauri -- build --target "$target" --bundles app --ci \
    --config '{"build":{"beforeBuildCommand":""}}'
  app_dir="$root_dir/src-tauri/target/$target/release/bundle/macos/TDM Downloader.app"
  image="$output_dir/TDM-Downloader_${version}_macos_${label}.dmg"
  bash scripts/package-macos-dmg.sh "$app_dir" "$image"
  hdiutil verify "$image" >/dev/null
  bash scripts/verify-macos-dmg.sh "$image" "$label" "$version"
  checksums+=("$(shasum -a 256 "$image" | sed "s|$output_dir/||")")
done
printf '%s\n' "${checksums[@]}" > "$output_dir/SHA256SUMS.txt"
echo "打包完成：$output_dir"
echo "打开对应 DMG，将 TDM Downloader 拖入 Applications 并替换旧版。"
