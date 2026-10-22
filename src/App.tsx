import { useCallback, useEffect, useMemo, useState } from "react";
import { api, onNewTask, onProgress, onTasks, onShowDownloads } from "./api";
import { NewDownloadDialog } from "./components/NewDownloadDialog";
import { SettingsPanel } from "./components/SettingsPanel";
import { TaskCard } from "./components/TaskCard";
import { RemoveTaskDialog } from "./components/RemoveTaskDialog";
import { Tooltip } from "./components/Tooltip";
import type { AppInfo, DownloadTask, NewTaskRequest, Settings } from "./types";
import { DEFAULT_SETTINGS, formatBytes, formatSpeed } from "./types";
import { useTheme } from "./useTheme";

type Filter = "all" | "active" | "completed";

export default function App() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [showNew, setShowNew] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [clipboardUrl, setClipboardUrl] = useState<string | undefined>();
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [removeRequest, setRemoveRequest] = useState<DownloadTask[] | null>(null);
  const [removing, setRemoving] = useState(false);
  const [removeError, setRemoveError] = useState("");

  const openNewDownload = useCallback(async () => {
    try {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      const text = (await readText()).trim();
      setClipboardUrl(/^https?:\/\/\S+$/i.test(text) ? text : undefined);
    } catch {
      setClipboardUrl(undefined);
    }
    setShowNew(true);
  }, []);

  // ── 初始加载 ──
  useEffect(() => {
    (async () => {
      try {
        const [list, cfg, info] = await Promise.all([
          api.listTasks(),
          api.getSettings(),
          api.getAppInfo(),
        ]);
        setTasks(list);
        setAppInfo(info);
        if (cfg) setSettings({ ...DEFAULT_SETTINGS, ...cfg });
      } catch (e) {
        console.error("初始化失败", e);
      }
    })();
  }, []);

  // ── 主题 ──
  useTheme(settings.theme);

  // ── 事件订阅 ──
  useEffect(() => {
    let un1: (() => void) | undefined;
    let un2: (() => void) | undefined;
    let un3: (() => void) | undefined;

    (async () => {
      un1 = await onTasks((list) => setTasks(list));
      un2 = await onProgress((p) => {
        setSpeeds((s) => ({ ...s, [p.id]: p.speed }));
        setTasks((prev) =>
          prev.map((t) =>
            t.id === p.id
              ? {
                  ...t,
                  downloaded: p.downloaded,
                  total_size: p.total_size || t.total_size,
                  status: p.status,
                  segments: p.segments,
                }
              : t
          )
        );
      });
      // 托盘菜单「新建下载」
      un3 = await onNewTask(() => { void openNewDownload(); });
    })();

    return () => {
      un1?.();
      un2?.();
      un3?.();
    };
  }, [openNewDownload]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void onShowDownloads(() => {
      setShowSettings(false);
      setShowNew(false);
      setFilter("all");
      setQuery("");
    }).then((cleanup) => { if (active) unlisten = cleanup; else cleanup(); });
    return () => { active = false; unlisten?.(); };
  }, []);

  // ── 任务栏进度条 ──
  useEffect(() => {
    const active = tasks.filter(
      (t) => t.status === "downloading" || t.status === "connecting" || t.status === "merging"
    );
    if (active.length === 0) {
      api.setProgressBar(null).catch(() => {});
      return;
    }
    const total = active.reduce((s, t) => s + (t.total_size || 0), 0);
    const done = active.reduce((s, t) => s + t.downloaded, 0);
    api.setProgressBar(total > 0 ? done / total : null).catch(() => {});
  }, [tasks]);

  // ── 操作 ──
  const refresh = useCallback(async () => {
    setTasks(await api.listTasks());
  }, []);

  const startDownloads = useCallback(
    async (reqs: NewTaskRequest[]) => {
      for (const r of reqs) {
        try {
          await api.startDownload(r);
        } catch (e) {
          console.error("创建任务失败", e);
        }
      }
      await refresh();
    },
    [refresh]
  );

  const pause = useCallback(
    async (id: string) => {
      await api.pauseTask(id);
      await refresh();
    },
    [refresh]
  );

  const resume = useCallback(
    async (id: string) => {
      await api.resumeTask(id);
      await refresh();
    },
    [refresh]
  );

  const cancel = useCallback(
    async (id: string) => {
      await api.cancelTask(id);
      await refresh();
    },
    [refresh]
  );

  const performRemove = useCallback(
    async (requested: DownloadTask[], deleteFiles: boolean, remember: boolean) => {
      setRemoving(true);
      setRemoveError("");
      try {
        // 批量移除失败后重试时，跳过已成功移除的任务。
        const existing = new Set((await api.listTasks()).map((task) => task.id));
        for (const task of requested) {
          if (existing.has(task.id)) await api.removeTask(task.id, deleteFiles);
        }
        if (remember) {
          const next = { ...settings, confirm_remove_task: false, remove_task_files: deleteFiles };
          await api.saveSettings(next);
          setSettings(next);
        }
        setRemoveRequest(null);
        await refresh();
      } catch (error) {
        setRemoveRequest(requested);
        setRemoveError(String(error));
        await refresh().catch(() => {});
      } finally {
        setRemoving(false);
      }
    },
    [refresh, settings]
  );

  const requestRemove = useCallback((requested: DownloadTask[]) => {
    if (!requested.length || removing) return;
    setRemoveError("");
    if (settings.confirm_remove_task) setRemoveRequest(requested);
    else void performRemove(requested, settings.remove_task_files, false);
  }, [performRemove, removing, settings.confirm_remove_task, settings.remove_task_files]);

  const remove = useCallback((id: string) => {
    requestRemove(tasks.filter((task) => task.id === id));
  }, [requestRemove, tasks]);

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

  const pauseAll = useCallback(async () => {
    await api.pauseAll();
    await refresh();
  }, [refresh]);

  const resumeAll = useCallback(async () => {
    await api.resumeAll();
    await refresh();
  }, [refresh]);

  // ── 统计 ──
  const stats = useMemo(() => {
    const active = tasks.filter(
      (t) => t.status === "downloading" || t.status === "connecting" || t.status === "merging"
    );
    const totalSpeed = active.reduce((sum, t) => sum + (speeds[t.id] ?? 0), 0);
    const done = tasks.filter((t) => t.status === "completed");
    const doneBytes = done.reduce((sum, t) => sum + t.downloaded, 0);
    const paused = tasks.filter((t) => t.status === "paused" || t.status === "failed");
    return {
      activeCount: active.length,
      totalSpeed,
      doneCount: done.length,
      doneBytes,
      pausedCount: paused.length,
    };
  }, [tasks, speeds]);

  const filtered = useMemo(() => {
    let list = tasks;
    if (filter === "active")
      list = list.filter((t) => t.status !== "completed" && t.status !== "cancelled");
    else if (filter === "completed") list = list.filter((t) => t.status === "completed");

    const q = query.trim().toLowerCase();
    if (q) list = list.filter((t) => t.filename.toLowerCase().includes(q) || t.url.toLowerCase().includes(q));
    return list;
  }, [tasks, filter, query]);

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

        <input
          className="search"
          value={query}
          placeholder="搜索文件名或链接"
          onChange={(e) => setQuery(e.target.value)}
        />

        <Tooltip text="暂停所有进行中的任务">
          <button
            className="btn"
            onClick={pauseAll}
            disabled={stats.activeCount === 0}
          >
            ⏸ 全部暂停
          </button>
        </Tooltip>
        <Tooltip text="继续所有已暂停或失败的任务">
          <button
            className="btn"
            onClick={resumeAll}
            disabled={stats.pausedCount === 0}
          >
            ▶ 全部继续
          </button>
        </Tooltip>
        <Tooltip text="新建下载任务，可一次粘贴多个链接">
          <button className="btn btn-primary" onClick={() => { void openNewDownload(); }}>
            + 新建下载
          </button>
        </Tooltip>
        <Tooltip text="设置">
          <button className="btn settings-entry" aria-label="设置" onClick={() => setShowSettings(true)}>
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="m9.5 3-.6 2.4-1.3.8-2.4-.7-2.5 4.3 1.8 1.7v1.5l-1.8 1.7 2.5 4.3 2.4-.7 1.3.8.6 2.4h5l.6-2.4 1.3-.8 2.4.7 2.5-4.3-1.8-1.7v-1.5l1.8-1.7-2.5-4.3-2.4.7-1.3-.8L14.5 3Z" />
              <circle cx="12" cy="12" r="3" />
            </svg>
          </button>
        </Tooltip>
      </div>

      <div className="content">
        {filtered.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">⬇</div>
            <div>{query ? "没有匹配的任务" : "还没有下载任务"}</div>
            <div style={{ marginTop: 8, fontSize: 12 }}>
              点击右上角「新建下载」开始，可一次粘贴多个链接；复制链接后也会自动识别
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
        <span>
          连接数 {settings.connections} · 并发上限 {settings.max_concurrent}
          {settings.speed_limit > 0 && ` · 限速 ${formatBytes(settings.speed_limit)}/s`}
          {settings.proxy_mode === "none" && " · 直连"}
          {settings.proxy_mode === "custom" && " · 自定义代理"}
          {appInfo && ` · v${appInfo.version}`}
        </span>
        {tasks.some((t) => t.status === "completed") && (
          <Tooltip text="移除列表中全部已完成的任务">
            <button
              className="btn btn-icon"
              disabled={removing}
              onClick={() => requestRemove(tasks.filter((task) => task.status === "completed"))}
            >
              清空已完成
            </button>
          </Tooltip>
        )}
      </div>

      {showNew && (
        <NewDownloadDialog
          settings={settings}
          defaultUrl={clipboardUrl}
          onClose={() => setShowNew(false)}
          onSubmit={startDownloads}
        />
      )}

      {showSettings && (
        <SettingsPanel
          settings={settings}
          onClose={() => setShowSettings(false)}
          onSaved={setSettings}
        />
      )}
      {removeRequest && (
        <RemoveTaskDialog
          tasks={removeRequest}
          initialDeleteFiles={settings.remove_task_files}
          busy={removing}
          error={removeError}
          onClose={() => { if (!removing) setRemoveRequest(null); }}
          onConfirm={(deleteFiles, remember) => { void performRemove(removeRequest, deleteFiles, remember); }}
        />
      )}
    </div>
  );
}
