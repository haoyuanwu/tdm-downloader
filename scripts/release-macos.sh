#!/usr/bin/env bash
# 在 macOS 上构建 arm64 / x86_64 DMG，并发布到 GitHub Releases。
# 使用前先以中文提交所有改动；脚本要求工作区干净，避免发布未提交的源码。
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "此脚本只能在 macOS 上运行" >&2
  exit 1
fi
for command_name in node npm rustup lipo hdiutil shasum git gh codesign ditto xcodebuild; do
  command -v "$command_name" >/dev/null || { echo "缺少命令：$command_name" >&2; exit 1; }
done
gh auth status >/dev/null

reuse_existing=false
if [[ "${1:-}" == "--reuse-existing" ]]; then
  reuse_existing=true
elif [[ $# -gt 0 ]]; then
  echo "用法：npm run release:macos [-- --reuse-existing]" >&2
  exit 1
fi

if [[ "$(git branch --show-current)" != main ]]; then
  echo "请在 main 分支发布" >&2
  exit 1
fi
if ! git diff --quiet || ! git diff --cached --quiet || [[ -n "$(git ls-files --others --exclude-standard)" ]]; then
  echo "工作区有未提交的改动，请先提交" >&2
  exit 1
fi

version="$(node -p 'require("./package.json").version')"
tauri_version="$(node -p 'require("./src-tauri/tauri.conf.json").version')"
if [[ "$version" != "$tauri_version" ]]; then
  echo "package.json 与 tauri.conf.json 的版本号不同" >&2
  exit 1
fi
tag="v$version"
release_changes="$(node --input-type=module - "$tag" <<'NODE'
import { readFileSync } from "node:fs";
const tag = process.argv[2];
const lines = readFileSync("CHANGELOG.md", "utf8").split(/\r?\n/);
const start = lines.findIndex(line => line === `## ${tag}` || line.startsWith(`## ${tag} `));
if (start < 0) throw new Error(`CHANGELOG.md 中缺少 ${tag} 的更新说明`);
const next = lines.findIndex((line, index) => index > start && line.startsWith("## "));
const changes = lines.slice(start + 1, next < 0 ? undefined : next).join("\n").trim();
if (!changes) throw new Error(`${tag} 的更新说明为空`);
process.stdout.write(changes);
NODE
)"
commit="$(git rev-parse HEAD)"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  if [[ "$(git rev-parse "$tag^{commit}")" != "$commit" ]]; then
    echo "标签 $tag 指向其他提交，请先更新版本号" >&2
    exit 1
  fi
fi
if gh release view "$tag" --json isDraft --jq '.isDraft' 2>/dev/null | grep -qx false; then
  echo "GitHub 上已发布 $tag，请先更新版本号" >&2
  exit 1
fi

rustup target add aarch64-apple-darwin x86_64-apple-darwin
mkdir -p release

build_prepared=false
prepare_build() {
  if [[ "$build_prepared" == false ]]; then
    echo "构建共享的前端与 Safari 扩展（仅执行一次）……"
    npm run build
    TDM_SAFARI_SIGNING_IDENTITY="${TDM_SAFARI_SIGNING_IDENTITY:--}" bash scripts/build-browser-extensions.sh
    build_prepared=true
  fi
}

build_dmg() {
  local target="$1" label="$2" bundle_dir app_dir output_dmg
  bundle_dir="src-tauri/target/$target/release/bundle"
  app_dir="$bundle_dir/macos/TDM Downloader.app"
  output_dmg="release/TDM-Downloader_${version}_macos_${label}.dmg"
  if [[ "$reuse_existing" == true && -f "$output_dmg" ]]; then
    hdiutil verify "$output_dmg" >/dev/null
    bash scripts/verify-macos-dmg.sh "$output_dmg" "$label" "$version"
    echo "复用已生成的 $output_dmg"
    return
  fi
  prepare_build
  echo "正在构建 $label ($target)……"
  # 只生成已签名的 .app；跳过 Tauri 会操作 Finder 的 DMG 美化流程。
  # 前端与 Safari 资源已经准备好，两个架构不再重复执行 beforeBuildCommand。
  npm run tauri -- build --target "$target" --bundles app --ci \
    --config '{"build":{"beforeBuildCommand":""}}'
  bash scripts/package-macos-dmg.sh "$app_dir" "$output_dmg"
  hdiutil verify "$output_dmg" >/dev/null
  bash scripts/verify-macos-dmg.sh "$output_dmg" "$label" "$version"
  echo "已生成：$output_dmg"
}

build_dmg aarch64-apple-darwin arm64
build_dmg x86_64-apple-darwin x86_64

arm_dmg="release/TDM-Downloader_${version}_macos_arm64.dmg"
x86_dmg="release/TDM-Downloader_${version}_macos_x86_64.dmg"
extension_zip="release/TDM-Browser-Extensions_${version}.zip"
extension_stage="$(mktemp -d)"
mkdir -p "$extension_stage/browser-extension" "$extension_stage/scripts"
ditto browser-extension/chrome "$extension_stage/browser-extension/chrome"
ditto browser-extension/native "$extension_stage/browser-extension/native"
cp browser-extension/README.md "$extension_stage/README.md"
cp scripts/install-chrome-bridge.sh "$extension_stage/scripts/"
ditto -c -k --norsrc --noextattr "$extension_stage" "$extension_zip"
rm -rf "$extension_stage"
arm_sha="$(shasum -a 256 "$arm_dmg" | awk '{print $1}')"
x86_sha="$(shasum -a 256 "$x86_dmg" | awk '{print $1}')"
extension_sha="$(shasum -a 256 "$extension_zip" | awk '{print $1}')"
notes_file="$(mktemp)"
trap 'rm -f "$notes_file"' EXIT
cat > "$notes_file" <<EOF
## TDM Downloader $tag

$release_changes

| 文件 | 适用 Mac | SHA256 |
| --- | --- | --- |
| $(basename "$arm_dmg") | Apple 芯片 (arm64) | $arm_sha |
| $(basename "$x86_dmg") | Intel (x86_64) | $x86_sha |
| $(basename "$extension_zip") | Chrome 扩展与 macOS 本地连接器 | $extension_sha |

Safari 扩展安装器随 DMG 内的应用提供，入口：设置 → 浏览器扩展。Chrome 请解压扩展压缩包，按应用教程加载 browser-extension/chrome，并使用 scripts/install-chrome-bridge.sh 安装连接器（需要 Xcode Command Line Tools 和 Python 3）。浏览器内默认按 Shift + Command + D 打开助手弹窗；临时签名 Safari 扩展需手动启用，部分版本需允许未签名扩展。

应用采用完整应用包临时签名（ad-hoc），已验证包内签名完整性；未进行 Developer ID 发行签名和 Apple 公证。首次打开时仍可能需要在系统设置的“隐私与安全性”中允许运行。
EOF

git -c credential.helper= -c credential.helper='!gh auth git-credential' push origin main
if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  git tag -a "$tag" -m "发布 TDM Downloader $version macOS 双架构版本"
fi
git -c credential.helper= -c credential.helper='!gh auth git-credential' push origin "$tag"

if gh release view "$tag" >/dev/null 2>&1; then
  gh release upload "$tag" "$arm_dmg" "$x86_dmg" "$extension_zip" --clobber
  gh release edit "$tag" --notes-file "$notes_file" --draft=false
else
  gh release create "$tag" "$arm_dmg" "$x86_dmg" "$extension_zip" --draft --title "$tag" --notes-file "$notes_file"
  gh release edit "$tag" --draft=false
fi

echo "发布完成：https://github.com/haoyuanwu/tdm-downloader/releases/tag/$tag"
