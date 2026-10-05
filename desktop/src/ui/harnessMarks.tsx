/**
 * One mark per harness (ide/06, ide/07): the glyph a workstream row, a
 * session row, a terminal tab, the launcher and the harness list wear so
 * *which* harness is here reads at a glance. The marks are the harnesses'
 * **own**, from two free sets, each a 24×24 single path drawn in the text
 * colour (or the brand colour under `tone="brand"`):
 *
 * | mark | source | licence |
 * |---|---|---|
 * | Claude Code | LobeHub icons, `claudecode.svg` | MIT — see NOTICES.md |
 * | Codex | LobeHub icons, `codex.svg` | MIT |
 * | pi | pi's own logo (pi.dev), the geometry LobeHub's `pi.svg` carries | MIT |
 * | Goose | LobeHub icons, `goose.svg` | MIT |
 * | GitHub Copilot CLI | LobeHub icons, `githubcopilot.svg` | MIT |
 * | Grok Build | LobeHub icons, `grok.svg` | MIT |
 * | Gemini CLI | LobeHub icons, `gemini.svg` | MIT |
 * | OpenCode | Simple Icons, `opencode.svg` | CC0 |
 * | Cursor | Simple Icons, `cursor.svg` | CC0 |
 * | Oh My Pi | hand-drawn here — OMP publishes no mark, only a hero image | — |
 *
 * The paths are inline: the desktop bundles no image files. An ACP agent
 * and a custom descriptor keep the kit's own glyphs. Every mark takes the
 * props a lucide icon takes (`size`, `className`, `aria-hidden`), so a call
 * site holds a `Glyph` and does not care which kind it got.
 */

import type { ComponentType, ReactElement, SVGProps } from "react";
import { Cable, Wand2 } from "lucide-react";
import { markIdOf, type MarkId } from "./harnessMarkModel.mjs";
import { ICON } from "./icons";

export interface MarkProps {
  size?: number | string;
  className?: string;
  title?: string;
  "aria-hidden"?: boolean | "true" | "false";
  /** The harness's own colour, or the text colour around it. */
  tone?: "current" | "brand";
}

export type Mark = ComponentType<MarkProps>;

/** The brand colour each mark wears under `tone="brand"`. */
const BRAND: Record<MarkId, string> = {
  "claude-code": "#D97757",
  codex: "currentColor",
  pi: "#3B82F6",
  omp: "#8B5CF6",
  opencode: "#10B981",
  copilot: "currentColor",
  grok: "currentColor",
  gemini: "currentColor",
  goose: "#F59E0B",
  "cursor-agent": "currentColor",
  acp: "currentColor",
  custom: "currentColor",
};

/** A filled mark (the sets' paths), or a stroked one (the hand-drawn OMP). */
function Svg({
  size = 14,
  className,
  title,
  tone,
  id,
  stroked = false,
  children,
  ...rest
}: MarkProps & { id: MarkId; stroked?: boolean; children: ReactElement | ReactElement[] } & Omit<SVGProps<SVGSVGElement>, "color">) {
  const color = tone === "brand" ? BRAND[id] : "currentColor";
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={stroked ? "none" : color}
      fillRule={stroked ? undefined : "evenodd"}
      stroke={stroked ? color : "none"}
      strokeWidth={stroked ? 2 : undefined}
      strokeLinecap={stroked ? "round" : undefined}
      strokeLinejoin={stroked ? "round" : undefined}
      className={className}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : rest["aria-hidden"] ?? true}
    >
      {title && <title>{title}</title>}
      {children}
    </svg>
  );
}

/** Claude Code — LobeHub icons (MIT). */
function ClaudeMark(p: MarkProps) {
  return (
    <Svg {...p} id="claude-code">
      <path d="M20.998 10.949H24v3.102h-3v3.028h-1.487V20H18v-2.921h-1.487V20H15v-2.921H9V20H7.488v-2.921H6V20H4.487v-2.921H3V14.05H0V10.95h3V5h17.998v5.949zM6 10.949h1.488V8.102H6v2.847zm10.51 0H18V8.102h-1.49v2.847z" />
    </Svg>
  );
}

/** Codex — LobeHub icons (MIT). */
function CodexMark(p: MarkProps) {
  return (
    <Svg {...p} id="codex">
      <path d="M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z" />
    </Svg>
  );
}

/** pi — its own logo (pi.dev; the geometry LobeHub's `pi.svg` carries, MIT). */
function PiMark(p: MarkProps) {
  return (
    <Svg {...p} id="pi">
      <path d="M1 1h16.5v11H12v5.5H6.5V23H1V1zm5.5 5.5V12H12V6.5H6.5z" />
      <path d="M17.5 12H23v11h-5.5V12z" />
    </Svg>
  );
}

/** Oh My Pi: π under a hat — hand-drawn; OMP publishes no mark of its own. */
function OmpMark(p: MarkProps) {
  return (
    <Svg {...p} id="omp" stroked>
      <path d="M9 4.5 12 2.5l3 2" />
      <path d="M5 9h14" />
      <path d="M9 9v10" />
      <path d="M15.5 9c0 3.5-.5 7 1.5 10" />
    </Svg>
  );
}

/** OpenCode — Simple Icons (CC0). */
function OpenCodeMark(p: MarkProps) {
  return (
    <Svg {...p} id="opencode">
      <path d="M22 24H2V0h20zM17 4.8H7v14.4h10z" />
    </Svg>
  );
}

/** GitHub Copilot CLI — LobeHub icons (MIT). */
function CopilotMark(p: MarkProps) {
  return (
    <Svg {...p} id="copilot">
      <path d="M19.245 5.364c1.322 1.36 1.877 3.216 2.11 5.817.622 0 1.2.135 1.592.654l.73.964c.21.278.323.61.323.955v2.62c0 .339-.173.669-.453.868C20.239 19.602 16.157 21.5 12 21.5c-4.6 0-9.205-2.583-11.547-4.258-.28-.2-.452-.53-.453-.868v-2.62c0-.345.113-.679.321-.956l.73-.963c.392-.517.974-.654 1.593-.654l.029-.297c.25-2.446.81-4.213 2.082-5.52 2.461-2.54 5.71-2.851 7.146-2.864h.198c1.436.013 4.685.323 7.146 2.864zm-7.244 4.328c-.284 0-.613.016-.962.05-.123.447-.305.85-.57 1.108-1.05 1.023-2.316 1.18-2.994 1.18-.638 0-1.306-.13-1.851-.464-.516.165-1.012.403-1.044.996a65.882 65.882 0 00-.063 2.884l-.002.48c-.002.563-.005 1.126-.013 1.69.002.326.204.63.51.765 2.482 1.102 4.83 1.657 6.99 1.657 2.156 0 4.504-.555 6.985-1.657a.854.854 0 00.51-.766c.03-1.682.006-3.372-.076-5.053-.031-.596-.528-.83-1.046-.996-.546.333-1.212.464-1.85.464-.677 0-1.942-.157-2.993-1.18-.266-.258-.447-.661-.57-1.108-.32-.032-.64-.049-.96-.05zm-2.525 4.013c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zm5 0c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zM7.635 5.087c-1.05.102-1.935.438-2.385.906-.975 1.037-.765 3.668-.21 4.224.405.394 1.17.657 1.995.657h.09c.649-.013 1.785-.176 2.73-1.11.435-.41.705-1.433.675-2.47-.03-.834-.27-1.52-.63-1.813-.39-.336-1.275-.482-2.265-.394zm6.465.394c-.36.292-.6.98-.63 1.813-.03 1.037.24 2.06.675 2.47.968.957 2.136 1.104 2.776 1.11h.044c.825 0 1.59-.263 1.995-.657.555-.556.765-3.187-.21-4.224-.45-.468-1.335-.804-2.385-.906-.99-.088-1.875.058-2.265.394zM12 7.615c-.24 0-.525.015-.84.044.03.16.045.336.06.526l-.001.159a2.94 2.94 0 01-.014.25c.225-.022.425-.027.612-.028h.366c.187 0 .387.006.612.028-.015-.146-.015-.277-.015-.409.015-.19.03-.365.06-.526a9.29 9.29 0 00-.84-.044z" />
    </Svg>
  );
}

/** Grok Build — LobeHub icons (MIT). */
function GrokMark(p: MarkProps) {
  return (
    <Svg {...p} id="grok">
      <path d="M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815" />
    </Svg>
  );
}

/** Gemini CLI — LobeHub icons (MIT): the four-pointed spark, one even-odd path. */
function GeminiMark(p: MarkProps) {
  return (
    <Svg {...p} id="gemini">
      <path
        fillRule="evenodd"
        d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z"
      />
    </Svg>
  );
}

/** Goose — LobeHub icons (MIT). */
function GooseMark(p: MarkProps) {
  return (
    <Svg {...p} id="goose">
      <path d="M21.595 23.61c1.167-.254 2.405-.944 2.405-.944l-2.167-1.784a12.124 12.124 0 01-2.695-3.131 12.127 12.127 0 00-3.97-4.049l-.794-.462a1.115 1.115 0 01-.488-.815.844.844 0 01.154-.575c.413-.582 2.548-3.115 2.94-3.44.503-.416 1.065-.762 1.586-1.159.074-.056.148-.112.221-.17.003-.002.007-.004.009-.007.167-.131.325-.272.45-.438.453-.524.563-.988.59-1.193-.061-.197-.244-.639-.753-1.148.319.02.705.272 1.056.569.235-.376.481-.773.727-1.171.165-.266-.08-.465-.086-.471h-.001V3.22c-.007-.007-.206-.25-.471-.086-.567.35-1.134.702-1.639 1.021 0 0-.597-.012-1.305.599a2.464 2.464 0 00-.438.45l-.007.009c-.058.072-.114.147-.17.221-.397.521-.743 1.083-1.16 1.587-.323.391-2.857 2.526-3.44 2.94a.842.842 0 01-.574.153 1.115 1.115 0 01-.815-.488l-.462-.794a12.123 12.123 0 00-4.049-3.97 12.133 12.133 0 01-3.13-2.695L1.332 0S.643 1.238.39 2.405c.352.428 1.27 1.49 2.34 2.302C1.58 4.167.73 3.75.06 3.4c-.103.765-.063 1.92.043 2.816.726.317 1.961.806 3.219 1.066-1.006.236-2.11.278-2.961.262.15.554.358 1.119.64 1.688.119.263.25.52.39.77.452.125 2.222.383 3.164.171l-2.51.897a27.776 27.776 0 002.544 2.726c2.031-1.092 2.494-1.241 4.018-2.238-2.467 2.008-3.108 2.828-3.8 3.67l-.483.678c-.25.351-.469.725-.65 1.117-.61 1.31-1.47 4.1-1.47 4.1-.154.486.202.842.674.674 0 0 2.79-.861 4.1-1.47.392-.182.766-.4 1.118-.65l.677-.483c.227-.187.453-.37.701-.586 0 0 1.705 2.02 3.458 3.349l.896-2.511c-.211.942.046 2.712.17 3.163.252.142.509.272.772.392.569.28 1.134.49 1.688.64-.016-.853.026-1.956.261-2.962.26 1.258.75 2.493 1.067 3.219.895.106 2.051.146 2.816.043a73.87 73.87 0 01-1.308-2.67c.811 1.07 1.874 1.988 2.302 2.34h-.001z" />
    </Svg>
  );
}

/** Cursor — Simple Icons (CC0). */
function CursorMark(p: MarkProps) {
  return (
    <Svg {...p} id="cursor-agent">
      <path d="M11.503.131 1.891 5.678a.84.84 0 0 0-.42.726v11.188c0 .3.162.575.42.724l9.609 5.55a1 1 0 0 0 .998 0l9.61-5.55a.84.84 0 0 0 .42-.724V6.404a.84.84 0 0 0-.42-.726L12.497.131a1.01 1.01 0 0 0-.996 0M2.657 6.338h18.55c.263 0 .43.287.297.515L12.23 22.918c-.062.107-.229.064-.229-.06V12.335a.59.59 0 0 0-.295-.51l-9.11-5.257c-.109-.063-.064-.23.061-.23" />
    </Svg>
  );
}

/** An ACP agent: the protocol's plug — any agent speaking it. */
function AcpMark({ size = 14, tone: _tone, ...p }: MarkProps) {
  return <Cable size={size} {...p} />;
}

/** A custom descriptor: the wand the settings panel already shows for one. */
function CustomMark({ size = 14, tone: _tone, ...p }: MarkProps) {
  return <Wand2 size={size} {...p} />;
}

export const HARNESS_MARK: Record<MarkId, Mark> = {
  "claude-code": ClaudeMark,
  codex: CodexMark,
  pi: PiMark,
  omp: OmpMark,
  opencode: OpenCodeMark,
  copilot: CopilotMark,
  grok: GrokMark,
  gemini: GeminiMark,
  goose: GooseMark,
  "cursor-agent": CursorMark,
  acp: AcpMark,
  custom: CustomMark,
};

/** The mark for a harness id, or the generic terminal glyph for one nothing here knows. */
export function harnessMark(id: string | null | undefined): Mark {
  const mark = markIdOf(id);
  return mark ? HARNESS_MARK[mark] : (ICON.harness as unknown as Mark);
}
