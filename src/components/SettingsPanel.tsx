import { useEffect, useState } from "react";
import { api } from "../api";
import type {
  AfterFinishAction,
  FileExistsAction,
  ProxyMode,
  Settings,
  ThemeMode,
} from "../types";
import { formatBytes } from "../types";

interface Props {
  settings: Settings;
  onClose: () => void;
  onSaved: (s: Settings) => void;
}

const PROXY_LABEL: Record<ProxyMode, string> = {
  system: "跟随系统代理",
  none: "直连（不使用代理）",
  custom: "自定义代理",
};

const EXISTS_LABEL: Record<FileExistsAction, string> = {
  rename: "自动重命名",
  overwrite: "覆盖原文件",
  skip: "跳过不下载",
};

const FINISH_LABEL: Record<AfterFinishAction, string> = {
  none: "不执行操作",
  quit: "退出 TDM Downloader",
  shutdown: "关机（30 秒后）",
  sleep: "休眠",
};

export function SettingsPanel({ settings, onClose, onSaved }: Props) {
  const [form, setForm] = useState<Settings>({ ...settings });
  const [saving, setSaving] = useState(false);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    api.getAutostart().then(setAutostart).catch(() => {});
  }, []);

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
      await api.setAutostart(autostart);
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
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <h2>设置</h2>

        <div className="settings-body">
          <section>
            <h3 className="section-title">下载</h3>

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
                <label>默认连接数（1-32）</label>
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
                  onChange={(e) =>
                    set("speed_limit", Math.max(0, Math.round((Number(e.target.value) || 0) * 1024)))
                  }
                />
              </div>
              <div className="field">
                <label>分段阈值 MB</label>
                <input
                  type="number"
                  min={0}
                  value={form.segment_threshold / 1024 / 1024}
                  onChange={(e) =>
                    set(
                      "segment_threshold",
                      Math.max(0, Math.round((Number(e.target.value) || 0) * 1024 * 1024))
                    )
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
              <label htmlFor="enable_segment" style={{ margin: 0 }}>
                启用多线程分段下载（服务端需支持 Range 请求）
              </label>
            </div>

            <div className="field">
              <label>文件已存在时</label>
              <select
                value={form.file_exists}
                onChange={(e) => set("file_exists", e.target.value as FileExistsAction)}
              >
                {(Object.keys(EXISTS_LABEL) as FileExistsAction[]).map((k) => (
                  <option key={k} value={k}>
                    {EXISTS_LABEL[k]}
                  </option>
                ))}
              </select>
            </div>
          </section>

          <section>
            <h3 className="section-title">网络</h3>

            <div className="field">
              <label>代理</label>
              <select
                value={form.proxy_mode}
                onChange={(e) => set("proxy_mode", e.target.value as ProxyMode)}
              >
                {(Object.keys(PROXY_LABEL) as ProxyMode[]).map((k) => (
                  <option key={k} value={k}>
                    {PROXY_LABEL[k]}
                  </option>
                ))}
              </select>
            </div>

            {form.proxy_mode === "custom" && (
              <div className="field">
                <label>代理地址</label>
                <input
                  type="text"
                  value={form.proxy_addr}
                  placeholder="http://127.0.0.1:8080"
                  onChange={(e) => set("proxy_addr", e.target.value)}
                />
              </div>
            )}

            <div className="field">
              <label>默认 User-Agent</label>
              <input
                type="text"
                value={form.user_agent}
                placeholder="留空使用内置 UA"
                onChange={(e) => set("user_agent", e.target.value)}
              />
            </div>
            <div className="hint">
              {form.proxy_mode === "system"
                ? "跟随 macOS 网络设置中的代理；PAC 自动配置请使用自定义代理地址"
                : form.proxy_mode === "none"
                ? "直连，同时忽略 HTTP_PROXY 等环境变量"
                : "使用下方手动填写的代理"}
            </div>
          </section>

          <section>
            <h3 className="section-title">系统</h3>

            <div className="field">
              <label>全部下载完成后</label>
              <select
                value={form.after_finish}
                onChange={(e) => set("after_finish", e.target.value as AfterFinishAction)}
              >
                {(Object.keys(FINISH_LABEL) as AfterFinishAction[]).map((k) => (
                  <option key={k} value={k}>
                    {FINISH_LABEL[k]}
                  </option>
                ))}
              </select>
            </div>

            <div className="checkbox-row">
              <input
                type="checkbox"
                id="close_to_tray"
                checked={form.close_to_tray}
                onChange={(e) => set("close_to_tray", e.target.checked)}
              />
              <label htmlFor="close_to_tray" style={{ margin: 0 }}>
                关闭窗口时最小化到系统托盘
              </label>
            </div>

            <div className="checkbox-row">
              <input
                type="checkbox"
                id="autostart"
                checked={autostart}
                onChange={(e) => setAutostart(e.target.checked)}
              />
              <label htmlFor="autostart" style={{ margin: 0 }}>
                开机自动启动
              </label>
            </div>
          </section>

          <section>
            <h3 className="section-title">外观</h3>
            <div className="field">
              <label>主题</label>
              <select
                value={form.theme}
                onChange={(e) => set("theme", e.target.value as ThemeMode)}
              >
                <option value="dark">深色</option>
                <option value="light">浅色</option>
              </select>
            </div>
            <div className="hint">当前全局限速：{form.speed_limit > 0 ? formatBytes(form.speed_limit) + "/s" : "不限速"}</div>
          </section>
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
