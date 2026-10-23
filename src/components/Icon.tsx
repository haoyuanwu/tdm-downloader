import type { CSSProperties } from "react";

const paths = {
  download: "M12 3v12m-5-5 5 5 5-5M4 16v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4",
  grid: "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
  activity: "M3 12h4l3-8 4 16 3-8h4",
  check: "m5 12 4 4L19 6",
  pause: "M8 5v14M16 5v14",
  play: "m8 5 11 7-11 7Z",
  close: "m6 6 12 12M18 6 6 18",
  plus: "M12 5v14M5 12h14",
  search: "M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
  settings: "m9.5 3-.6 2.4-1.3.8-2.4-.7-2.5 4.3 1.8 1.7v1.5l-1.8 1.7 2.5 4.3 2.4-.7 1.3.8.6 2.4h5l.6-2.4 1.3-.8 2.4.7 2.5-4.3-1.8-1.7v-1.5l1.8-1.7-2.5-4.3-2.4.7-1.3-.8L14.5 3ZM15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0",
  folder: "M3 7V5a1 1 0 0 1 1-1h5l2 3h9a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7Z",
  file: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Zm0 0v6h6M8 13h8M8 17h5",
  link: "m10 13 4-4M8 16l-1 1a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0m2 1 1-1a4 4 0 0 1 6 6l-4 4a4 4 0 0 1-6 0",
  trash: "M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7",
  alert: "M12 3 2 21h20ZM12 9v5M12 17v.1",
  arrow: "M5 12h14m-5-5 5 5-5 5",
  external: "M14 3h7v7m0-7L10 14M10 3H4a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h16a1 1 0 0 0 1-1v-6",
  chevron: "m6 9 6 6 6-6",
  monitor: "M3 3h18v14H3ZM8 21h8M12 17v4",
  power: "M12 2v10M6 5a9 9 0 1 0 12 0",
  globe: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M3 12h18M12 3c4 4 4 14 0 18-4-4-4-14 0-18",
  clock: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M12 7v5l3 2",
} as const;
export type IconName = keyof typeof paths;

export function Icon({ name, size = 20, style }: { name: IconName; size?: number; style?: CSSProperties }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={style}><path d={paths[name]} /></svg>;
}
