# TDM Downloader for macOS

这是原项目的 macOS 构建，沿用相同的 Rust 分段下载引擎和 React 界面。支持批量建任务、链接预解析、HTTP Range 多连接下载、暂停与续传、限速、任务管理、剪贴板链接检测、菜单栏图标和下载完成后的动作。

## 构建

需要 macOS 12+、完整 Xcode、Rust 1.99+ 和 Node.js 22+。

### 本地打包安装（推荐）

双击项目根目录的 **打包macOS.command** 即可打包当前 Mac 架构。也可以在项目目录执行：

```sh
npm run package:macos                 # 当前 Mac 架构
npm run package:macos -- arm64        # Apple 芯片
npm run package:macos -- x86_64       # Intel
npm run package:macos -- all          # 两种架构
```

首次需要完整 Xcode（用于构建 Safari 扩展）、Rust 与 Node.js；脚本会在缺少 Node 依赖时执行 npm ci，安装对应 Rust 目标，再构建界面、Safari 扩展与 Chrome 连接器。无需 GitHub 登录、提交记录或干净工作区。

输出在 **release/packages/**。打开对应 DMG，将应用拖入 Applications 并替换旧版；安装后先打开一次新版，软件会同步 Chrome 连接器目标。插件版本不变时不用重新加载插件。

此脚本不会自动安装或上传，也不会弹出 Finder 安装窗口。应用使用临时签名，尚未进行 Developer ID 签名与 Apple 公证。

开发时运行 `npm run tauri dev`；本地打包完成后可直接双击 DMG 安装，无需开发服务器。

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

## Safari 本地测试签名

本地打包自动优先选择 Developer ID Application 或 Apple Development 证书，仅用于 Safari 安装器与扩展。当前 Safari 27 的实测中，临时签名虽然可通过 codesign 完整性校验，Safari 仍会拒绝将其列入扩展设置；换用 Apple Development 签名后可识别。

可通过 `TDM_SAFARI_SIGNING_IDENTITY` 显式指定签名证书 SHA 或名称，设为 `-` 可选择临时签名。公开发行应使用 Developer ID Application 并完成公证；发布脚本不会自动将个人开发证书用于公开包。安装器更新保留较新版本与已签名副本，防止被临时签名资源覆盖。
