import { useEffect } from "react";
import type { ThemeMode } from "./types";

/** 将系统外观解析成实际配色，并在 macOS 切换外观时同步更新。 */
export function useTheme(theme: ThemeMode) {
  useEffect(() => {
    const appearance = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = theme === "system"
        ? (appearance.matches ? "dark" : "light")
        : theme;
    };
    apply();
    if (theme !== "system") return;
    appearance.addEventListener("change", apply);
    return () => appearance.removeEventListener("change", apply);
  }, [theme]);
}
