# 命令手册（COMMANDS）

TDM Downloader 项目的全部常用命令。所有命令均在项目根目录 `D:\CODE\tdm-downloader` 下执行。

---

## 0. 环境要求（一次性）

| 依赖 | 用途 | 安装 |
|------|------|------|
| Rust 1.99+（MSVC 工具链） | 编译后端 | `winget install Rustlang.Rustup`，然后 `rustup default stable` |
| Node.js 22+ | 前端构建 | https://nodejs.org |
| Visual Studio C++ 生成工具 | MSVC 链接器 | VS 安装器勾选「使用 C++ 的桌面开发」 |
| WebView2 Runtime | 应用运行 | Windows 10/11 已自带 |

验证环境：

```bash
rustc --version        # 期望 1.99+
node -v                # 期望 v22+
```

---

## 1. 首次拉取 / 换机后初始化

```bash
cd D:\CODE\tdm-downloader
npm install
```

如 esbuild 安装脚本被 npm 安全策略拦截（`allowScripts`），放行它：

```bash
npm install-scripts approve esbuild
npm rebuild esbuild
```

---

## 2. 运行（开发模式）

```bash
npm run tauri dev
```

- 自动启动 Vite（http://localhost:1420）+ 编译 Rust + 弹出应用窗口
- 首次编译约 20-25 分钟，之后增量编译只需几秒
- 改前端代码 → 页面热更新；改 Rust 代码 → 自动重编译重启

只起前端页面（不起桌面窗口，用于调试 UI）：

```bash
npm run dev
```

---

## 3. 测试（下载引擎）

```bash
cd src-tauri
cargo test
```

内置 3 个用例：8 线程分段下载逐字节比对、断点续传、纯函数校验。

### 3.1 真实大文件实测（CLI 工具）

无需启动 GUI，直接调用引擎下载真实文件，用于验证多线程、续传、代理、停滞重连：

```bash
cd src-tauri
cargo build --release --example download_cli

# 用法: download_cli <URL> <保存路径> [连接数]
./target/release/examples/download_cli.exe \
  "https://huggingface.co/unsloth/Qwen3.8-27B-GGUF/resolve/main/Qwen3.8-27B-UD-IQ2_XXS.gguf" \
  "D:\models\Qwen3.8-27B-UD-IQ2_XXS.gguf" 16
```

每 3 秒打印进度/速度，结束后校验文件大小。中断后重跑会自动从 `.partN` 续传。

**代理相关环境变量：**

```bash
# 强制使用环境变量代理（默认在 Windows 上优先读系统代理）
set TDM_USE_ENV_PROXY=1
```

---

## 4. 构建与打包

### 4.1 一键打包（推荐）—— 生成 NSIS 安装包

```bash
npm run tauri build
```

产物：

| 文件 | 说明 |
|------|------|
| `src-tauri\target\release\bundle\nsis\TDM Downloader_0.1.0_x64-setup.exe` | Windows 安装包 |
| `src-tauri\target\release\tdm-downloader.exe` | 免安装绿色版 exe |

### 4.2 分步执行

```bash
npm run build                 # 1. 构建前端 → dist/
cd src-tauri
cargo build --release         # 2. 编译 release 版后端
npx tauri build               # 3. 打安装包（需在项目根目录跑）
```

### 4.3 校验安装包哈希

```bash
sha256sum "src-tauri/target/release/bundle/nsis/TDM Downloader_0.1.0_x64-setup.exe"
```

---

## 5. 清理

```bash
# 清理 Rust 编译缓存（可释放数 GB）
cd src-tauri && cargo clean

# 清理前端依赖与构建产物（项目根目录）
rm -rf node_modules dist

# 重新初始化
npm install && npm run tauri dev
```

---

## 6. 版本号与发布

发布新版本前修改两处版本号保持一致：

- `package.json` → `"version": "0.1.0"`
- `src-tauri/tauri.conf.json` → `"version": "0.1.0"`
- `src-tauri/Cargo.toml` → `version = "0.1.0"`

改完再跑 `npm run tauri build`，安装包文件名会带上新版本号。

---

## 7. 常见问题

| 问题 | 解决 |
|------|------|
| `tauri build` 报 assetProtocol/feature 不匹配 | 检查 `tauri.conf.json` 的 security 配置与 Cargo.toml features 一致 |
| rustc 报 `missing manifest in toolchain` | `rustup toolchain install stable --force` 重装 |
| esbuild 装不上 / vite 起不来 | 见第 1 节放行 esbuild 脚本 |
| 打包卡在下载 NSIS | 网络问题，重跑 `npm run tauri build` 即可（有缓存断点续传） |
| 编译报 emit 找不到 | 确认 `src-tauri/src/engine.rs` 顶部有 `use tauri::Emitter;` |

---

## 8. 常用命令速查

```bash
npm run tauri dev      # 开发运行
cargo test             # 引擎测试（在 src-tauri 下）
npm run tauri build    # 打包安装包
cargo clean            # 清缓存（在 src-tauri 下）
```
