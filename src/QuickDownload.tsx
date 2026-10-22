import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, EVT_QUICK_OPEN } from "./api";
import type { QuickOpenContext } from "./api";
import QuickTaskList from "./QuickTaskList";
import { DEFAULT_SETTINGS } from "./types";
import type { ThemeMode } from "./types";
import { useTheme } from "./useTheme";

const isUrl = (value: string) => {
  try {
    const url = new URL(value.trim());
    return (url.protocol === "http:" || url.protocol === "https:") && Boolean(url.hostname);
  } catch {
    return false;
  }
};

export default function QuickDownload() {
  const [value, setValue] = useState("");
  const [hint, setHint] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [mode, setMode] = useState<QuickOpenContext["mode"]>("manual");
  const [filename, setFilename] = useState("");
  const field = useRef<HTMLInputElement>(null);
  const refreshId = useRef(0);
  const [theme, setTheme] = useState<ThemeMode>(DEFAULT_SETTINGS.theme);
  useTheme(theme);

  const refresh = useCallback(async (context: QuickOpenContext) => {
    const id = ++refreshId.current;
    const automatic = context.mode === "clipboard" && Boolean(context.url);
    const nextMode = context.mode === "tasks" ? "tasks" : automatic ? "clipboard" : "manual";
    setMode(nextMode);
    setFilename(context.filename || "");
    setValue(automatic ? context.url! : "");
    setError("");
    setHint(null);
    if (nextMode === "manual") requestAnimationFrame(() => field.current?.focus());
    const settings = await api.getSettings().catch(() => null);
    if (settings) setTheme(settings.theme);
    if (nextMode === "manual") {
      const clipboardHint = await api.quickClipboardHint().catch(() => null);
      if (id === refreshId.current) setHint(clipboardHint);
    }
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;
    void listen<QuickOpenContext>(EVT_QUICK_OPEN, (event) => { void refresh(event.payload); }).then((fn) => {
      if (active) unlisten = fn;
      else fn();
    });
    void api.quickOpenContext().then(refresh).catch(() => {});
    return () => { active = false; unlisten?.(); };
  }, [refresh]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") void api.hideQuickWindow();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const download = async () => {
    const url = value.trim();
    if (!isUrl(url)) {
      setError("请输入有效的 HTTP 或 HTTPS 下载链接");
      return;
    }
    setBusy(true);
    setError("");
    try {
      await api.startDownload({ url, dir: "" });
      await api.hideQuickWindow();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const dismiss = () => { void api.hideQuickWindow(); };

  if (mode === "tasks") return <QuickTaskList />;

  return (
    <main className="quick-panel">
      <div className="quick-heading">
        <div className="quick-mark">↓</div>
        <div>
          <strong>{mode === "clipboard" ? "发现下载链接" : "快速下载"}</strong>
          <span>{mode === "clipboard" ? "是否将此文件加入下载任务？" : "输入链接，立即加入下载任务"}</span>
        </div>
        <button className="quick-close" aria-label="关闭" onClick={dismiss}>×</button>
      </div>
      {mode === "clipboard" ? (
        <div className="quick-found">
          <strong title={filename}>{filename || "下载文件"}</strong>
          <span title={value}>{value}</span>
        </div>
      ) : <form onSubmit={(e) => { e.preventDefault(); void download(); }}>
        <div className="quick-entry">
          <input
            ref={field}
            aria-label="下载地址"
            value={value}
            placeholder={hint ? `${hint} · 按 Tab 填入` : "输入或粘贴 HTTP(S) 下载链接"}
            onChange={(e) => { setValue(e.target.value); setError(""); }}
            onKeyDown={(e) => {
              if (e.key === "Tab" && !e.shiftKey && !value && hint) {
                e.preventDefault();
                setValue(hint);
              }
            }}
          />
          <button type="submit" disabled={busy || !value.trim()}>下载</button>
        </div>
      </form>}
      <div className="quick-footer">
        {mode === "clipboard" ? (
          <>
            <span className={error ? "quick-error" : ""}>{error}</span>
            <div className="quick-actions">
              <button onClick={dismiss}>暂不下载</button>
              <button className="quick-primary" disabled={busy} onClick={() => void download()}>下载</button>
            </div>
          </>
        ) : (
          <>
            <span className={error ? "quick-error" : ""}>{error || (hint ? "Tab 填入剪贴板链接 · 回车下载" : "回车下载 · Esc 关闭")}</span>
            <button onClick={() => void api.showMainWindow()}>任务列表 ↗</button>
          </>
        )}
      </div>
    </main>
  );
}
