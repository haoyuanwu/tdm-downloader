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
for command_name in node npm rustup lipo hdiutil shasum git gh; do
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

build_dmg() {
  local target="$1" label="$2" bundle_dir dmg_dir app_binary source_dmg output_dmg actual_arch
  bundle_dir="src-tauri/target/$target/release/bundle"
  dmg_dir="$bundle_dir/dmg"
  app_binary="src-tauri/target/$target/release/tdm-downloader"
  output_dmg="release/TDM-Downloader_${version}_macos_${label}.dmg"
  if [[ "$reuse_existing" == true && -f "$app_binary" && -f "$output_dmg" ]]; then
    [[ "$(lipo -archs "$app_binary")" == "$label" ]] || { echo "$label 架构不匹配" >&2; exit 1; }
    hdiutil verify "$output_dmg" >/dev/null
    echo "复用已生成的 $output_dmg"
    return
  fi
  rm -rf "$dmg_dir"
  echo "正在构建 $label ($target)……"
  npm run tauri -- build --target "$target" --bundles dmg --ci --no-sign

  # Tauri 生成 DMG 后会清理临时 .app，发布版可执行文件仍保留在 release/。
  [[ -f "$app_binary" ]] || { echo "找不到应用可执行文件：$app_binary" >&2; exit 1; }
  actual_arch="$(lipo -archs "$app_binary")"
  if [[ "$actual_arch" != "$label" ]]; then
    echo "架构不匹配：预期 $label，实际 $actual_arch" >&2
    exit 1
  fi

  source_dmg="$(find "$dmg_dir" -maxdepth 1 -type f -name "*.dmg" -print -quit)"
  [[ -n "$source_dmg" ]] || { echo "找不到 $label DMG" >&2; exit 1; }
  cp "$source_dmg" "$output_dmg"
  hdiutil verify "$output_dmg" >/dev/null
  echo "已生成：$output_dmg"
}

build_dmg aarch64-apple-darwin arm64
build_dmg x86_64-apple-darwin x86_64

arm_dmg="release/TDM-Downloader_${version}_macos_arm64.dmg"
x86_dmg="release/TDM-Downloader_${version}_macos_x86_64.dmg"
arm_sha="$(shasum -a 256 "$arm_dmg" | awk '{print $1}')"
x86_sha="$(shasum -a 256 "$x86_dmg" | awk '{print $1}')"
notes_file="$(mktemp)"
trap 'rm -f "$notes_file"' EXIT
cat > "$notes_file" <<EOF
## TDM Downloader $tag

$release_changes

| 文件 | 适用 Mac | SHA256 |
| --- | --- | --- |
| $(basename "$arm_dmg") | Apple 芯片 (arm64) | $arm_sha |
| $(basename "$x86_dmg") | Intel (x86_64) | $x86_sha |

安装包未进行 Apple 开发者签名和公证。首次打开时可能需要在系统设置的“隐私与安全性”中允许运行。
EOF

git -c credential.helper= -c credential.helper='!gh auth git-credential' push origin main
if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  git tag -a "$tag" -m "发布 TDM Downloader $version macOS 双架构版本"
fi
git -c credential.helper= -c credential.helper='!gh auth git-credential' push origin "$tag"

if gh release view "$tag" >/dev/null 2>&1; then
  gh release upload "$tag" "$arm_dmg" "$x86_dmg" --clobber
  gh release edit "$tag" --notes-file "$notes_file" --draft=false
else
  gh release create "$tag" "$arm_dmg" "$x86_dmg" --draft --title "$tag" --notes-file "$notes_file"
  gh release edit "$tag" --draft=false
fi

echo "发布完成：https://github.com/haoyuanwu/tdm-downloader/releases/tag/$tag"
