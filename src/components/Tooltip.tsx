import { cloneElement, useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import { createPortal } from "react-dom";

interface TooltipProps {
  /** 提示文字 */
  text: string;
  /** 被包裹的元素（通常是按钮），需能接收鼠标事件 */
  children: ReactElement;
  /** 优先显示的方向，空间不足时会自动翻转 */
  placement?: "top" | "bottom";
  /** 悬停多久后显示（毫秒） */
  delay?: number;
}

interface Pos {
  x: number;
  y: number;
  place: "top" | "bottom";
}

/**
 * 轻量提示气泡：悬停 delay 毫秒后在目标上方（空间不足则下方）显示说明文字。
 *
 * 用 portal 渲染到 body + fixed 定位，避免被任务列表的滚动容器裁剪；
 * 相比原生 title 属性，显示更快、样式跟随深浅主题。
 */
export function Tooltip({ text, children, placement = "top", delay = 300 }: TooltipProps) {
  const [pos, setPos] = useState<Pos | null>(null);
  const timer = useRef<number | null>(null);
  const anchorRef = useRef<HTMLSpanElement>(null);

  const clear = () => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  };

  useEffect(() => clear, []);

  const show = () => {
    const el = anchorRef.current;
    if (!el) return;
    clear();
    timer.current = window.setTimeout(() => {
      const r = el.getBoundingClientRect();
      // 上方空间不足（比如顶栏按钮）翻到下方；下方放不下则翻回上方
      let place = placement;
      if (place === "top" && r.top < 44) place = "bottom";
      else if (place === "bottom" && r.bottom + 44 > window.innerHeight) place = "top";
      setPos({
        x: r.left + r.width / 2,
        y: place === "top" ? r.top - 8 : r.bottom + 8,
        place,
      });
    }, delay);
  };

  const hide = () => {
    clear();
    setPos(null);
  };

  // 事件挂在外层锚点上：原生 disabled 按钮不派发鼠标事件，挂在子元素上会导致禁用时无提示
  const trigger = cloneElement(children, {
    "aria-label": children.props["aria-label"] ?? text,
  } as React.HTMLAttributes<HTMLElement>);

  return (
    <>
      <span
        ref={anchorRef}
        className="tooltip-anchor"
        onMouseEnter={() => show()}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
        onKeyDown={(event) => { if (event.key === "Escape") hide(); }}
        onMouseDown={hide} /* 点击后立即收起，避免气泡挡住后续操作 */
      >
        {trigger}
      </span>
      {pos &&
        text &&
        createPortal(
          <div
            className={`tooltip tooltip-${pos.place}`}
            style={{ left: pos.x, top: pos.y }}
            role="tooltip"
          >
            {text}
          </div>,
          document.body
        )}
    </>
  );
}
