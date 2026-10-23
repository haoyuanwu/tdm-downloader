import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import type { IconName } from "./Icon";
import { useDialog } from "../useDialog";
import type { NotificationStatus } from "../api";
import type {
  AfterFinishAction,
  FileExistsAction,
  ProxyMode,
  Settings,
  ThemeMode,
} from "../types";
import { formatBytes } from "../types";
import { isWindows, trashName } from "../platform";

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
  sleep: "睡眠",
};

export function SettingsPanel({ settings, onClose, onSaved }: Props) {
  const [form, setForm] = useState<Settings>({ ...settings });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [section, setSection] = useState("appearance");
  const [startupReady, setStartupReady] = useState(false);
  const [startupError, setStartupError] = useState("");
  const dialog = useDialog(onClose, saving);
  const loadStartup = useCallback(() => {
    setStartupReady(false);
    setStartupError("");
    void api.getAutostart().then((enabled) => {
      setForm((previous) => ({ ...previous, autostart: enabled }));
      setStartupReady(true);
    }).catch((error) => setStartupError(`读取开机启动状态失败：${String(error)}`));
  }, []);
  const [notification, setNotification] = useState<NotificationStatus | null>(null);
  const [notificationMessage, setNotificationMessage] = useState("");
  const [testingNotification, setTestingNotification] = useState(false);

  useEffect(() => {
    loadStartup();
    const refresh = () => { void api.notificationStatus().then(setNotification).catch((error) => setNotificationMessage(String(error))); };
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [loadStartup]);

  const testNotification = async () => {
    setTestingNotification(true);
    setNotificationMessage("");
    try {
      setNotificationMessage(await api.testDownloadNotification());
    } catch (error) {
      setNotificationMessage(String(error));
    } finally {
      setTestingNotification(false);
      void api.notificationStatus().then(setNotification).catch(() => {});
    }
  };

  const notificationHint = !notification ? "正在读取通知权限…"
    : notification.authorization === "denied" ? "通知权限已关闭，请在系统设置中允许通知"
    : isWindows && notification.authorization === "unknown" ? "开发版通知使用 Windows PowerShell 身份；请在安装版中检查应用通知权限"
    : notification.authorization !== "authorized" ? "需要允许发送通知，才能收到下载完成提醒"
    : notification.center_enabled === null || notification.alerts_enabled === null ? "系统允许发送通知；横幅、通知中心和勿扰状态请在系统通知设置中确认"
    : !notification.center_enabled ? "通知中心显示已关闭，请在系统设置中开启"
    : !notification.alerts_enabled ? "通知会显示在通知中心；横幅已关闭"
    : `已允许通知，下载完成后会显示横幅并保留在通知中心${notification.delivered_ids.length ? `（当前 ${notification.delivered_ids.length} 条提醒）` : ""}`;

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
    setSaveError("");
    try {
      await api.saveSettings(form);
      onSaved(form);
      onClose();
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={() => { if (!saving) onClose(); }}>
      <div ref={dialog} className="modal modal-wide settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <div><h2 id="settings-title">偏好设置</h2><p className="dialog-subtitle">让下载器更贴合你的工作方式</p></div>
          <button type="button" className="btn settings-close" aria-label="关闭设置" title="关闭（Esc）" onClick={onClose} disabled={saving}>
            <Icon name="close" size={18} />
          </button>
        </div>

        <div className="settings-layout">
          <nav className="settings-nav" aria-label="设置分类">
            {([
              ["appearance", "外观", "monitor"], ["downloads", "下载", "download"],
              ["network", "网络", "globe"], ["system", "启动与通知", "power"],
            ] as [string, string, IconName][]).map(([id, label, icon]) => <button key={id} className={section === id ? "is-active" : ""} aria-current={section === id ? "page" : undefined} onClick={() => setSection(id)}><Icon name={icon} size={18} />{label}</button>)}
          </nav>
          <div className="settings-body">
          <section hidden={section !== "appearance"}>
            <h3 className="section-title">外观</h3>
            <div className="field">
              <label>主题</label>
              <div className="theme-options" role="radiogroup" aria-label="主题">
                {([['system', '跟随系统'], ['light', '浅色'], ['dark', '深色']] as [ThemeMode, string][]).map(([id, label]) => <label key={id} className={`theme-option ${form.theme === id ? "is-selected" : ""}`}>
                  <input type="radio" name="theme" value={id} checked={form.theme === id} onChange={() => set("theme", id)} />
                  <span className={`theme-preview preview-${id}`}><i /><span><b /><b /><b /></span></span>
                  <span className="theme-option-label">{label}{form.theme === id && <Icon name="check" size={15} />}</span>
                </label>)}
              </div>
              <div className="hint">选择跟随系统后，将随系统外观自动切换深浅色。</div>
            </div>
          </section>
          <section hidden={section !== "downloads"}>
            <h3 className="section-title">下载</h3>

            <div className="field">
              <label htmlFor="preference-field-1">默认下载目录</label>
              <div style={{ display: "flex", gap: 8 }}>
                <input id="preference-field-1"
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
                <label htmlFor="preference-field-2">默认连接数（1-32）</label>
                <input id="preference-field-2"
                  type="number"
                  min={1}
                  max={32}
                  value={form.connections}
                  onChange={(e) => set("connections", Number(e.target.value) || 1)}
                />
              </div>
              <div className="field">
                <label htmlFor="preference-field-3">最大并发任务数</label>
                <input id="preference-field-3"
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
                <label htmlFor="preference-field-4">全局限速 KB/s（0 = 不限）</label>
                <input id="preference-field-4"
                  type="number"
                  min={0}
                  value={form.speed_limit / 1024}
                  onChange={(e) =>
                    set("speed_limit", Math.max(0, Math.round((Number(e.target.value) || 0) * 1024)))
                  }
                />
              </div>
              <div className="field">
                <label htmlFor="preference-field-5">分段阈值 MB</label>
                <input id="preference-field-5"
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
              <label htmlFor="preference-field-6">文件已存在时</label>
              <select id="preference-field-6"
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

            <div className="checkbox-row">
              <input type="checkbox" id="confirm_remove_task" checked={form.confirm_remove_task}
                onChange={(event) => set("confirm_remove_task", event.target.checked)} />
              <label htmlFor="confirm_remove_task" style={{ margin: 0 }}>删除任务时询问是否删除本地文件</label>
            </div>
            <div className="field">
              <label htmlFor="preference-field-7">删除任务时的文件处理方式</label>
              <select id="preference-field-7" value={form.remove_task_files ? "delete" : "keep"}
                onChange={(event) => set("remove_task_files", event.target.value === "delete")}>
                <option value="keep">保留本地文件</option>
                <option value="delete">同时删除本地文件（移到{trashName}）</option>
              </select>
              <div className="hint">开启询问时作为默认选项；关闭询问后直接按此项处理。</div>
            </div>
            <div className="hint">当前全局限速：{form.speed_limit > 0 ? formatBytes(form.speed_limit) + "/s" : "不限速"}</div>
          </section>

          <section hidden={section !== "network"}>
            <h3 className="section-title">网络</h3>

            <div className="field">
              <label htmlFor="preference-field-8">代理</label>
              <select id="preference-field-8"
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
                <label htmlFor="preference-field-9">代理地址</label>
                <input id="preference-field-9"
                  type="text"
                  value={form.proxy_addr}
                  placeholder="http://127.0.0.1:8080"
                  onChange={(e) => set("proxy_addr", e.target.value)}
                />
              </div>
            )}

            <div className="field">
              <label htmlFor="preference-field-10">默认 User-Agent</label>
              <input id="preference-field-10"
                type="text"
                value={form.user_agent}
                placeholder="留空使用内置 UA"
                onChange={(e) => set("user_agent", e.target.value)}
              />
            </div>
            <div className="hint">
              {form.proxy_mode === "system"
                ? `跟随${isWindows ? " Windows 系统" : " macOS 网络设置中的"}手动代理；PAC 自动配置请使用自定义代理地址`
                : form.proxy_mode === "none"
                ? "直连，同时忽略 HTTP_PROXY 等环境变量"
                : "使用下方手动填写的代理"}
            </div>
          </section>

          <section hidden={section !== "system"}>
            <h3 className="section-title">启动与通知</h3>
            <div className="startup-card">
              <div className="preference-row"><span className="preference-icon"><Icon name="power" /></span><label htmlFor="autostart"><strong>开机自动启动</strong><span>登录电脑后自动运行 TDM Downloader</span></label><input className="switch-input" type="checkbox" role="switch" id="autostart" checked={form.autostart} disabled={!startupReady || saving} onChange={(event) => set("autostart", event.target.checked)} /></div>
              <div className="preference-row startup-secondary"><label htmlFor="startup-hidden"><strong>启动时在后台运行</strong><span>{isWindows ? "仅显示托盘图标，需要时再打开主窗口" : "仅显示菜单栏图标，保持桌面整洁"}</span></label><input className="switch-input" type="checkbox" role="switch" id="startup-hidden" checked={form.startup_hidden} disabled={!form.autostart || saving} onChange={(event) => set("startup_hidden", event.target.checked)} /></div>
              <div className="startup-state"><span className={`connection-dot ${form.autostart ? "is-active" : ""}`} />{!startupReady && !startupError ? "正在读取启动项…" : form.autostart ? "保存后，下次登录将自动启动" : "当前由你手动启动"}</div>
            </div>

            <div className="field">
              <label>下载完成提醒</label>
              <div className="notification-controls">
                <button className="btn" disabled={testingNotification} onClick={() => void testNotification()}>
                  {testingNotification ? "正在发送…" : "测试通知和提示音"}
                </button>
                <button className="btn" onClick={() => void api.openNotificationSettings().catch((error) => setNotificationMessage(String(error)))}>打开通知设置</button>
              </div>
              <div className="hint" role="status">{notificationMessage || notification?.last_error || notificationHint}</div>
            </div>

            <div className="field">
              <label htmlFor="preference-field-11">全部下载完成后</label>
              <select id="preference-field-11"
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


          </section>
          </div>
        </div>

        {startupError && <div className="error-text" role="alert">{startupError}<button className="text-button" onClick={loadStartup}>重试</button></div>}
        {saveError && <p className="error-text" role="alert">{saveError}</p>}
        <div className="modal-footer"><span className="settings-save-hint">更改将在保存后生效</span>
          <button className="btn" onClick={onClose} disabled={saving}>
            取消
          </button>
          <button className="btn btn-primary" onClick={save} disabled={saving || !startupReady}>
            {saving ? "正在保存…" : "保存设置"}
          </button>
        </div>
      </div>
    </div>
  );
}
