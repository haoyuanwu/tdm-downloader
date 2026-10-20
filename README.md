# TDM Downloader

一个用 **Tauri 2 + Rust** 构建的 Windows 多线程下载管理器。

功能对标 TDM Fast（已通过 REA 逆向分析其实现），但**代码与 UI 完全自研**，
不复制原软件的任何代码、文案或图标。

## 特性

### 已实现（P0 核心）

| 功能 | 说明 |
|------|------|
| **多线程分段下载** | 按 HTTP Range 切分为 N 段并行下载，默认 8 连接 |
| **断点续传** | 暂停后从分片实际长度继续，不重新下载 |
| **实时进度** | 已下载字节 / 总大小 / 百分比 / 瞬时速度 / 剩余时间 |
| **任务管理** | 新建、暂停、恢复、取消、删除、清空已完成 |
| **限速** | 单任务限速（时间窗口配额算法） |
| **分片可视化** | 每个连接一条独立进度条 |
| **剪贴板识别** | 自动检测复制的 http(s) 链接，一键下载 |
| **设置持久化** | 下载目录、连接数、并发上限、全局限速、分段阈值 |

### 计划中（P1/P2）

- 系统托盘菜单、任务栏进度条
- 浏览器接管（`tdm:` 协议 + 浏览器扩展）
- 批量链接抓取
- 磁力/BT、视频站点下载

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
| `start_download` | 新建下载任务 |
| `list_tasks` | 列出所有任务 |
| `pause_task` / `resume_task` / `cancel_task` | 暂停 / 恢复 / 取消 |
| `remove_task` / `clear_completed` | 删除 / 清空已完成 |
| `get_settings` / `save_settings` | 读写设置 |
| `pick_directory` / `open_path` / `open_download_dir` | 文件系统交互 |

### 前端订阅的事件

- `download://progress` — 每 500ms 推送一次进度（速度、ETA、分片明细）
- `download://tasks` — 任务列表变化

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
- 尚未实现托盘菜单与浏览器协议接管（P1）
