import type { DownloadTask } from "../types";
import { formatBytes, formatEta, formatSpeed, STATUS_COLOR, STATUS_LABEL } from "../types";

interface Props {
  task: DownloadTask;
  speed: number;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onRemove: (id: string) => void;
  onOpen: (task: DownloadTask) => void;
  onOpenDir: (task: DownloadTask) => void;
}

export function TaskCard({
  task,
  speed,
  onPause,
  onResume,
  onCancel,
  onRemove,
  onOpen,
  onOpenDir,
}: Props) {
  const total = task.total_size || 0;
  const pct = total > 0 ? Math.min((task.downloaded / total) * 100, 100) : 0;
  const isActive = task.status === "downloading" || task.status === "connecting" || task.status === "merging";
  const isPausable = isActive;
  const isResumable = task.status === "paused" || task.status === "failed";
  const isDone = task.status === "completed";

  const fillClass =
    task.status === "failed" || task.status === "cancelled"
      ? "progress-fill failed"
      : task.status === "paused"
      ? "progress-fill paused"
      : "progress-fill";

  const eta =
    isActive && speed > 0 && total > task.downloaded
      ? (total - task.downloaded) / speed
      : null;

  return (
    <div className="task-card">
      <div className="task-top">
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="task-name">{task.filename}</div>
          <div className="task-url">{task.url}</div>
        </div>

        <span
          className="badge"
          style={{
            background: `${STATUS_COLOR[task.status]}22`,
            color: STATUS_COLOR[task.status],
          }}
        >
          {STATUS_LABEL[task.status]}
        </span>

        <div className="task-actions">
          {isPausable && (
            <button className="btn btn-icon" onClick={() => onPause(task.id)} title="暂停">
              ⏸
            </button>
          )}
          {isResumable && (
            <button className="btn btn-icon" onClick={() => onResume(task.id)} title="继续">
              ▶
            </button>
          )}
          {isActive && (
            <button className="btn btn-icon" onClick={() => onCancel(task.id)} title="取消">
              ✕
            </button>
          )}
          {isDone && (
            <button className="btn btn-icon" onClick={() => onOpen(task)} title="打开文件">
              📂
            </button>
          )}
          {!isActive && (
            <button className="btn btn-icon" onClick={() => onOpenDir(task)} title="打开所在目录">
              📁
            </button>
          )}
          <button
            className="btn btn-icon btn-danger"
            onClick={() => onRemove(task.id)}
            title="从列表移除"
          >
            🗑
          </button>
        </div>
      </div>

      <div className="progress-wrap">
        <div className="progress-bar">
          <div className={fillClass} style={{ width: `${pct}%` }} />
        </div>

        {task.segments.length > 1 && (
          <div className="segments">
            {task.segments.map((s) => {
              const st = s.end >= s.start && s.end !== Number.MAX_SAFE_INTEGER
                ? s.end - s.start + 1
                : 0;
              const p = st > 0 ? Math.min((s.downloaded / st) * 100, 100) : 0;
              return (
                <div className="seg" key={s.index} title={`分片 ${s.index + 1}`}>
                  <div className="seg-fill" style={{ width: `${p}%` }} />
                </div>
              );
            })}
          </div>
        )}

        <div className="progress-meta">
          <span>
            <b>{formatBytes(task.downloaded)}</b>
            {total > 0 ? ` / ${formatBytes(total)}` : " / 未知"}
          </span>
          <span>
            {pct.toFixed(1)}%
            {task.connections > 1 && ` · ${task.connections} 连接`}
          </span>
          {isActive && <span>{formatSpeed(speed)}</span>}
          {isActive && <span>剩余 {formatEta(eta ? Math.round(eta) : null)}</span>}
        </div>

        {task.error && <div className="error-text">⚠ {task.error}</div>}
      </div>
    </div>
  );
}
