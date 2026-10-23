#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"
build_dir="$root_dir/src-tauri/target/safari-extension"
xcodebuild -project 'browser-extension/safari-project/TDM Safari Connector/TDM Safari Connector.xcodeproj' -scheme 'TDM Safari Connector' -configuration Debug -derivedDataPath "$build_dir" ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO CODE_SIGN_IDENTITY=- CODE_SIGNING_ALLOWED=YES build
mkdir -p release/safari
ditto "$build_dir/Build/Products/Debug/TDM Safari Connector.app" 'release/safari/TDM Safari Connector.app'
# Prefer a real certificate for local Safari tests; the release script supplies
# an explicit distribution identity or '-' so personal development signing is not published.
safari_identity="${TDM_SAFARI_SIGNING_IDENTITY:-auto}"
if [[ "$safari_identity" == auto ]]; then
  safari_identity="$(security find-identity -v -p codesigning | sed -nE 's/^[[:space:]]*[0-9]+\) ([A-F0-9]{40}) "Developer ID Application:.*$/\1/p' | head -1)"
  if [[ -z "$safari_identity" ]]; then
    safari_identity="$(security find-identity -v -p codesigning | sed -nE 's/^[[:space:]]*[0-9]+\) ([A-F0-9]{40}) "Apple Development:.*$/\1/p' | head -1)"
  fi
  safari_identity="${safari_identity:--}"
fi
if [[ "$safari_identity" != "-" ]]; then
  codesign --force --sign "$safari_identity" --preserve-metadata=entitlements,flags \
    'release/safari/TDM Safari Connector.app/Contents/PlugIns/TDM Safari Connector Extension.appex'
  codesign --force --sign "$safari_identity" --preserve-metadata=entitlements,flags \
    'release/safari/TDM Safari Connector.app'
  echo "Safari 安装器已使用证书签名。"
else
  echo "Safari 安装器使用临时签名；部分 Safari 版本不会列出该扩展，本地测试请配置 Apple Development 证书。"
fi
codesign --verify --deep --strict 'release/safari/TDM Safari Connector.app'
echo "已生成：$root_dir/release/safari/TDM Safari Connector.app"
