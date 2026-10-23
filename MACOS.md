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

先同步更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json` 的版本号，在 `CHANGELOG.md` 中填写该版本的中文更新说明，以中文提交并保持 `main` 工作区干净。然后在已登录 `gh` 的 macOS 上运行：

```sh
npm run release:macos
```

脚本一次性构建共享的前端、Safari 扩展与通用架构 Chrome 连接器，再分别构建 arm64 和 x86_64 的已签名应用，通过 `hdiutil create` 在后台生成 DMG。整个发布流程不调用 Finder / AppleScript，不打开安装窗口、不运行安装器。DMG 中保留应用与「Applications」目录快捷方式，可正常拖入安装。

每个 DMG 只执行一次必要校验：映像完整性、只读隐藏挂载后的实际版本与架构、完整应用签名和内置 Safari 扩展。随后计算 SHA256，从更新日志提取中文说明，推送 `main` 与标签，并上传两个 DMG 和 Chrome 扩展压缩包。文件保存在 `release/`，同一版本已正式发布时需更新版本号。

如果构建已完成但上传中断，确认源码未变后可运行 `npm run release:macos -- --reuse-existing`，复用并再次校验已生成的 DMG。

## Apple 芯片 Mac 提示“需要更新 App”

若提示未来 macOS 版本不再支持此应用，请确认启动的不是此前安装的 Intel 版。退出旧应用后，从发布页下载文件名带 `macos_arm64.dmg` 的版本，拖入“应用程序”并选择替换；仅打开 DMG 不会更新已经安装的应用。随后从“应用程序”重新启动。可用下面的命令核对安装结果，Apple 芯片版应输出 `arm64`：

```sh
lipo -archs "/Applications/TDM Downloader.app/Contents/MacOS/tdm-downloader"
```

## macOS 集成

- 默认下载到 `~/Downloads`，设置保存在 `~/Library/Application Support/tdm-downloader/settings.json`。
- “在 Finder 中打开”使用系统 `open` 命令。
- “设置 → 启动与通知 → 开机自动启动”写入当前用户的 `~/Library/LaunchAgents/com.tdm.downloader.plist`，在下次登录时生效；可选择登录后仅在菜单栏后台运行，手动打开应用仍会显示主窗口。保存失败会显示错误，开关会读取真实启动项状态。移动应用后需要重新打开设置并切换一次该选项。
- “跟随系统代理”读取当前 macOS 网络配置的 HTTP/HTTPS 手动代理。PAC 自动配置无法由下载引擎直接解析；使用 PAC 时，可在设置中填写代理服务器地址。
- 关闭窗口可继续在菜单栏运行；使用菜单栏的“退出”才会完全结束。
- 菜单栏仅显示图标。开始下载后显示进度圆环，中央数字表示下载、连接、排队和合并中的任务数，每 3 秒切换一个任务的进度；大小未知的下载显示旋转圆弧。点击图标可打开显示名称、进度和速度的下载列表，列表中可新建下载或打开主窗口。所有进行中任务结束后恢复默认图标。
- 没有任务时，点击图标打开快速下载输入框；剪贴板有下载链接时，按 Tab 可填入。后台检测到新的可下载文件链接时，同一个小窗显示文件信息及“下载 / 暂不下载”操作。右键点击图标可打开任务管理菜单。主窗口隐藏时 Dock 图标随之隐藏。
- 下载并合并完成后发送系统本地通知，并独立播放 Glass 提示音；失败、取消或跳过已有文件不发送完成提醒。应用使用现代 macOS 通知接口并请求真实授权，前台也显示横幅并保留通知中心记录。可在“设置 → 系统”查看权限、测试通知和声音、打开系统通知设置。
- 点击通知会恢复 Dock 图标、显示并聚焦主窗口，切换到下载列表。
- 多显示器下，菜单栏弹窗按本次点击的屏幕和图标位置显示，兼容不同缩放比例；在另一屏点击会移动弹窗，并避免越出屏幕边缘。
- “设置 → 外观”支持跟随系统、深色和浅色主题；外观位于设置首项，右上角按钮和 Esc 均可关闭设置。
- 移除单个任务或清空已完成任务时，可选择保留本地文件或将文件移到废纸篓；勾选“下次不再询问”后保存本次处理方式。可在“设置 → 下载”重新开启询问或更改处理方式。保留文件也会保留未完成分片，方便以后续传。
- `npm run tauri dev` 会在 `target/debug/` 中维护一个开发启动容器，让开发版以 “TDM Downloader Dev” 身份获得通知权限，继续支持 Vite 热重载和 Rust 自动重启。无需生成 DMG 或安装应用；正式版与开发版的通知权限分别管理。
- “关机”在全部任务完成后等待 30 秒，再通过系统事件请求关机。macOS 可能要求授权自动化权限；“休眠”使用 `pmset sleepnow`。

默认生成的 `.app` 使用临时签名，未进行 Developer ID 发行签名和 Apple 公证，首次打开可能需要在系统设置的“隐私与安全性”中允许运行。临时签名修复完整应用包的签名一致性，不等同于通过 Apple 的发行信任检查。要通过默认 Gatekeeper 检查，需要 Developer ID Application 证书及公证凭据；可按 [Tauri macOS 签名说明](https://v2.tauri.app/distribute/sign/macos/) 设置 `APPLE_SIGNING_IDENTITY` 与 Apple 公证环境变量后发布。
