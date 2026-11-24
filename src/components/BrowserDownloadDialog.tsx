import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { QuickOpenContext } from "../api";
import { useDialog } from "../useDialog";
import { Icon } from "./Icon";

interface Context { id: number; payload: QuickOpenContext }

function Confirmation({ context, onDone }: { context: Context; onDone: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [filename, setFilename] = useState<string | null>(null);
  const created = useRef(false);
  useEffect(() => {
    let active = true;
    void api.browserDialogFilename(context.id).then(
      (name) => { if (active) setFilename(name); },
      () => { if (active) setFilename(context.payload.filename || "下载文件"); },
    );
    return () => { active = false; };
  }, [context]);
  const close = async () => {
    if (busy) return;
    try { await api.finishBrowserDialog(context.id, created.current); onDone(); }
    catch (reason) { setError(String(reason)); }
  };
  const dialog = useDialog(() => { void close(); }, busy);
  const download = async () => {
    if (busy || !filename) return;
    setBusy(true); setError("");
    try {
      if (!created.current) {
        await api.startDownload({ url: context.payload.url!, dir: "", filename });
        created.current = true;
      }
      await api.finishBrowserDialog(context.id, true);
      onDone();
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };
  const suffix = filename?.match(/.(\.(?:tar\.)?[a-z\d]{1,16})$/i)?.[1] ?? "";
  return <div className="modal-overlay browser-download-overlay">
    <div ref={dialog} className="modal browser-download-dialog" role="dialog" aria-modal="true" aria-labelledby="browser-download-title">
      <div className="settings-header">
        <div><h2 id="browser-download-title">浏览器下载</h2><p className="dialog-subtitle">确认后将此文件加入下载任务</p></div>
        <button className="btn settings-close" aria-label="关闭下载确认" disabled={busy} onClick={() => void close()}><Icon name="close" size={18} /></button>
      </div>
      <div className="browser-download-file">
        <Icon name="download" size={24} />
        <strong title={filename ?? undefined} aria-live="polite">
          <span>{filename ? filename.slice(0, filename.length - suffix.length) : "正在识别文件名…"}</span>
          {suffix && <span>{suffix}</span>}
        </strong>
      </div>
      <p className="browser-download-url">{context.payload.url}</p>
      {error && <p className="error-text" role="alert">{error}</p>}
      <div className="modal-footer">
        <button className="btn" disabled={busy} onClick={() => void close()}>暂不下载</button>
        <button className="btn btn-primary" data-initial-focus disabled={busy} aria-disabled={!filename} onClick={() => void download()}>{busy ? "正在添加…" : created.current ? "完成交接" : "下载"}</button>
      </div>
    </div>
  </div>;
}

export function BrowserDownloadDialog() {
  const [context, setContext] = useState<Context | null>(null);
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout>;
    let latest = 0;
    const receive = (next: Context | null) => {
      if (!active || !next?.payload.url || next.id <= latest) return;
      latest = next.id;
      clearTimeout(timer);
      setContext(null);
      // Runs in the visible main page after wake-up, rather than a hidden tray WebView.
      timer = setTimeout(() => { if (active) setContext(next); }, 600);
    };
    void (async () => {
      const cleanup = await listen<Context>("tdm://browser-download", (event) => receive(event.payload));
      if (!active) { cleanup(); return; }
      stop = cleanup;
      receive(await api.browserDialogContext());
    })().catch((reason) => console.error("读取浏览器下载请求失败", reason));
    return () => { active = false; clearTimeout(timer); stop?.(); };
  }, []);
  return context ? <Confirmation key={context.id} context={context} onDone={() => setContext(null)} /> : null;
}
