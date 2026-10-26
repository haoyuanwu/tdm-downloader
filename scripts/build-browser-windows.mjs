import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

if (process.platform !== "win32") throw new Error("此脚本仅用于 Windows 打包");
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const bridgeVersion = JSON.parse(readFileSync(path.join(root, "browser-extension/native/windows-version.json"), "utf8")).version;
const cargoVersion = readFileSync(path.join(root, "browser-extension/native-windows/Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (cargoVersion !== bridgeVersion) throw new Error("Windows 连接器的 Cargo 版本与版本文件不一致");
const result = spawnSync("cargo", ["build", "--manifest-path", "browser-extension/native-windows/Cargo.toml", "--target-dir", "src-tauri/target", "--release"], { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const output = path.join(root, "release/browser-native-windows");
mkdirSync(output, { recursive: true });
copyFileSync(path.join(root, "src-tauri/target/release/tdm-browser-bridge.exe"), path.join(output, "tdm-browser-bridge.exe"));
