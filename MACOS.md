# TDM Downloader for macOS

这是原项目的 macOS 构建，沿用相同的 Rust 分段下载引擎和 React 界面。支持批量建任务、链接预解析、HTTP Range 多连接下载、暂停与续传、限速、任务管理、剪贴板链接检测、菜单栏图标和下载完成后的动作。

## 构建

需要 macOS 12+、Xcode Command Line Tools、Rust 1.99+ 和 Node.js 22+。

```sh
npm ci
npm run tauri build
```

生成的应用在 `src-tauri/target/release/bundle/macos/TDM Downloader.app`，磁盘映像在 `src-tauri/target/release/bundle/dmg/`。开发时运行 `npm run tauri dev`。

## 发布到 GitHub

先同步更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json` 的版本号，以中文提交并保持 `main` 工作区干净。然后在已登录 `gh` 的 macOS 上运行：

```sh
npm run release:macos
```

脚本会安装所需 Rust 目标，分别构建 arm64 和 x86_64 的未签名 DMG，核对应用可执行文件架构与磁盘映像完整性，计算 SHA256，推送 `main` 和版本标签，并上传到 GitHub Releases。文件也保存在 `release/`。同一版本已正式发布时，脚本会要求先更新版本号。

## macOS 集成

- 默认下载到 `~/Downloads`，设置保存在 `~/Library/Application Support/tdm-downloader/settings.json`。
- “在 Finder 中打开”使用系统 `open` 命令。
- “开机自动启动”写入当前用户的 `~/Library/LaunchAgents/com.tdm.downloader.plist`，在下次登录时生效。移动应用后需要重新打开设置并切换一次该选项。
- “跟随系统代理”读取当前 macOS 网络配置的 HTTP/HTTPS 手动代理。PAC 自动配置无法由下载引擎直接解析；使用 PAC 时，可在设置中填写代理服务器地址。
- 关闭窗口可继续在菜单栏运行；使用菜单栏的“退出”才会完全结束。
- 菜单栏仅显示图标。左键点击图标会打开快速下载小窗，输入链接后点击“下载”；剪贴板有下载链接时，占位提示会显示链接，按 Tab 可填入。后台检测到新的可下载文件链接时，同一个小窗显示文件信息及“下载 / 暂不下载”操作。右键点击图标可打开任务管理菜单。主窗口隐藏时 Dock 图标随之隐藏。
- “关机”在全部任务完成后等待 30 秒，再通过系统事件请求关机。macOS 可能要求授权自动化权限；“休眠”使用 `pmset sleepnow`。

本地生成的 `.app` 未签名和公证。给其他 Mac 分发前需要使用自己的 Apple Developer 证书签名、公证。
