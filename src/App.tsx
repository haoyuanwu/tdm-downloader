import { useCallback, useEffect, useMemo, useState } from "react";
import { api, onNewTask, onProgress, onTasks } from "./api";
import { NewDownloadDialog } from "./components/NewDownloadDialog";
import { SettingsPanel } from "./components/SettingsPanel";
import { TaskCard } from "./components/TaskCard";
import { Tooltip } from "./components/Tooltip";
import type { AppInfo, DownloadTask, NewTaskRequest, Settings } from "./types";
import { DEFAULT_SETTINGS, formatBytes, formatSpeed } from "./types";

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
  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
  }, [settings.theme]);

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

  const remove = useCallback(
    async (id: string) => {
      await api.removeTask(id, false);
      await refresh();
    },
    [refresh]
  );

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
          <button className="btn" onClick={() => setShowSettings(true)}>
            ⚙
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
          <Tooltip text="清空列表中已完成的任务（不删除已下载的文件）">
            <button
              className="btn btn-icon"
              onClick={async () => {
                await api.clearCompleted();
                await refresh();
              }}
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
    </div>
  );
}
