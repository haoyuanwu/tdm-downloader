import { useEffect, useRef } from "react";

/** 对话框焦点隔离、Esc 关闭和关闭后焦点恢复。 */
export function useDialog(onClose: () => void, busy = false) {
  const ref = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  const blocked = useRef(busy);
  close.current = onClose;
  blocked.current = busy;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const root = ref.current;
    const controls = () => Array.from(root?.querySelectorAll<HTMLElement>(
      "button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex='0']"
    ) ?? []).filter((item) => item.getClientRects().length > 0);
    (root?.querySelector<HTMLElement>("[data-initial-focus]") ?? controls()[0])?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.key === "Escape" && !blocked.current) {
        event.preventDefault();
        close.current();
      }
      if (event.key !== "Tab") return;
      const items = controls();
      const first = items[0];
      const last = items[items.length - 1];
      if (!items.length) { event.preventDefault(); root?.focus(); }
      else if (event.shiftKey && (document.activeElement === first || !root?.contains(document.activeElement))) {
        event.preventDefault(); last?.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !root?.contains(document.activeElement))) {
        event.preventDefault(); first?.focus();
      }
    };
    window.addEventListener("keydown", keydown);
    return () => { window.removeEventListener("keydown", keydown); if (previous?.isConnected) previous.focus(); };
  }, []);
  return ref;
}
