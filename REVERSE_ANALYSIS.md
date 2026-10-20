# TDM Fast 逆向分析报告

> 分析工具：REA（Reverse Engineer Anything）+ 自写 PE 解析脚本
> 目标：`D:\App\TDM Fast\TDM Fast.exe`（版本 5.0.1，6,304,568 字节）
> 结论仅供技术学习参考，不用于复制原代码/破解授权。

## 1. 基本信息

| 项目 | 值 |
|------|-----|
| 文件名 | TDM Fast.exe |
| 版本 | 5.0.1 |
| 大小 | 6,304,568 字节（约 6MB） |
| SHA-256 | `1529190dcd48feabc97ce88856c645b08e9d3e45620adc686cdf9a7664e8edd8` |
| 格式 | PE32+ (x86_64)，6 个 section |
| 是否 .NET | 否（无 CLI 目录，纯 native） |
| 是否 Go | 否（无 Go build-info） |

## 2. 技术栈（关键结论）

**这是一个 Tauri 2 + Rust 编写的桌面应用**，不是纯 C++。

### 2.1 框架与运行时
- **框架**：Tauri 2.12.3
- **UI 内核**：Microsoft Edge WebView2（要求系统已装 WebView2 运行时）
- **异步运行时**：tokio 1.53.2
- **HTTP 客户端**：reqwest 0.12.28 + hyper 1.12.0

### 2.2 依赖库（从编译路径字符串提取）
- 加密：aes 0.8.4、rsa 0.9.10、cipher 0.4.4、block-buffer、rand_core、bcrypt
- 压缩：brotli-decompressor 6.0.1、fdeflate 0.3.7、png 0.18.1
- 编码：base64 0.22/0.23、idna 1.1.0、httparse、httpdate
- 序列化：serde、url、cookie 0.18.2
- 其它：aho-corasick 1.1.5（多模式匹配，可能用于链接/正则）、anyhow、bytes、crc32fast、icu_normalizer
- Tauri 插件：dialog 2.8.2、clipboard-manager 2.4.2、opener 2.7.1、muda 0.20.0（菜单）

### 2.3 构建者痕迹
- 路径泄露 `C:\Users\LINGDU\.cargo\registry\...`（开发者用户名 LINGDU）
- 前端渲染页面：`/renderer/progress.html`、`/renderer/settings.html`、`/renderer/newdownload.html` 及对应 `.js`

## 3. 功能清单（从字符串逆向还原）

### 3.1 下载引擎
- 多线程分段下载（tokio multi_thread scheduler）
- 断点续传：`.download` 临时文件后缀、分段合并 `[Merger]`
- 限速：`--limit-rate` 参数
- 任务状态机：`connecting` → `downloading` → `merging` → `queued` / `cancelled`
- 进度字段：`totaldownloaded`、`filepath`、`speed`

### 3.2 视频下载
- 抖音视频下载：`https://www.douyin.com/video/` 集成
- yt-dlp 调用：`ytdlp`、`--load-info-json`
- ffmpeg 合并：从 `https://ffmpeg.tdmfast.com/manifest.json` 或 `https://ffmpeg.actcb.com/manifest.json` 拉取 ffmpeg
- 媒体信息探测：`fps`、`filesize`、`tbr`、`format_note`、`hasVideo`、`hasAudio`

### 3.3 BT/磁力下载
- 磁力链接：`magnet:startPaused`

### 3.4 浏览器接管
- 自定义 URL 协议：`tdmfast:`（注册表 `Software\Classes\tdmfast\URL Protocol`）
- Cookie 共享：`# Netscape HTTP Cookie File`、`tdmfast-cookies`
- 浏览器扩展对接端点：`/capture`

### 3.5 系统集成
- 系统托盘：`tdm-tray`
- 任务栏进度条：`kernel:progress`（Windows 任务栏图标进度）
- 剪贴板监控：clipboard-manager 插件 + 自动识别下载链接

### 3.6 授权激活系统（不逆向、不破解）
- 激活服务器：`https://activate.tdmfast.com`、`https://activate1.lingdu80.com`
- 验证接口：`/api/verify.php`
- 硬件指纹：`machine_id`、`hw_id`
- **说明**：这部分涉及付费授权，不进行分析，自研软件也不需要此模块。

## 4. 对本项目（自研下载器）的启示

功能对标（合法部分，功能不受版权保护）：
1. ✅ 多线程分段下载 + 断点续传 + 限速
2. ✅ 任务队列与状态管理
3. ✅ 浏览器接管（自定义协议 + Cookie）
4. ✅ 剪贴板自动识别链接
5. ✅ 系统托盘 + 任务栏进度
6. ✅ （可选）视频下载（抖音/yt-dlp/ffmpeg）
7. ⬜ 磁力/BT（可选，复杂度高）
8. ❌ 不实现：授权激活系统（这是它的付费机制，与功能无关）

技术栈选择：Tauri 2 + Rust（与原版一致，性能最优），前端 WebView2。
