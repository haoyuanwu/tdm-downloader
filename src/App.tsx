import { useCallback, useEffect, useMemo, useState } from "react";
import { api, onNewTask, onProgress, onTasks, onShowDownloads } from "./api";
import { BrowserDownloadDialog } from "./components/BrowserDownloadDialog";
import { NewDownloadDialog } from "./components/NewDownloadDialog";
import { SettingsPanel } from "./components/SettingsPanel";
import { TaskCard } from "./components/TaskCard";
import { RemoveTaskDialog } from "./components/RemoveTaskDialog";
import { Tooltip } from "./components/Tooltip";
import { Icon } from "./components/Icon";
import type { IconName } from "./components/Icon";
import type { AppInfo, DownloadTask, NewTaskRequest, Settings } from "./types";
import { DEFAULT_SETTINGS, formatBytes, formatSpeed } from "./types";
import { useTheme } from "./useTheme";

type Filter = "all" | "active" | "paused" | "completed" | "failed";
const FILTERS: { id: Filter; label: string; icon: IconName }[] = [
  { id: "all", label: "全部任务", icon: "grid" },
  { id: "active", label: "进行中", icon: "download" },
  { id: "paused", label: "已暂停", icon: "pause" },
  { id: "completed", label: "已完成", icon: "check" },
  { id: "failed", label: "下载失败", icon: "alert" },
];
const running = (task: DownloadTask) => ["pending", "connecting", "downloading", "merging"].includes(task.status);

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
      list = list.filter(running);
    else if (filter !== "all") list = list.filter((t) => t.status === filter);

    const q = query.trim().toLowerCase();
    if (q) list = list.filter((t) => t.filename.toLowerCase().includes(q) || t.url.toLowerCase().includes(q));
    return list;
  }, [tasks, filter, query]);

  const counts: Record<Filter, number> = {
    all: tasks.length,
    active: tasks.filter(running).length,
    paused: tasks.filter((task) => task.status === "paused").length,
    completed: stats.doneCount,
    failed: tasks.filter((task) => task.status === "failed").length,
  };
  const currentLabel = FILTERS.find((item) => item.id === filter)!.label;

  return (
    <div className="app">
      <aside className="sidebar" aria-label="主导航">
        <div className="brand"><span className="brand-mark"><Icon name="download" size={24} /></span><div><strong>TDM</strong><span>Downloader</span></div></div>
        <div className="sidebar-caption">下载空间</div>
        <nav className="sidebar-nav" aria-label="任务分类">
          {FILTERS.map((item) => <button key={item.id} className={`nav-item ${filter === item.id ? "is-active" : ""}`} aria-current={filter === item.id ? "page" : undefined} onClick={() => { setFilter(item.id); setQuery(""); }}>
            <Icon name={item.icon} size={19} /><span>{item.label}</span><b>{counts[item.id]}</b>
          </button>)}
        </nav>
        <div className="sidebar-bottom">
          <div className="sidebar-tip"><Icon name="link" size={18} /><p>复制下载链接<span>菜单栏会为你发现新的下载</span></p></div>
          <button className="nav-item settings-nav-entry" aria-label="设置" onClick={() => setShowSettings(true)}><Icon name="settings" size={24} /><span>设置</span><Icon name="chevron" size={15} /></button>
          <div className="sidebar-version">TDM Downloader <span>{appInfo ? `v${appInfo.version}` : ""}</span></div>
        </div>
      </aside>
      <main className="workspace">
        <header className="workspace-header">
          <div><div className="eyebrow">你的文件，有序抵达</div><h1>{currentLabel}<span>{counts[filter]}</span></h1></div>
          <button className="btn btn-primary new-download-button" onClick={() => { void openNewDownload(); }}><Icon name="plus" size={19} />新建下载</button>
        </header>
        <section className="overview" aria-label="下载概览">
          <div className="overview-card"><span className="overview-icon"><Icon name="activity" /></span><div><span>当前速度</span><strong>{formatSpeed(stats.totalSpeed)}</strong></div><small>实时传输</small></div>
          <div className="overview-card"><span className="overview-icon"><Icon name="download" /></span><div><span>进行中</span><strong>{counts.active}<em>个任务</em></strong></div><small>最多 {settings.max_concurrent} 个并发</small></div>
          <div className="overview-card"><span className="overview-icon success"><Icon name="check" /></span><div><span>已完成</span><strong>{stats.doneCount}<em>个文件</em></strong></div><small>{formatBytes(stats.doneBytes)} 已下载</small></div>
        </section>
        <div className="task-toolbar">
          <div className="search-wrap"><Icon name="search" size={17} /><input className="search" aria-label="搜索任务" value={query} placeholder="搜索文件名或下载链接" onChange={(event) => setQuery(event.target.value)} />{query && <button className="plain-icon" aria-label="清空搜索" onClick={() => setQuery("")}><Icon name="close" size={16} /></button>}</div>
          <div className="toolbar-actions">
            <Tooltip text="暂停所有进行中的任务"><button className="btn btn-quiet" onClick={pauseAll} disabled={stats.activeCount === 0}><Icon name="pause" size={16} /><span>全部暂停</span></button></Tooltip>
            <Tooltip text="继续所有已暂停或失败的任务"><button className="btn btn-quiet" onClick={resumeAll} disabled={stats.pausedCount === 0}><Icon name="play" size={16} /><span>全部继续</span></button></Tooltip>
          </div>
        </div>
        <div className="content">
          {filtered.length === 0 ? (
            <div className="empty">
              <div className="empty-illustration"><div className="empty-orbit" /><div className="empty-file"><Icon name={query ? "search" : "download"} size={36} /></div><span className="empty-mini"><Icon name="plus" size={16} /></span></div>
              <h2>{query ? "没有找到匹配的任务" : filter === "all" ? "准备好，下一个文件" : `暂无${currentLabel}任务`}</h2>
              <p>{query ? "试试其他文件名，或粘贴链接新建下载。" : filter === "all" ? "粘贴一个链接，开始下载。也支持一次添加多个文件。" : "任务状态发生变化后，会显示在这里。"}</p>
              <button className="btn" onClick={() => query ? setQuery("") : void openNewDownload()}><Icon name={query ? "close" : "plus"} size={17} />{query ? "清空搜索" : "添加下载链接"}</button>
              {!query && filter === "all" && <span className="empty-hint"><Icon name="link" size={14} />复制链接也能从菜单栏快速添加</span>}
            </div>
          ) : <div className="task-list" aria-label={currentLabel}>
            {filtered.map((t) => <TaskCard key={t.id} task={t} speed={speeds[t.id] ?? 0} onPause={pause} onResume={resume} onCancel={cancel} onRemove={remove} onOpen={openFile} onOpenDir={openDir} />)}
          </div>}
        </div>
        <footer className="footer"><span className={`connection-dot ${counts.active ? "is-active" : ""}`} /><span>{counts.active ? "下载服务运行中" : "已就绪，等待新的下载"}</span><span className="footer-config">{settings.connections} 连接 · {settings.max_concurrent} 并发{settings.speed_limit > 0 ? ` · 限速 ${formatBytes(settings.speed_limit)}/s` : " · 不限速"}</span>
          {stats.doneCount > 0 && <button className="footer-clear" disabled={removing} onClick={() => requestRemove(tasks.filter((task) => task.status === "completed"))}><Icon name="trash" size={14} />清空已完成</button>}
        </footer>
      </main>

      <BrowserDownloadDialog />
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
