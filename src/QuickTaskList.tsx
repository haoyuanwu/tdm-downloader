import { Icon } from "./components/Icon";
import { useEffect, useState } from "react";
import { api, onProgress, onTasks } from "./api";
import type { DownloadTask } from "./types";
import { formatBytes, formatSpeed, STATUS_LABEL } from "./types";

const running = (task: DownloadTask) => ["pending", "connecting", "downloading", "merging"].includes(task.status);

export default function QuickTaskList() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [error, setError] = useState("");

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
    <main className="quick-panel quick-downloads">
      <div className="quick-heading">
        <div className="quick-mark" aria-hidden="true"><Icon name="download" size={19} /></div>
        <div>
          <strong>下载列表</strong>
          <span>{count ? `${count} 个任务进行中` : "没有正在下载的任务"}</span>
        </div>
        <button className="quick-close" aria-label="关闭" onClick={() => void api.hideQuickWindow()}><Icon name="close" size={18} /></button>
      </div>
      <div className="quick-task-list" aria-label="下载任务">
        {error && <p className="quick-error" role="alert">{error}</p>}
        {!error && !visible.length && <p className="quick-empty">还没有下载任务</p>}
        {visible.map((task) => {
          const known = task.total_size > 0 && task.total_size < Number.MAX_SAFE_INTEGER;
          const percent = task.status === "completed" ? 100 : known ? Math.min(100, Math.max(0, task.downloaded / task.total_size * 100)) : undefined;
          const speed = task.status === "downloading" ? (speeds[task.id] ? formatSpeed(speeds[task.id]) : "0 B/s") : "—";
          return (
            <article key={task.id} className={`quick-task quick-task-${task.status}`}>
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
      <div className="quick-footer">
        <button onClick={() => void api.showQuickInput()}>＋ 新建下载</button>
        <button onClick={() => void api.showMainWindow()}>管理任务 ↗</button>
      </div>
    </main>
  );
}
