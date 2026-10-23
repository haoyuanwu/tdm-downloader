import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "./Icon";
import { useDialog } from "../useDialog";
import { api } from "../api";
import type { NewTaskRequest, ProbeInfo, Settings } from "../types";
import { formatBytes } from "../types";

interface Props {
  settings: Settings;
  defaultUrl?: string;
  onClose: () => void;
  onSubmit: (reqs: NewTaskRequest[]) => void;
}

/** 从多行文本中解析出 URL 列表 */
function parseUrls(text: string): string[] {
  return text
    .split(/[\r\n,]+/)
    .map((s) => s.trim())
    .filter((s) => /^https?:\/\/\S+$/i.test(s));
}

export function NewDownloadDialog({ settings, defaultUrl, onClose, onSubmit }: Props) {
  const [text, setText] = useState(defaultUrl ?? "");
  const [dir, setDir] = useState(settings.download_dir);
  const [filename, setFilename] = useState("");
  const [connections, setConnections] = useState(settings.connections);
  const [speedLimitKb, setSpeedLimitKb] = useState(0);
  const [startPaused, setStartPaused] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [userAgent, setUserAgent] = useState("");
  const [referer, setReferer] = useState("");
  const [cookie, setCookie] = useState("");

  const [probe, setProbe] = useState<ProbeInfo | null>(null);
  const [probing, setProbing] = useState(false);
  const timerRef = useRef<number | undefined>(undefined);

  const dialog = useDialog(onClose);
  const urls = useMemo(() => parseUrls(text), [text]);
  const single = urls.length === 1;

  // ── URL 预解析（防抖 700ms，只解析第一个链接）──
  useEffect(() => {
    const first = urls[0];
    if (!first) {
      setProbe(null);
      return;
    }
    setProbing(true);
    window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(async () => {
      try {
        const headers: [string, string][] = [];
        if (referer.trim()) headers.push(["Referer", referer.trim()]);
        if (cookie.trim()) headers.push(["Cookie", cookie.trim()]);
        const info = await api.probeUrl(
          first,
          userAgent.trim() || undefined,
          headers.length ? headers : undefined
        );
        setProbe(info);
        // 服务端给出文件名且用户未手填时，自动填入
        if (info.ok && info.filename && !filename.trim()) {
          setFilename(info.filename);
        }
      } catch {
        setProbe(null);
      } finally {
        setProbing(false);
      }
    }, 700);
    return () => window.clearTimeout(timerRef.current);
  }, [text, userAgent, referer, cookie, filename, urls]);

  const pickDir = async () => {
    try {
      const picked = await api.pickDirectory();
      if (picked) setDir(picked);
    } catch (e) {
      console.error(e);
    }
  };

  const submit = useCallback(() => {
    if (urls.length === 0) return;
    const headers: [string, string][] = [];
    if (referer.trim()) headers.push(["Referer", referer.trim()]);
    if (cookie.trim()) headers.push(["Cookie", cookie.trim()]);

    const reqs: NewTaskRequest[] = urls.map((u) => ({
      url: u,
      dir,
      filename: single ? filename.trim() || null : null,
      connections,
      speed_limit: Math.max(0, Math.round(speedLimitKb * 1024)),
      user_agent: userAgent.trim() || null,
      headers: headers.length ? headers : null,
      start_paused: startPaused,
    }));
    onSubmit(reqs);
    onClose();
  }, [
    urls,
    dir,
    filename,
    single,
    connections,
    speedLimitKb,
    userAgent,
    referer,
    cookie,
    startPaused,
    onSubmit,
    onClose,
  ]);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div ref={dialog} className="modal new-download-dialog" role="dialog" aria-modal="true" aria-labelledby="new-download-title" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header"><div><h2 id="new-download-title">新建下载</h2><p className="dialog-subtitle">添加链接，剩下的交给 TDM</p></div><button className="btn settings-close" aria-label="关闭新建下载" onClick={onClose}><Icon name="close" size={18} /></button></div>

        <div className="field">
          <label htmlFor="download-field-1">下载链接（每行一个，可一次粘贴多个）</label>
          <textarea id="download-field-1"
            className="url-area"
            value={text}
            data-initial-focus
            rows={3}
            placeholder={"https://example.com/file.zip\nhttps://example.com/other.iso"}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submit();
            }}
          />
          {urls.length > 1 && (
            <div className="hint">将创建 {urls.length} 个下载任务</div>
          )}
        </div>

        {single && (
          <div className={`probe-box ${probe && !probe.ok ? "error" : ""}`}>
            {probing && <span className="hint">解析中…</span>}
            {!probing && probe?.ok && (
              <>
                <span className="tag">可下载</span>
                <span>{probe.filename || "（未获取到文件名）"}</span>
                <span className="dim">
                  {probe.total_size > 0 ? formatBytes(probe.total_size) : "大小未知"}
                </span>
                <span className={`tag ${probe.accept_ranges ? "ok" : "warn"}`}>
                  {probe.accept_ranges ? "支持断点续传" : "不支持续传"}
                </span>
              </>
            )}
            {!probing && probe && !probe.ok && (
              <span>无法连接：{probe.error}</span>
            )}
          </div>
        )}

        <div className="field">
          <label htmlFor="download-field-2">保存到</label>
          <div style={{ display: "flex", gap: 8 }}>
            <input id="download-field-2"
              type="text"
              value={dir}
              onChange={(e) => setDir(e.target.value)}
              style={{ flex: 1 }}
            />
            <button className="btn" onClick={pickDir}>
              浏览
            </button>
          </div>
        </div>

        <div className="field">
          <label htmlFor="download-field-3">文件名（留空自动推断）</label>
          <input id="download-field-3"
            type="text"
            value={filename}
            placeholder={single ? "自动推断" : "批量下载时逐个自动推断"}
            disabled={!single}
            onChange={(e) => setFilename(e.target.value)}
          />
        </div>

        <div className="field-row">
          <div className="field">
            <label htmlFor="download-field-4">连接数（线程）</label>
            <input id="download-field-4"
              type="number"
              min={1}
              max={32}
              value={connections}
              onChange={(e) => setConnections(Number(e.target.value) || 1)}
            />
          </div>
          <div className="field">
            <label htmlFor="download-field-5">限速 KB/s（0 = 不限）</label>
            <input id="download-field-5"
              type="number"
              min={0}
              value={speedLimitKb}
              onChange={(e) => setSpeedLimitKb(Number(e.target.value) || 0)}
            />
          </div>
        </div>

        <button className="advanced-toggle" aria-expanded={showAdvanced} onClick={() => setShowAdvanced((v) => !v)}><Icon name="chevron" size={16} style={{ transform: showAdvanced ? "rotate(180deg)" : undefined }} />高级选项<span>请求头、限速与启动方式</span></button>

        {showAdvanced && (
          <div className="advanced">
            <div className="field">
              <label htmlFor="download-field-6">User-Agent（留空使用全局设置）</label>
              <input id="download-field-6"
                type="text"
                value={userAgent}
                placeholder={settings.user_agent || "默认"}
                onChange={(e) => setUserAgent(e.target.value)}
              />
            </div>
            <div className="field-row">
              <div className="field">
                <label htmlFor="download-field-7">Referer</label>
                <input id="download-field-7"
                  type="text"
                  value={referer}
                  placeholder="https://"
                  onChange={(e) => setReferer(e.target.value)}
                />
              </div>
              <div className="field">
                <label htmlFor="download-field-8">Cookie</label>
                <input id="download-field-8"
                  type="text"
                  value={cookie}
                  placeholder="name=value; ..."
                  onChange={(e) => setCookie(e.target.value)}
                />
              </div>
            </div>
            <div className="checkbox-row">
              <input
                type="checkbox"
                id="start_paused"
                checked={startPaused}
                onChange={(e) => setStartPaused(e.target.checked)}
              />
              <label htmlFor="start_paused" style={{ margin: 0 }}>
                仅添加到列表，稍后手动开始
              </label>
            </div>
          </div>
        )}

        <div className="modal-footer">
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button
            className="btn btn-primary"
            onClick={submit}
            disabled={urls.length === 0}
          >
            {startPaused ? "添加到列表" : "开始下载"}
            {urls.length > 1 ? ` (${urls.length})` : ""}
          </button>
        </div>
      </div>
    </div>
  );
}
