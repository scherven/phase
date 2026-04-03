import type { ReactNode } from "react";

type HudTone = "neutral" | "emerald" | "rose" | "cyan" | "amber";

interface HudPlateProps {
  label: string;
  tone?: HudTone;
  onClick?: () => void;
  children: ReactNode;
  trailing?: ReactNode;
}

const TONE_CLASSES: Record<HudTone, string> = {
  neutral: "border-white/12 bg-slate-950/72 text-slate-100 shadow-[0_16px_48px_rgba(15,23,42,0.45)]",
  emerald: "border-emerald-400/30 bg-emerald-950/40 text-emerald-50 shadow-[0_16px_48px_rgba(16,185,129,0.18)]",
  rose: "border-rose-400/30 bg-rose-950/38 text-rose-50 shadow-[0_16px_48px_rgba(244,63,94,0.18)]",
  cyan: "border-cyan-400/40 bg-cyan-950/42 text-cyan-50 shadow-[0_16px_48px_rgba(34,211,238,0.2)]",
  amber: "border-amber-400/30 bg-amber-950/38 text-amber-50 shadow-[0_16px_48px_rgba(245,158,11,0.18)]",
};

export function HudPlate({
  label,
  tone = "neutral",
  onClick,
  children,
  trailing,
}: HudPlateProps) {
  const Component = onClick ? "button" : "div";

  return (
    <Component
      type={onClick ? "button" : undefined}
      onClick={onClick}
      className={`group relative inline-flex max-w-full items-center gap-2 rounded-[18px] border px-2.5 py-1.5 backdrop-blur-xl transition-all duration-200 ${TONE_CLASSES[tone]} ${
        onClick ? "cursor-pointer hover:-translate-y-0.5 hover:border-white/30" : ""
      }`}
    >
      <div className="absolute inset-[1px] rounded-[16px] bg-gradient-to-b from-white/8 via-transparent to-black/10" />
      <div className="relative min-w-0">
        <div className="mb-0.5 flex items-center">
          <span className="text-[9px] font-semibold uppercase tracking-[0.18em] text-white/68">
            {label}
          </span>
        </div>
        <div className="flex items-center gap-2">
          {children}
        </div>
      </div>
      {trailing ? (
        <div className="relative flex shrink-0 items-center gap-1.5">
          {trailing}
        </div>
      ) : null}
    </Component>
  );
}
