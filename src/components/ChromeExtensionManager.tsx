import { useEffect, useState } from "react";
import { api } from "../api";
import type { BrowserExtensionStatus } from "../api";
import { isWindows } from "../platform";

export function ChromeExtensionManager() {
  const [status, setStatus] = useState<BrowserExtensionStatus | null>(null);
  const [extensionId, setExtensionId] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void api.browserExtensionStatus().then((result) => {
      if (active) { setStatus(result); setExtensionId(result.chrome_extension_id || ""); }
    }).catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, []);

  const run = async (connect: boolean) => {
    setBusy(true); setMessage(""); setError("");
    try {
      const next = connect ? await api.configureChromeExtension(extensionId.trim()) : await api.prepareChromeExtension();
      setStatus(next);
      setMessage(connect ? "连接器已配置，可以测试下载；若刚加载了新文件夹，请先在 Chrome 中重新加载扩展。" :
        !status?.chrome_installed_version ? "插件文件与连接器已准备好。首次使用请打开固定目录，在 Chrome 中加载该文件夹，再填写当前扩展 ID。" :
        next.chrome_installed_version !== status.chrome_installed_version ? "Chrome 插件文件已更新，请在 Chrome 扩展管理页重新加载；扩展 ID 保持不变。" :
        next.bridge_installed_version !== status.bridge_installed_version ? "本地连接器已更新。Chrome 插件版本未变，无需重新加载。" :
        "没有需要更新的插件文件，已保留现有文件和扩展 ID。");
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  return <div className="browser-extension-manager" aria-label="Chrome 插件版本与配置">
    <strong>插件与 App 分开升级</strong>
    <p className="hint">App 更新会自动同步连接目标；插件版本不变时，保留插件文件与扩展 ID，无需重新配置。此处检查的是当前 App 附带的版本。</p>
    {status && <dl className="extension-versions">
      <div><dt>Chrome 插件</dt><dd>已准备 {status.chrome_installed_version || "未安装"} · App 附带 {status.chrome_version}</dd></div>
      <div><dt>本地连接器</dt><dd>已安装 {status.bridge_installed_version || "未托管"} · App 附带 {status.bridge_version}</dd></div>
    </dl>}
    <div className="extension-manager-actions">
      <button type="button" className="btn btn-primary" disabled={busy || !status} onClick={() => void run(false)}>{busy ? "正在处理…" : !status?.chrome_installed_version ? "一键准备插件" : "检查并更新插件"}</button>
      <button type="button" className="btn" disabled={busy || !status?.chrome_installed_version} onClick={() => { void api.openChromeExtensionDirectory().catch((reason) => setError(String(reason))); }}>打开插件文件夹</button>
    </div>
    {status?.chrome_installed_version && <>
      <p className="hint extension-directory">固定目录：{status.chrome_directory}</p>
      <label className="extension-id-label" htmlFor="chrome-extension-id">{isWindows ? "Chrome / Edge 扩展 ID" : "Chrome 扩展 ID"}</label>
      <div className="extension-manager-actions">
        <input id="chrome-extension-id" value={extensionId} onChange={(event) => setExtensionId(event.target.value)} placeholder="从扩展管理页复制 32 位 ID" autoCapitalize="none" spellCheck={false} disabled={busy} />
        <button type="button" className="btn" disabled={busy || !/^[a-p]{32}$/.test(extensionId.trim())} onClick={() => void run(true)}>{isWindows ? "连接浏览器" : "连接 Chrome"}</button>
      </div>
      <p className="hint">首次从旧文件夹迁移到此固定目录时，请以 Chrome 新显示的 ID 为准。首次加载和更新后的重新加载仍需在 Chrome 中完成。</p>
    </>}
    {message && <p className="hint" role="status">{message}</p>}
    {error && <p className="error-text" role="alert">{error}</p>}
  </div>;
}
