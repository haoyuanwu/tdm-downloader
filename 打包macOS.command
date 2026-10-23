#!/usr/bin/env bash
cd "$(dirname "$0")" || exit 1
result=0
bash scripts/build-macos.sh "$@" || result=$?
if [[ "$result" -ne 0 ]]; then echo "打包失败，请查看上面的错误信息。"; fi
if [[ -t 0 ]]; then read -r -p "按回车关闭此窗口…" || true; fi
exit "$result"
