import { useEffect, useState } from "react";
import { api } from "../api";
import type { NewTaskRequest, Settings } from "../types";
import { formatBytes } from "../types";

interface Props {
  settings: Settings;
  defaultUrl?: string;
  onClose: () => void;
  onSubmit: (req: NewTaskRequest) => void;
}

export function NewDownloadDialog({ settings, defaultUrl, onClose, onSubmit }: Props) {
  const [url, setUrl] = useState(defaultUrl ?? "");
  const [dir, setDir] = useState(settings.download_dir);
  const [filename, setFilename] = useState("");
  const [connections, setConnections] = useState(settings.connections);
  const [speedLimitKb, setSpeedLimitKb] = useState(settings.speed_limit / 1024);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (defaultUrl) setUrl(defaultUrl);
  }, [defaultUrl]);

  const pickDir = async () => {
    try {
      const picked = await api.pickDirectory();
      if (picked) setDir(picked);
    } catch (e) {
      console.error(e);
    }
  };

  const submit = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setBusy(true);
    try {
      onSubmit({
        url: trimmed,
        dir,
        filename: filename.trim() || null,
        connections,
        speed_limit: Math.max(0, Math.round(speedLimitKb * 1024)),
      });
      onClose();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>新建下载任务</h2>

        <div className="field">
          <label>下载链接 URL</label>
          <input
            type="text"
            value={url}
            autoFocus
            placeholder="https://example.com/file.zip"
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
          />
        </div>

        <div className="field">
          <label>保存到</label>
          <div style={{ display: "flex", gap: 8 }}>
            <input
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
          <label>文件名（留空自动从 URL 推断）</label>
          <input
            type="text"
            value={filename}
            placeholder="自动推断"
            onChange={(e) => setFilename(e.target.value)}
          />
        </div>

        <div className="field-row">
          <div className="field">
            <label>连接数（线程）</label>
            <input
              type="number"
              min={1}
              max={32}
              value={connections}
              onChange={(e) => setConnections(Number(e.target.value) || 1)}
            />
          </div>
          <div className="field">
            <label>限速 KB/s（0 = 不限）</label>
            <input
              type="number"
              min={0}
              value={speedLimitKb}
              onChange={(e) => setSpeedLimitKb(Number(e.target.value) || 0)}
            />
          </div>
        </div>

        <div style={{ fontSize: 12, color: "var(--text-dim)" }}>
          默认下载目录：{settings.download_dir}
          {settings.speed_limit > 0 && ` · 全局限速 ${formatBytes(settings.speed_limit)}/s`}
        </div>

        <div className="modal-footer">
          <button className="btn" onClick={onClose} disabled={busy}>
            取消
          </button>
          <button className="btn btn-primary" onClick={submit} disabled={busy || !url.trim()}>
            开始下载
          </button>
        </div>
      </div>
    </div>
  );
}
