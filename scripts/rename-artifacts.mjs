#!/usr/bin/env node
/**
 * 打包后处理：把 Tauri 产物统一重命名并收集到 release/ 目录。
 *
 * 默认命名规则（纯版本号，不含产品名）:
 *   release/<version>_x64-setup.exe      安装包
 *   release/<version>_x64_portable.exe   便携版（免安装）
 *
 * 可选加前缀：
 *   node scripts/rename-artifacts.mjs --prefix=TDM-Downloader
 *   -> release/TDM-Downloader_0.2.0_x64-setup.exe
 *
 * 产物同时保留一份 stable 名称（latest_x64-setup.exe），方便脚本/链接固定引用。
 */
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);

function arg(name, def = "") {
  const hit = argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : def;
}

const conf = JSON.parse(
  fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8")
);
const version = conf.version;
const productName = conf.productName ?? "App";
const prefix = arg("prefix"); // 默认空 => 纯版本号命名
const arch = arg("arch", "x64");

const nsisDir = path.join(root, "src-tauri/target/release/bundle/nsis");
const portableSrc = path.join(root, "src-tauri/target/release/tdm-downloader.exe");
const outDir = path.join(root, "release");

if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });

/** 在目录里找第一个匹配后缀的 exe */
function findExe(dir, suffix) {
  if (!fs.existsSync(dir)) return null;
  return (
    fs
      .readdirSync(dir)
      .filter((f) => f.toLowerCase().endsWith(suffix))
      .map((f) => path.join(dir, f))
      .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)[0] ?? null
  );
}

// 安装包：优先精确匹配 "<productName>_<version>_<arch>-setup.exe"，找不到再退化为任意 *-setup.exe
const exact = path.join(nsisDir, `${productName}_${version}_${arch}-setup.exe`);
const setupSrc = fs.existsSync(exact) ? exact : findExe(nsisDir, "-setup.exe");

if (!setupSrc) {
  console.error(
    "[rename-artifacts] 未找到 NSIS 安装包，请先执行 `npm run tauri build`"
  );
  process.exit(1);
}

const base = prefix ? `${prefix}_${version}` : version;

function sha256(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function copyAs(src, targetName) {
  const dst = path.join(outDir, targetName);
  fs.copyFileSync(src, dst);
  const size = fs.statSync(dst).size;
  console.log(
    `  ${targetName.padEnd(34)} ${(size / 1024 / 1024).toFixed(2)} MiB  sha256=${sha256(
      dst
    ).slice(0, 16)}…`
  );
  return dst;
}

console.log(`[rename-artifacts] version=${version} arch=${arch} prefix="${prefix}"`);
console.log(`[rename-artifacts] output -> ${path.relative(root, outDir)}/`);

copyAs(setupSrc, `${base}_${arch}-setup.exe`);
copyAs(setupSrc, `latest_${arch}-setup.exe`);

if (fs.existsSync(portableSrc)) {
  copyAs(portableSrc, `${base}_${arch}_portable.exe`);
  copyAs(portableSrc, `latest_${arch}_portable.exe`);
} else {
  console.warn(`[rename-artifacts] 跳过便携版（未找到 ${portableSrc}）`);
}

console.log("[rename-artifacts] done");
