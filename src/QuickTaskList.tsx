import { Icon } from "./components/Icon";
import { useEffect, useRef, useState } from "react";
import { api, onProgress, onTasks } from "./api";
import { RemoveTaskDialog } from "./components/RemoveTaskDialog";
import { isWindows } from "./platform";
import type { DownloadTask } from "./types";
import { formatBytes, formatSpeed, STATUS_LABEL } from "./types";

const running = (task: DownloadTask) => ["pending", "connecting", "downloading", "merging"].includes(task.status);

export default function QuickTaskList() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [menu, setMenu] = useState<{ task: DownloadTask; x: number; y: number } | null>(null);
  const [remove, setRemove] = useState<DownloadTask | null>(null);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [busy, setBusy] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    menuRef.current?.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
    const dismiss = () => setMenu(null);
    window.addEventListener("click", dismiss);
    window.addEventListener("blur", dismiss);
    return () => { window.removeEventListener("click", dismiss); window.removeEventListener("blur", dismiss); };
  }, [menu]);
  useEffect(() => () => { void api.quickTaskInteraction(false); }, []);

  const removeTask = async (task: DownloadTask, files: boolean, remember = false) => {
    setBusy(true); setError("");
    try {
      await api.removeTask(task.id, files);
      setTasks((previous) => previous.filter((item) => item.id !== task.id));
      if (remember) {
        const settings = await api.getSettings();
        await api.saveSettings({ ...settings, confirm_remove_task: false, remove_task_files: files });
      }
      setRemove(null);
      await api.quickTaskInteraction(false);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  const action = async (task: DownloadTask, name: string) => {
    setMenu(null); setError(""); setNotice("");
    try {
      await api.quickTaskInteraction(true);
      if (name === "remove") {
        const settings = await api.getSettings();
        if (settings.confirm_remove_task) { setDeleteFiles(settings.remove_task_files); setRemove(task); return; }
        await removeTask(task, settings.remove_task_files);
      } else if (name === "open") await api.openPath(task.save_path);
      else if (name === "reveal") await api.revealTaskFile(task.id);
      else {
        const path = await api.saveTaskAs(task.id);
        if (path) setNotice(`已另存为：${path}`);
      }
      await api.quickTaskInteraction(false);
    } catch (reason) { setError(String(reason)); await api.quickTaskInteraction(false).catch(() => {}); }
  };

  useEffect(() => {
    let active = true;
    const cleanups: (() => void)[] = [];
    const keep = (unlisten: () => void) => { if (active) cleanups.push(unlisten); else unlisten(); };
    void (async () => {
      await Promise.all([
        onTasks((list) => { if (active) setTasks(list); }).then(keep),
        onProgress((progress) => {
          if (!active) return;
          setSpeeds((previous) => ({ ...previous, [progress.id]: progress.speed }));
          setTasks((previous) => previous.map((task) => {
            if (task.id !== progress.id || ["completed", "cancelled", "failed"].includes(task.status)) return task;
            return { ...task, downloaded: progress.downloaded, total_size: progress.total_size, status: progress.status };
          }));
        }).then(keep),
      ]);
      const list = await api.listTasks();
      if (active) setTasks(list);
    })().catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; cleanups.forEach((unlisten) => unlisten()); };
  }, []);

  const visible = tasks.filter((task) => task.status !== "cancelled")
    .sort((a, b) => Number(running(b)) - Number(running(a)) || b.created_at - a.created_at);
  const count = visible.filter(running).length;

  return (
    <main className="quick-panel quick-downloads" onKeyDown={(event) => { if ((remove || menu) && event.key === "Escape") event.stopPropagation(); }}>
      <div className="quick-heading">
        <div className="quick-mark" aria-hidden="true"><Icon name="download" size={19} /></div>
        <div>
          <strong>下载列表</strong>
          <span>{count ? `${count} 个任务进行中` : "没有正在下载的任务"}</span>
        </div>
        <button className="quick-close" aria-label="关闭" onClick={() => void api.hideQuickWindow()}><Icon name="close" size={18} /></button>
      </div>
      <div className="quick-task-list" aria-label="下载任务">
        {error && !remove && <p className="quick-error" role="alert">{error}</p>}
        {notice && <p className="hint" role="status">{notice}</p>}
        {!error && !visible.length && <p className="quick-empty">还没有下载任务</p>}
        {visible.map((task) => {
          const known = task.total_size > 0 && task.total_size < Number.MAX_SAFE_INTEGER;
          const percent = task.status === "completed" ? 100 : known ? Math.min(100, Math.max(0, task.downloaded / task.total_size * 100)) : undefined;
          const speed = task.status === "downloading" ? (speeds[task.id] ? formatSpeed(speeds[task.id]) : "0 B/s") : "—";
          return (
            <article key={task.id} className={`quick-task quick-task-${task.status}`} tabIndex={0}
              aria-label={`${task.filename}，右键打开操作菜单`}
              onContextMenu={(event) => { event.preventDefault(); setMenu({ task, x: Math.max(8, Math.min(event.clientX, window.innerWidth - 196)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 174)) }); }}
              onKeyDown={(event) => { if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault(); const rect = event.currentTarget.getBoundingClientRect(); setMenu({ task, x: Math.min(rect.left + 12, window.innerWidth - 196), y: Math.max(8, Math.min(rect.top + 16, window.innerHeight - 174)) }); } }}>
              <div className="quick-task-title">
                <strong title={task.filename}>{task.filename}</strong>
                <span>{percent === undefined ? "大小未知" : `${percent.toFixed(0)}%`}</span>
              </div>
              <div className={`quick-task-track${percent === undefined && running(task) ? " is-indeterminate" : ""}`}
                role="progressbar" aria-label={`${task.filename} 下载进度`} aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={percent} aria-valuetext={percent === undefined ? "大小未知" : `${percent.toFixed(0)}%`}>
                <div style={{ width: `${percent ?? 0}%` }} />
              </div>
              <div className="quick-task-meta">
                <span title={task.error || undefined}>{STATUS_LABEL[task.status]} · {formatBytes(task.downloaded)}{known ? ` / ${formatBytes(task.total_size)}` : ""}</span>
                <span>{speed}</span>
              </div>
            </article>
          );
        })}
      </div>
      {menu && <div ref={menuRef} role="menu" aria-label="下载任务操作" className="quick-task-menu" style={{ left: menu.x, top: menu.y }}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") { event.preventDefault(); setMenu(null); }
          if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
            event.preventDefault(); const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") || []);
            const current = items.indexOf(document.activeElement as HTMLButtonElement);
            items[event.key === "Home" ? 0 : event.key === "End" ? items.length - 1 : (current + (event.key === "ArrowUp" ? -1 : 1) + items.length) % items.length]?.focus();
          }
        }}>
        <button role="menuitem" disabled={menu.task.status !== "completed"} onClick={() => void action(menu.task, "open")}>打开</button>
        <button role="menuitem" onClick={() => void action(menu.task, "reveal")}>{isWindows ? "在资源管理器中显示" : "在 Finder 中显示"}</button>
        <button role="menuitem" disabled={menu.task.status !== "completed"} onClick={() => void action(menu.task, "save")}>另存为…</button>
        <button role="menuitem" className="is-danger" onClick={() => void action(menu.task, "remove")}>删除…</button>
      </div>}
      {remove && <div className="quick-remove-dialog"><RemoveTaskDialog tasks={[remove]} initialDeleteFiles={deleteFiles} busy={busy} error={error}
        onClose={() => { if (!busy) { setRemove(null); setError(""); void api.quickTaskInteraction(false); } }}
        onConfirm={(files, remember) => void removeTask(remove, files, remember)} /></div>}
      <div className="quick-footer">
        <button onClick={() => void api.showQuickInput()}>＋ 新建下载</button>
        <button onClick={() => void api.showMainWindow()}>管理任务 ↗</button>
      </div>
    </main>
  );
}
