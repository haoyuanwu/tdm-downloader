import { useEffect, useRef, useState } from "react";
import type { DownloadTask } from "../types";

interface Props {
  tasks: DownloadTask[];
  initialDeleteFiles: boolean;
  busy: boolean;
  error: string;
  onConfirm: (deleteFiles: boolean, remember: boolean) => void;
  onClose: () => void;
}

export function RemoveTaskDialog({ tasks, initialDeleteFiles, busy, error, onConfirm, onClose }: Props) {
  const [deleteFiles, setDeleteFiles] = useState(initialDeleteFiles);
  const [remember, setRemember] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    cancel.current?.focus();
    return () => previous?.focus();
  }, []);

  return (
    <div className="modal-overlay" onClick={() => { if (!busy) onClose(); }}>
      <div ref={dialog} className="modal remove-dialog" role="dialog" aria-modal="true" aria-labelledby="remove-task-title"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape" && !busy) { event.preventDefault(); event.stopPropagation(); onClose(); }
          if (event.key === "Tab") {
            const controls = Array.from(dialog.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)") ?? []);
            const first = controls[0];
            const last = controls[controls.length - 1];
            if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
            if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
          }
        }}>
        <h2 id="remove-task-title">{tasks.length === 1 ? "移除下载任务" : `移除 ${tasks.length} 个下载任务`}</h2>
        <p className="remove-question">是否同时删除本地文件？</p>
        <div className="remove-files">
          {tasks.slice(0, 3).map((task) => <div key={task.id} title={task.save_path}>{task.filename}</div>)}
          {tasks.length > 3 && <div>另有 {tasks.length - 3} 个文件</div>}
        </div>
        <fieldset className="remove-options" disabled={busy}>
          <legend className="sr-only">本地文件处理方式</legend>
          <label className={deleteFiles ? "" : "is-selected"}>
            <input type="radio" name="remove-files" checked={!deleteFiles} onChange={() => setDeleteFiles(false)} />
            <span><strong>保留本地文件</strong><small>仅从列表移除任务，保留已下载的数据</small></span>
          </label>
          <label className={deleteFiles ? "is-selected" : ""}>
            <input type="radio" name="remove-files" checked={deleteFiles} onChange={() => setDeleteFiles(true)} />
            <span><strong>同时删除本地文件</strong><small>下载文件和未完成分片将移到废纸篓</small></span>
          </label>
        </fieldset>
        <label className="checkbox-row remove-remember">
          <input type="checkbox" checked={remember} disabled={busy} onChange={(event) => setRemember(event.target.checked)} />
          下次不再询问，记住本次选择
        </label>
        <p className="hint">进行中的下载会停止。可在设置中修改此选择。</p>
        {error && <p className="error-text" role="alert">{error}</p>}
        <div className="modal-footer">
          <button ref={cancel} className="btn" disabled={busy} onClick={onClose}>取消</button>
          <button className={`btn ${deleteFiles ? "btn-danger" : "btn-primary"}`} disabled={busy} onClick={() => onConfirm(deleteFiles, remember)}>
            {busy ? "正在移除…" : deleteFiles ? "删除任务和文件" : "仅移除任务"}
          </button>
        </div>
      </div>
    </div>
  );
}
