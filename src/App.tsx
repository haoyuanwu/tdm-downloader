import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, onProgress, onTasks } from "./api";
import { NewDownloadDialog } from "./components/NewDownloadDialog";
import { SettingsPanel } from "./components/SettingsPanel";
import { TaskCard } from "./components/TaskCard";
import type { DownloadTask, NewTaskRequest, Settings } from "./types";
import { formatBytes, formatSpeed } from "./types";

type Filter = "all" | "active" | "completed";

const DEFAULT_SETTINGS: Settings = {
  download_dir: "",
  connections: 8,
  speed_limit: 0,
  max_concurrent: 3,
  enable_segment: true,
  segment_threshold: 1024 * 1024,
};

export default function App() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [filter, setFilter] = useState<Filter>("all");
  const [showNew, setShowNew] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [clipboardUrl, setClipboardUrl] = useState<string | undefined>();

  // 用于"批量添加"（多行 URL）
  const speedsRef = useRef(speeds);
  speedsRef.current = speeds;

  // ── 初始加载 ──
  useEffect(() => {
    (async () => {
      try {
        const [list, cfg] = await Promise.all([api.listTasks(), api.getSettings()]);
        setTasks(list);
        if (cfg) setSettings(cfg);
      } catch (e) {
        console.error("初始化失败", e);
      }
    })();
  }, []);

  // ── 事件订阅 ──
  useEffect(() => {
    let un1: (() => void) | undefined;
    let un2: (() => void) | undefined;

    (async () => {
      un1 = await onTasks((list) => setTasks(list));
      un2 = await onProgress((p) => {
        setSpeeds((s) => ({ ...s, [p.id]: p.speed }));
        // 同步任务级进度，减少列表刷新延迟
        setTasks((prev) =>
          prev.map((t) =>
            t.id === p.id
              ? { ...t, downloaded: p.downloaded, total_size: p.total_size || t.total_size, status: p.status, segments: p.segments }
              : t
          )
        );
      });
    })();

    return () => {
      un1?.();
      un2?.();
    };
  }, []);

  // ── 剪贴板链接自动识别 ──
  useEffect(() => {
    let cancelled = false;
    const check = async () => {
      try {
        const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
        const text = await readText();
        if (!cancelled && text && /^https?:\/\/\S+$/i.test(text.trim())) {
          setClipboardUrl(text.trim());
        }
      } catch {
        /* 忽略 */
      }
    };
    check();
    const timer = setInterval(check, 5000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  // ── 操作 ──
  const startDownload = useCallback(async (req: NewTaskRequest) => {
    try {
      await api.startDownload(req);
      const list = await api.listTasks();
      setTasks(list);
    } catch (e) {
      console.error("创建任务失败", e);
      alert(`创建任务失败：${e}`);
    }
  }, []);

  const pause = useCallback(async (id: string) => {
    await api.pauseTask(id);
    setTasks(await api.listTasks());
  }, []);

  const resume = useCallback(async (id: string) => {
    await api.resumeTask(id);
    setTasks(await api.listTasks());
  }, []);

  const cancel = useCallback(async (id: string) => {
    await api.cancelTask(id);
    setTasks(await api.listTasks());
  }, []);

  const remove = useCallback(async (id: string) => {
    await api.removeTask(id, false);
    setTasks(await api.listTasks());
  }, []);

  const openFile = useCallback(async (task: DownloadTask) => {
    try {
      await api.openPath(task.save_path);
    } catch (e) {
      console.error(e);
    }
  }, []);

  const openDir = useCallback(async (task: DownloadTask) => {
    try {
      await api.openDownloadDir(task.save_path);
    } catch (e) {
      console.error(e);
    }
  }, []);

  // ── 统计 ──
  const stats = useMemo(() => {
    const active = tasks.filter(
      (t) => t.status === "downloading" || t.status === "connecting" || t.status === "merging"
    );
    const totalSpeed = active.reduce((sum, t) => sum + (speeds[t.id] ?? 0), 0);
    const done = tasks.filter((t) => t.status === "completed");
    const doneBytes = done.reduce((sum, t) => sum + t.downloaded, 0);
    return { activeCount: active.length, totalSpeed, doneCount: done.length, doneBytes };
  }, [tasks, speeds]);

  const filtered = useMemo(() => {
    if (filter === "active")
      return tasks.filter((t) => t.status !== "completed" && t.status !== "cancelled");
    if (filter === "completed") return tasks.filter((t) => t.status === "completed");
    return tasks;
  }, [tasks, filter]);

  return (
    <div className="app">
      <div className="header">
        <h1>
          <span className="logo">⬇</span>
          TDM Downloader
        </h1>

        <div className="tabs">
          {(["all", "active", "completed"] as Filter[]).map((f) => (
            <button
              key={f}
              className={`tab ${filter === f ? "active" : ""}`}
              onClick={() => setFilter(f)}
            >
              {f === "all" ? "全部" : f === "active" ? "进行中" : "已完成"}
            </button>
          ))}
        </div>

        <button className="btn btn-primary" onClick={() => setShowNew(true)}>
          + 新建下载
        </button>
        <button className="btn" onClick={() => setShowSettings(true)}>
          ⚙ 设置
        </button>
      </div>

      {clipboardUrl && (
        <div
          style={{
            padding: "8px 16px",
            background: "rgba(88,166,255,0.1)",
            borderBottom: "1px solid var(--border)",
            fontSize: 12,
            display: "flex",
            alignItems: "center",
            gap: 10,
          }}
        >
          <span style={{ color: "var(--text-dim)" }}>检测到剪贴板链接：</span>
          <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {clipboardUrl}
          </span>
          <button className="btn btn-icon" onClick={() => setShowNew(true)}>
            下载
          </button>
          <button className="btn btn-icon" onClick={() => setClipboardUrl(undefined)}>
            ✕
          </button>
        </div>
      )}

      <div className="content">
        {filtered.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">⬇</div>
            <div>还没有下载任务</div>
            <div style={{ marginTop: 8, fontSize: 12 }}>
              点击右上角「新建下载」开始，或复制链接后自动识别
            </div>
          </div>
        ) : (
          <div className="task-list">
            {filtered.map((t) => (
              <TaskCard
                key={t.id}
                task={t}
                speed={speeds[t.id] ?? 0}
                onPause={pause}
                onResume={resume}
                onCancel={cancel}
                onRemove={remove}
                onOpen={openFile}
                onOpenDir={openDir}
              />
            ))}
          </div>
        )}
      </div>

      <div className="footer">
        <span>
          进行中 <b>{stats.activeCount}</b>
        </span>
        <span>
          总速度 <b>{formatSpeed(stats.totalSpeed)}</b>
        </span>
        <span>
          已完成 <b>{stats.doneCount}</b> 个 · <b>{formatBytes(stats.doneBytes)}</b>
        </span>
        <span style={{ flex: 1 }} />
        <span>连接数 {settings.connections} · 并发上限 {settings.max_concurrent}</span>
        {tasks.some((t) => t.status === "completed") && (
          <button
            className="btn btn-icon"
            onClick={async () => {
              await api.clearCompleted();
              setTasks(await api.listTasks());
            }}
          >
            清空已完成
          </button>
        )}
      </div>

      {showNew && (
        <NewDownloadDialog
          settings={settings}
          defaultUrl={clipboardUrl}
          onClose={() => setShowNew(false)}
          onSubmit={startDownload}
        />
      )}

      {showSettings && (
        <SettingsPanel
          settings={settings}
          onClose={() => setShowSettings(false)}
          onSaved={setSettings}
        />
      )}
    </div>
  );
}
