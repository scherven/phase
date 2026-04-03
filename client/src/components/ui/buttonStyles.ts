export type GameButtonTone =
  | "neutral"
  | "emerald"
  | "amber"
  | "blue"
  | "red"
  | "indigo"
  | "slate";

export type GameButtonSize = "xs" | "sm" | "md" | "lg";

interface GameButtonOptions {
  tone: GameButtonTone;
  size?: GameButtonSize;
  disabled?: boolean;
  className?: string;
}

const BASE_CLASSES =
  "min-h-9 border border-solid font-semibold backdrop-blur-xl transition-all duration-150 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40 inline-flex items-center justify-center shadow-[0_10px_24px_rgba(15,23,42,0.18)]";

const SIZE_CLASSES: Record<GameButtonSize, string> = {
  xs: "px-2.5 py-1 text-xs rounded-full",
  sm: "px-3.5 py-2 text-sm rounded-full",
  md: "px-3.5 py-2 text-[11px] rounded-full lg:px-4 lg:text-xs",
  lg: "px-6 py-3 text-base rounded-full",
};

const TONE_CLASSES: Record<GameButtonTone, string> = {
  neutral:
    "border-white/12 bg-white/8 text-slate-100 hover:border-white/20 hover:bg-white/12",
  emerald:
    "border-emerald-300/35 bg-emerald-500/18 text-emerald-50 hover:bg-emerald-500/24",
  amber:
    "border-amber-300/35 bg-amber-500/16 text-amber-50 hover:bg-amber-500/24",
  blue: "border-blue-300/35 bg-blue-500/18 text-blue-50 hover:bg-blue-500/24",
  red: "border-red-300/35 bg-red-500/18 text-red-50 hover:bg-red-500/24",
  indigo:
    "border-indigo-300/35 bg-indigo-500/18 text-indigo-50 hover:bg-indigo-500/24",
  slate:
    "border-white/10 bg-slate-900/76 text-slate-100 hover:border-white/20 hover:bg-slate-800/76",
};

export function gameButtonClass({
  tone,
  size = "md",
  disabled = false,
  className = "",
}: GameButtonOptions): string {
  const parts = [BASE_CLASSES, SIZE_CLASSES[size], TONE_CLASSES[tone]];

  if (disabled) {
    parts.push("opacity-40 pointer-events-none");
  }

  if (className) {
    parts.push(className);
  }

  return parts.join(" ");
}
