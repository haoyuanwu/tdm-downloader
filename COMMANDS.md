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

### 3.2 v0.2.0 功能验证工具（无 GUI 自检）

`examples/verify_v020.rs` 一次性把 v0.2.0 新增能力跑一遍并输出 PASS/FAIL：

```bash
cd src-tauri
cargo run --release --example verify_v020              # 全部（本地 + 真实链接，真链跑 90s）
cargo run --release --example verify_v020 -- --local   # 只跑本地确定性用例（约 8 秒）
cargo run --release --example verify_v020 -- --remote --seconds 60
```

**A 组（真实链接，默认 HuggingFace 6.77GB）**：链接预解析（文件名/大小/是否支持 Range）、16 线程下载、断点续传基线、二次启动校验偏移、分片是否全部推进。

**B 组（本地回显服务器，不依赖外网）**：自定义 UA/Referer/Cookie/自定义头是否真的发出、非法头名是否被忽略、三种代理模式、限速精度、续传不重复下载已完成的头部。

改 URL/保存路径就编辑文件顶部两个常量 `URL` / `SAVE`。

---

## 4. 构建与打包

### 4.1 一键打包（推荐）

```bash
npm run package
```

这一条等于 `tauri build` + 产物重命名，最终文件统一落在 `release/` 目录。

命名规则：**以版本号为文件名**（不带产品名）

| 文件 | 说明 |
|------|------|
| `release\0.2.0_x64-setup.exe` | Windows 安装包 |
| `release\0.2.0_x64_portable.exe` | 免安装绿色版 exe |
| `release\latest_x64-setup.exe` | 同上安装包的固定名副本（脚本/链接可长期引用） |
| `release\latest_x64_portable.exe` | 同上便携版的固定名副本 |

终端会顺带打印每个文件的 SHA256，可直接贴进 Release 说明。

想保留产品名前缀时用另一条：

```bash
npm run package:prefix     # -> release\TDM-Downloader_0.2.0_x64-setup.exe
```

脚本本身也可以单独跑（比如已经 build 过，只想重命名）：

```bash
node scripts/rename-artifacts.mjs                      # 纯版本号
node scripts/rename-artifacts.mjs --prefix=TDM-Downloader
node scripts/rename-artifacts.mjs --arch=arm64         # 其他架构时改后缀
```

### 4.1.1 不重命名（拿 Tauri 原始产物）

```bash
npm run tauri build
```

原始产物位置：

| 文件 | 说明 |
|------|------|
| `src-tauri\target\release\bundle\nsis\TDM Downloader_<版本>_x64-setup.exe` | Windows 安装包 |
| `src-tauri\target\release\tdm-downloader.exe` | 免安装绿色版 exe |

> 注：Tauri 2 没有提供安装包文件名的模板配置，名称固定由 `productName` + `version` 拼成，所以要改只能靠打包后重命名，这也是 `scripts/rename-artifacts.mjs` 存在的原因。

### 4.2 分步执行

```bash
npm run build                 # 1. 构建前端 → dist/
cd src-tauri
cargo build --release         # 2. 编译 release 版后端
npx tauri build               # 3. 打安装包（需在项目根目录跑）
```

### 4.3 校验安装包哈希

```bash
sha256sum release/0.2.0_x64-setup.exe release/0.2.0_x64_portable.exe
```

（`npm run package` 结束时已经打印过一遍，这里是想二次核对时用）

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

发布新版本前修改三处版本号保持一致：

- `package.json` → `"version": "0.2.0"`
- `src-tauri/tauri.conf.json` → `"version": "0.2.0"`
- `src-tauri/Cargo.toml` → `version = "0.2.0"`

改完再跑 `npm run package`，`release/` 下的文件名会自动跟着变（例如 `0.3.0_x64-setup.exe`）。

发布到 GitHub Release：

```bash
gh release create v0.2.0 \
  --title "TDM Downloader v0.2.0" \
  --notes-file notes.md \
  --latest \
  release/0.2.0_x64-setup.exe \
  release/0.2.0_x64_portable.exe
```

> 注意：附件名取自你传的文件名，所以文件名改成什么样，Release 里就显示什么样。

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
npm run package        # 打包 + 按版本号重命名到 release/
npm run tauri build    # 只打包，不做重命名
cargo clean            # 清缓存（在 src-tauri 下）
```
