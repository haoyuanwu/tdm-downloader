import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { DownloadTask } from "../types";
import { formatBytes, formatEta, formatSpeed, STATUS_LABEL } from "../types";
import { Tooltip } from "./Tooltip";
import { Icon } from "./Icon";

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

/** 速度迷你曲线：最近 60 个采样点 */
function Sparkline({ data }: { data: number[] }) {
  if (data.length < 2) return null;
  const max = Math.max(...data, 1);
  const w = 120;
  const h = 26;
  const step = w / (data.length - 1);
  const points = data
    .map((v, i) => `${(i * step).toFixed(1)},${(h - (v / max) * (h - 2) - 1).toFixed(1)}`)
    .join(" ");
  return (
    <svg aria-hidden="true" className="sparkline" width={w} height={h} viewBox={`0 0 ${w} ${h}`}>
      <polyline points={points} fill="none" stroke="var(--accent)" strokeWidth={1.5} />
    </svg>
  );
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
  const pct = task.status === "completed" ? 100 : total > 0 ? Math.max(0, Math.min((task.downloaded / total) * 100, 100)) : 0;
  const isActive =
    task.status === "downloading" ||
    task.status === "connecting" ||
    task.status === "merging";
  const isPausable = isActive;
  const isResumable = task.status === "paused" || task.status === "failed";
  const isDone = task.status === "completed";

  const fillClass =
    task.status === "failed" || task.status === "cancelled"
      ? "progress-fill failed"
      : task.status === "paused"
      ? "progress-fill paused"
      : "progress-fill";

  const eta = isActive && speed > 0 && total > task.downloaded ? (total - task.downloaded) / speed : null;

  // 速度采样历史
  const [history, setHistory] = useState<number[]>([]);
  const lastSpeed = useRef(0);
  useEffect(() => {
    if (!isActive) return;
    if (speed === lastSpeed.current && history.length > 0) return;
    lastSpeed.current = speed;
    setHistory((h) => [...h, speed].slice(-60));
  }, [speed, isActive, history.length]);

  const copyUrl = async () => {
    try {
      await api.copyText(task.url);
    } catch {
      /* 忽略 */
    }
  };

  return (
    <article className={`task-card status-${task.status}`}>
      <div className="task-top">
        <div className="file-type"><Icon name="file" size={24} /><span>{task.filename.split(".").length > 1 ? task.filename.split(".").pop()!.slice(0, 5).toUpperCase() : "FILE"}</span></div>
        <div className="task-identity">
          <div className="task-name" title={task.filename}>{task.filename}</div>
          <div className="task-url" title={task.url}>{task.url}</div>
        </div>

        {isActive && <Sparkline data={history} />}

        <span className={`badge badge-${task.status}`}>
          {STATUS_LABEL[task.status]}
        </span>

        {!task.resumable && isActive && (
          <span className="badge warn" title="服务端不支持 Range 请求，暂停后会重新下载">
            不可续传
          </span>
        )}

        <div className="task-actions">
          {isPausable && (
            <Tooltip text="暂停">
              <button className="btn btn-icon" onClick={() => onPause(task.id)}>
                <Icon name="pause" size={17} />
              </button>
            </Tooltip>
          )}
          {isResumable && (
            <Tooltip text={task.status === "failed" ? "重试" : "继续"}>
              <button className="btn btn-icon" onClick={() => onResume(task.id)}>
                <Icon name="play" size={17} />
              </button>
            </Tooltip>
          )}
          {isActive && (
            <Tooltip text="取消下载">
              <button className="btn btn-icon" onClick={() => onCancel(task.id)}>
                <Icon name="close" size={17} />
              </button>
            </Tooltip>
          )}
          {isDone && (
            <Tooltip text="打开文件">
              <button className="btn btn-icon" onClick={() => onOpen(task)}>
                <Icon name="external" size={17} />
              </button>
            </Tooltip>
          )}
          {!isActive && (
            <Tooltip text="打开所在目录">
              <button className="btn btn-icon" onClick={() => onOpenDir(task)}>
                <Icon name="folder" size={17} />
              </button>
            </Tooltip>
          )}
          <Tooltip text="复制链接">
            <button className="btn btn-icon" onClick={copyUrl}>
              <Icon name="link" size={17} />
            </button>
          </Tooltip>
          <Tooltip text={isActive ? "取消并从列表移除" : "从列表移除"}>
            <button className="btn btn-icon btn-danger" onClick={() => onRemove(task.id)}>
              <Icon name="trash" size={17} />
            </button>
          </Tooltip>
        </div>
      </div>

      <div className="progress-wrap">
        <div className={`progress-bar ${isActive && total === 0 ? "is-indeterminate" : ""}`} role="progressbar" aria-label={`${task.filename} 下载进度`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={total > 0 || isDone ? pct : undefined} aria-valuetext={total > 0 || isDone ? `${pct.toFixed(1)}%` : "文件大小未知"}>
          <div className={fillClass} style={{ width: `${pct}%` }} />
        </div>

        {task.segments.length > 1 && (
          <div className="segments">
            {task.segments.map((s) => {
              const st =
                s.end >= s.start && s.end !== Number.MAX_SAFE_INTEGER ? s.end - s.start + 1 : 0;
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
            {total > 0 || isDone ? `${pct.toFixed(1)}%` : "大小未知"}
            {task.connections > 1 && ` · ${task.connections} 连接`}
            {task.speed_limit > 0 && ` · 限速 ${formatBytes(task.speed_limit)}/s`}
          </span>
          {isActive && <span>{formatSpeed(speed)}</span>}
          {isActive && <span>剩余 {formatEta(eta ? Math.round(eta) : null)}</span>}
        </div>

        {task.error && <div className="error-text" role="alert"><Icon name="alert" size={15} />{task.error}</div>}
      </div>
    </article>
  );
}
