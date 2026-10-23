# TDM 浏览器下载助手

## 支持范围

- Chrome：监听新下载，暂停原任务并唤起 TDM。TDM 成功创建任务后取消浏览器原任务；取消确认、连接失败或 120 秒超时后尝试恢复原任务。极小文件可能在暂停前已经完成，保留浏览器结果。
- Safari：点击带 download 属性或常见文件后缀的链接时唤起 TDM，其他链接可右键选择“使用 TDM 下载此链接”。Safari 不提供 Chrome 的完整下载接管接口；脚本按钮、POST 下载、Content-Disposition 动态下载不保证自动识别。
- 仅普通 HTTP/HTTPS GET 文件；不读取浏览器 Cookie，不接管 blob/data URL、DRM、需要特殊请求头或登录态的资源。
- 应用中仍需点击“下载”，不允许网页通过协议直接创建下载或访问本地文件。

## App 内一键配置与独立版本

新版入口：设置 → 浏览器扩展 → Google Chrome。点击「一键准备插件」后打开固定文件夹，在 Chrome 中加载一次，再填写扩展 ID 并点击「连接 Chrome」。App 内配置使用随包附带的通用架构连接器，不需要 Xcode 或 Python。下方终端脚本保留给手动配置场景。

Chrome 文件固定保存在 `~/Library/Application Support/TDM Browser Extensions/chrome`，连接器及 ID 注册文件使用固定路径。App 覆盖更新会同步连接目标，插件版本相同则不改写文件；更新文件后仍需在 Chrome 中重新加载。首次从旧文件夹迁移时需要使用新文件夹显示的扩展 ID。

Chrome 的 `manifest.json`、Safari 的 `manifest.json`、连接器的 `native/version.json` 分别维护版本，与 App 版本无关。App 内按钮检查当前安装包附带的插件版本，不从网络下载未知代码；不会降级已准备的新版插件。Safari 安装器仍通过应用内按钮打开，首次启用和网站授权由 Safari 完成。

## Chrome 安装

1. 先运行当前开发版 `npm run tauri dev`，或安装之后包含本功能的正式版（v0.2.6 及以前尚不支持）。
2. 打开 `chrome://extensions`，开启开发者模式，点击“加载已解压的扩展程序”，选择本目录的 `chrome` 文件夹。
3. 复制显示的 32 位扩展 ID，在项目根目录执行：

   ```sh
   bash scripts/install-chrome-bridge.sh 扩展ID "/完整路径/TDM Downloader.app"
   ```

   开发版应用路径为 `src-tauri/target/debug/TDM Downloader Dev.app` 的绝对路径，仅适合保持 `npm run tauri dev` 运行时联调；关闭开发服务器后，连接器会返回错误并保留 Chrome 下载，不再打开空白开发窗口。日常使用请配置到正式版应用路径。连接器只允许该扩展 ID 调用。脚本需要 Xcode Command Line Tools，编译后运行不依赖 Python。
4. 重新加载扩展，在网页点击文件下载。扩展按钮可关闭接管、查看最近状态。TDM 弹窗取消或超时后，Chrome 恢复原下载；若服务器不允许续传，需在 Chrome 下载列表重试。

连接器位于 `~/Library/Application Support/TDM Browser Bridge`；注册信息位于 `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.tdm.downloader.bridge.json`。移动应用后重新执行安装脚本。

## Safari 安装与构建

原生容器项目：`safari-project/TDM Safari Connector/TDM Safari Connector.xcodeproj`。它是独立的 Safari 扩展安装容器，TDM 负责下载。

```sh
bash scripts/build-safari-extension.sh
```

输出在 `release/safari/TDM Safari Connector.app`，包含 arm64 和 x86_64 两种架构。macOS 打包时会自动执行此脚本并把容器放入 TDM 应用资源中；开发版使用上述本地输出。

应用内入口：**设置 → 浏览器扩展 → 打开扩展安装器**，该页提供启用、网站授权、临时签名与故障排查教程。

本地测试打开容器后在 Safari 设置的“扩展”中启用 TDM 下载助手，并授权所需网站。临时签名构建可能需要 Safari 开发菜单“允许未签名的扩展”；需由用户自行启用。正式分发需要 Apple Developer 发行签名及公证。没有自动改变浏览器的安全设置。

开发版使用独立的 `tdm-dev://` 协议，正式版和 Safari 使用 `tdm://`，避免开发容器抢占正式版浏览器唤起。

首次打开容器点击“打开 Safari 扩展设置”。需先安装或运行支持 `tdm://download` 的 TDM 版本。若同时安装旧版和开发版，Safari 可能使用系统注册的旧应用，测试前应先更新正式应用或只保留新版注册。

## 验证

```sh
node --test browser-extension/chrome/background.test.cjs
cd src-tauri && cargo test --lib browser_bridge
```

覆盖接管成功后取消原任务、失败/拒绝/超时恢复、关闭接管、忽略 blob 和协议输入校验。浏览器授权与实际下载仍需在 Chrome/Safari 中人工联调。

## 打开插件与快捷键

- Safari：先在 Safari 设置 → 扩展中启用「TDM 下载助手」并允许网站访问。在浏览器内按 **Shift + Command + D（⇧⌘D）**，或点击工具栏 TDM 图标打开助手弹窗，打开「自动接管下载」开关。
- Chrome：先启用扩展并安装本地连接器，按 **Shift + Command + D** 打开助手（Windows/Linux 为 Ctrl + Shift + D），或从工具栏拼图菜单打开并固定「TDM 下载助手」。
- 快捷键只打开助手弹窗，不会绕过浏览器首次启用步骤。Chrome 旧版快捷键或冲突可在 `chrome://extensions/shortcuts` 中调整「激活此扩展程序」。
- 扩展 0.1.1 修复 Safari 右键菜单中文编码并重建缓存菜单；更新后重新载入扩展或重启 Safari，再刷新网页。
