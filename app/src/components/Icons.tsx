// 线性单色图标，1.5px 线宽，颜色跟随文字（docs/08 §6）。
import type { ProgramKind } from "../backend/types";

const paths: Record<string, string> = {
  moon: "M20 14.5A8.5 8.5 0 1 1 9.5 4a7 7 0 0 0 10.5 10.5Z M9 12l2 2-2 2 M13 16h3",
  pwsh: "M5 8l4 4-4 4 M11 16h7",
  claude: "M12 3v18 M3 12h18 M5.6 5.6l12.8 12.8 M18.4 5.6 5.6 18.4",
  codex: "M9 4c-2 0-3 1-3 3v2c0 1.5-1 2.5-2 3 1 .5 2 1.5 2 3v2c0 2 1 3 3 3 M15 4c2 0 3 1 3 3v2c0 1.5 1 2.5 2 3-1 .5-2 1.5-2 3v2c0 2-1 3-3 3",
  ssh: "M5.5 4h13A1.5 1.5 0 0 1 20 5.5v4a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 9.5v-4A1.5 1.5 0 0 1 5.5 4Z M5.5 13h13a1.5 1.5 0 0 1 1.5 1.5v4a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5v-4A1.5 1.5 0 0 1 5.5 13Z M8 7.5h.01 M8 16.5h.01",
  git: "M6 4a2 2 0 1 1 0 4 2 2 0 0 1 0-4Z M6 16a2 2 0 1 1 0 4 2 2 0 0 1 0-4Z M18 6a2 2 0 1 1 0 4 2 2 0 0 1 0-4Z M6 8v8 M18 10c0 4-6 3-11 6.5",
  editor: "M4 20h4L19 9l-4-4L4 16v4Z M13.5 6.5l4 4",
  node: "M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3Z M12 12v9 M4 7.5l8 4.5 8-4.5",
  python: "M12 4c-4 0-4 2-4 3v2h4 M8 9H6c-1.5 0-2 1.5-2 3s.5 3 2 3h2 M12 20c4 0 4-2 4-3v-2h-4 M16 15h2c1.5 0 2-1.5 2-3s-.5-3-2-3h-2 M8 15v2c0 1 0 3 4 3 M16 9V7c0-1 0-3-4-3",
  build: "M14.7 6.3a4 4 0 0 0-5.4 5.2L4 16.8V20h3.2l5.3-5.3a4 4 0 0 0 5.2-5.4l-2.5 2.5-2.5-2.5 2.5-2.5Z",
  other: "M5 5h14v14H5Z M9 9h6v6H9Z",
  plus: "M12 5v14 M5 12h14",
  search: "M11 5a6 6 0 1 1 0 12 6 6 0 0 1 0-12Z M20 20l-4.2-4.2",
  gear: "M12 9a3 3 0 1 1 0 6 3 3 0 0 1 0-6Z M12 3v2 M12 19v2 M3 12h2 M19 12h2 M5.6 5.6 7 7 M17 17l1.4 1.4 M5.6 18.4 7 17 M17 7l1.4-1.4",
  palette: "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16Z M12 4a8 8 0 0 1 0 16Z",
  menu: "M4 7h16 M4 12h16 M4 17h16",
  layout: "M4 5h16v14H4Z M9 5v14",
  close: "M6 6l12 12 M18 6 6 18",
};

export function Icon({ name, size = 16, className }: { name: string; size?: number; className?: string }) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={paths[name] ?? paths.other} />
    </svg>
  );
}

export const kindIcon = (k: ProgramKind | undefined) => k ?? "pwsh";
