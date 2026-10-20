import { useState } from "react";
import { api } from "../api";
import type { Settings } from "../types";
import { formatBytes } from "../types";

interface Props {
  settings: Settings;
  onClose: () => void;
  onSaved: (s: Settings) => void;
}

export function SettingsPanel({ settings, onClose, onSaved }: Props) {
  const [form, setForm] = useState<Settings>({ ...settings });
  const [saving, setSaving] = useState(false);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const pickDir = async () => {
    try {
      const picked = await api.pickDirectory();
      if (picked) set("download_dir", picked);
    } catch (e) {
      console.error(e);
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      await api.saveSettings(form);
      onSaved(form);
      onClose();
    } catch (e) {
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>设置</h2>

        <div className="field">
          <label>默认下载目录</label>
          <div style={{ display: "flex", gap: 8 }}>
            <input
              type="text"
              value={form.download_dir}
              onChange={(e) => set("download_dir", e.target.value)}
              style={{ flex: 1 }}
            />
            <button className="btn" onClick={pickDir}>
              浏览
            </button>
          </div>
        </div>

        <div className="field-row">
          <div className="field">
            <label>默认连接数</label>
            <input
              type="number"
              min={1}
              max={32}
              value={form.connections}
              onChange={(e) => set("connections", Number(e.target.value) || 1)}
            />
          </div>
          <div className="field">
            <label>最大并发任务数</label>
            <input
              type="number"
              min={1}
              max={16}
              value={form.max_concurrent}
              onChange={(e) => set("max_concurrent", Number(e.target.value) || 1)}
            />
          </div>
        </div>

        <div className="field-row">
          <div className="field">
            <label>全局限速 KB/s（0 = 不限）</label>
            <input
              type="number"
              min={0}
              value={form.speed_limit / 1024}
              onChange={(e) => set("speed_limit", Math.max(0, Math.round((Number(e.target.value) || 0) * 1024)))}
            />
          </div>
          <div className="field">
            <label>分段阈值 MB（大于此值才多线程）</label>
            <input
              type="number"
              min={0}
              value={form.segment_threshold / 1024 / 1024}
              onChange={(e) =>
                set("segment_threshold", Math.max(0, Math.round((Number(e.target.value) || 0) * 1024 * 1024)))
              }
            />
          </div>
        </div>

        <div className="checkbox-row">
          <input
            type="checkbox"
            id="enable_segment"
            checked={form.enable_segment}
            onChange={(e) => set("enable_segment", e.target.checked)}
          />
          <label htmlFor="enable_segment" style={{ margin: 0, color: "var(--text)" }}>
            启用多线程分段下载（服务端需支持 Range 请求）
          </label>
        </div>

        <div style={{ fontSize: 12, color: "var(--text-dim)" }}>
          当前全局限速：{form.speed_limit > 0 ? formatBytes(form.speed_limit) + "/s" : "不限速"}
        </div>

        <div className="modal-footer">
          <button className="btn" onClick={onClose} disabled={saving}>
            取消
          </button>
          <button className="btn btn-primary" onClick={save} disabled={saving}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
