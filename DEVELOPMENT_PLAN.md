# TDM Downloader 开发规划

功能对标 TDM Fast（已逆向确认其技术栈为 Tauri 2.12.3 + Rust + WebView2），
但**代码与 UI 完全自研**，不复制原软件任何代码、文案、图标。

## 一、技术栈

| 层 | 选型 | 理由 |
|----|------|------|
| 桌面框架 | Tauri 2 | 体积小(~10MB)、内存低、原生系统集成强 |
| 后端语言 | Rust (tokio + reqwest) | 异步 I/O 性能天花板，多线程分段下载 |
| 前端 | React 18 + TypeScript + Vite | 类型安全，构建快 |
| UI 内核 | WebView2 | Windows 原生，已装 |

## 二、功能模块规划

### P0 — 核心下载引擎（第一版必做）

1. **多线程分段下载**
   - HEAD 探测 `Content-Length` + `Accept-Ranges`
   - 支持断点续传时按 N 段切分（默认 8 连接）
   - 每段独立 Range 请求写入 `.partN` 分片文件
   - 全部完成后合并为最终文件
2. **断点续传 / 暂停恢复**
   - `.tdmstate.json` 记录任务元信息与各分片已下载字节
   - 暂停后从分片实际长度续传
3. **实时进度上报**
   - 通过 Tauri IPC 事件推送：已下载字节、总字节、瞬时速度、剩余时间
   - 前端进度条 + 速度曲线
4. **任务管理**
   - 新建 / 暂停 / 恢复 / 取消 / 删除 / 清空已完成
   - 任务状态机：Pending → Downloading → Paused → Merging → Completed / Failed / Cancelled
5. **限速**（全局 + 单任务）
   - 令牌桶限速器，`--limit-rate` 等价能力

### P1 — 系统集成（第二版）

6. 系统托盘 + 后台常驻
7. 任务栏进度条（Windows Taskbar Progress）
8. 浏览器接管（自定义 URL Protocol `tdm:` + 剪贴板监控）
9. 下载完成通知

### P2 — 扩展能力（后续）

10. 磁力/BT 下载
11. 视频站点下载（yt-dlp / ffmpeg 集成）
12. 站点抓取（批量链接识别）

## 三、目录结构

```
tdm-downloader/
├── REVERSE_ANALYSIS.md      # 逆向分析报告（已完成）
├── DEVELOPMENT_PLAN.md      # 本文档
├── src/                     # React 前端
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── TaskList.tsx
│   │   ├── NewDownloadDialog.tsx
│   │   └── SettingsPanel.tsx
│   └── types.ts
├── src-tauri/               # Rust 后端
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── engine.rs        # 下载引擎（分段/续传/限速/合并）
│       └── models.rs        # 数据结构
└── index.html / package.json / vite.config.ts
```

## 四、实施步骤

1. ✅ 环境：Rust 1.99.1 + MSVC + Node 22 + WebView2
2. 项目骨架：Cargo.toml / tauri.conf.json / capabilities / 图标
3. Rust 下载引擎（models.rs + engine.rs）
4. Tauri 命令层（lib.rs 注册 IPC 命令）
5. 前端 UI（React 组件 + 事件订阅）
6. 编译 → 运行 → 冒烟测试（真实下载验证）

## 五、验收标准

- `cargo build` 编译通过，无 error
- 前端 `npm run build` 通过
- 启动应用能添加下载任务
- 真实 URL 下载成功，多线程分段生效
- 暂停/恢复后能从断点续传（不重新下载）
