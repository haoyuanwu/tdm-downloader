# TDM Downloader 软件介绍

> 📘 **项目简介**：[查看项目简介](SOFTWARE_INTRO.md) — 功能、安装、上手、设置、FAQ
> 📦 **版本发布列表**：[GitHub Releases](https://github.com/haoyuanwu/tdm-downloader/releases) — 下载各平台安装包、查看更新说明及历史版本
> 📄 本文件是**技术向说明**：架构、IPC 接口、构建与测试

一个用 **Tauri 2 + Rust** 构建的多线程下载管理器。macOS 下载、构建和发布说明见 [MACOS.md](MACOS.md)。

Windows 适配说明与两端功能差异见 [WINDOWS.md](WINDOWS.md)。

下载：[macOS Apple 芯片版](https://github.com/haoyuanwu/tdm-downloader/releases/download/v0.2.7/TDM-Downloader_0.2.7_macos_arm64.dmg) · [macOS Intel 版](https://github.com/haoyuanwu/tdm-downloader/releases/download/v0.2.7/TDM-Downloader_0.2.7_macos_x86_64.dmg) · [Windows 安装包](https://github.com/haoyuanwu/tdm-downloader/releases/download/v0.2.2/0.2.2_x64-setup.exe)

## 浏览器扩展

Chrome 扩展与 macOS 连接器：[下载安装包](https://github.com/haoyuanwu/tdm-downloader/releases/download/v0.2.7/TDM-Browser-Extensions_0.2.7.zip)。Safari 扩展安装器随 macOS 应用提供。启用步骤见软件「设置 → 浏览器扩展」及 [扩展说明](browser-extension/README.md)。默认按 Shift + Command + D 打开助手弹窗。

## 特性

### 已实现（P0 核心）

| 功能 | 说明 |
|------|------|
| **多线程分段下载** | 按 HTTP Range 切分为 N 段并行下载，默认 8 连接 |
| **批量建任务** | 一次粘贴多个链接（每行一个或逗号分隔） |
| **链接预解析** | 建任务前探测文件名 / 大小 / 是否支持 Range |
| **断点续传** | 暂停后从分片实际长度继续，不重新下载 |
| **自定义请求头** | 单任务可设 User-Agent / Referer / Cookie |
| **代理模式** | 跟随系统 / 直连 / 自定义地址 |
| **实时进度** | 已下载字节 / 总大小 / 百分比 / 瞬时速度 / 剩余时间 / 速度曲线 |
| **任务管理** | 新建、暂停、恢复、失败重试、取消、删除、清空已完成、批量暂停/恢复、搜索 |
| **限速** | 全局限速 + 单任务限速（时间窗口配额算法） |
| **分片可视化** | 每个连接一条独立进度条 |
| **剪贴板识别** | 自动检测复制的 http(s) 链接，一键下载 |
| **系统托盘** | 菜单（显示 / 新建 / 暂停全部 / 恢复全部 / 退出）+ 关闭窗口最小化到托盘 |
| **任务栏进度条** | 合计进度同步到 Windows 任务栏图标 |
| **完成后动作** | 无 / 退出 / 关机 / 休眠 |
| **设置持久化** | 目录、连接数、并发上限、限速、分段阈值、代理、文件冲突策略、主题、自启 |

### 计划中（P1/P2）

- 磁力/BT 下载
- 视频站点下载（集成 yt-dlp / ffmpeg）

---

## 快速开始

### 环境要求

- Rust 1.99+（`rustup` 安装，MSVC 工具链）
- Node.js 22+
- WebView2 Runtime（Windows 10/11 通常已自带）
- Visual Studio C++ 生成工具（用于 MSVC 链接）

### 开发模式

```bash
cd tdm-downloader
npm install
npm run tauri dev
```

首次编译较慢（约 20-25 分钟，Tauri 依赖树很大），后续增量编译很快。

### 生产打包

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/nsis/`，为 NSIS 安装包。

### 仅编译后端

```bash
cd src-tauri
cargo build          # 产出 target/debug/tdm-downloader.exe
cargo test           # 运行下载引擎测试
```

---

## 项目结构

```
tdm-downloader/
├── src/                        # React 18 + TypeScript 前端
│   ├── main.tsx
│   ├── App.tsx                 # 主界面（任务列表 / 统计 / 弹窗）
│   ├── api.ts                  # Tauri IPC 封装 + 事件订阅
│   ├── types.ts                # 与 Rust models.rs 对应的类型
│   ├── styles.css
│   └── components/
│       ├── TaskCard.tsx        # 单个任务卡片（进度条 / 分片 / 操作）
│       ├── NewDownloadDialog.tsx
│       └── SettingsPanel.tsx
├── src-tauri/                  # Rust 后端
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── gen_icons.py            # 纯 stdlib 生成应用图标
│   └── src/
│       ├── main.rs
│       ├── lib.rs              # 12 个 IPC 命令
│       ├── models.rs           # 任务/状态/设置数据结构
│       └── engine.rs           # 下载引擎（分段/续传/限速/合并）+ 集成测试
├── REVERSE_ANALYSIS.md         # TDM Fast 逆向分析报告
└── DEVELOPMENT_PLAN.md         # 开发规划
```

---

## 后端 IPC 命令

| 命令 | 说明 |
|------|------|
| `start_download` | 新建下载任务（可带 UA / 自定义头 / start_paused） |
| `probe_url` | 预解析链接：文件名、大小、是否支持 Range |
| `list_tasks` | 列出所有任务 |
| `pause_task` / `resume_task` / `cancel_task` | 暂停 / 恢复（含失败重试）/ 取消 |
| `pause_all` / `resume_all` | 批量暂停 / 批量恢复 |
| `remove_task` / `clear_completed` | 删除 / 清空已完成 |
| `get_settings` / `save_settings` | 读写设置 |
| `pick_directory` / `open_path` / `open_download_dir` | 文件系统交互 |
| `set_progress_bar` | Windows 任务栏进度条（0.0–1.0，null 清除） |
| `show_main_window` / `hide_main_window` / `quit_app` | 窗口与退出控制（托盘菜单用） |
| `set_autostart` / `get_autostart` | 开机自启开关（写 Windows 启动目录） |
| `get_app_info` | 版本与可执行文件路径 |
| `copy_text` | 写入剪贴板 |

### 前端订阅的事件

- `download://progress` — 每 500ms 推送一次进度（速度、ETA、分片明细）
- `download://tasks` — 任务列表变化
- `tdm://new-task` — 托盘菜单点击「新建下载」

---

## 下载文件布局

下载过程中会在目标目录产生临时文件，完成后自动清理：

```
<文件名>.part0 ... .partN      # 各连接的下载分片
<文件名>.tdmstate.json         # 断点续传状态（URL、分片进度、总大小）
```

任务完成后分片与状态文件会被合并进最终文件并删除。

---

## 测试

```bash
cd src-tauri && cargo test
```

内置一个支持 Range 请求的极简 HTTP 服务器，真实跑通：

1. `test_segmented_download_and_merge` — 8 线程分段下载 → 合并，逐字节比对源文件
2. `test_resume_from_partial` — 模拟中断（分片 0 完成 40%、分片 1 完成 10%）后续传，校验完整性
3. `test_helpers` — 分片切分连续性、文件名推断、非法字符清理

---

## 已知限制

- 分段下载要求服务端支持 `Accept-Ranges: bytes`；不支持时自动退化为单连接
- 文件大小未知时（无 Content-Length）使用单连接流式下载
- 尚未实现浏览器协议接管（P1）；托盘菜单已实现
