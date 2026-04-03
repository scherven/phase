interface StatusBadgeProps {
  label: string;
  value?: number | string;
  tone?: "neutral" | "amber";
}

export function StatusBadge({
  label,
  value,
  tone = "neutral",
}: StatusBadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-2 py-1 text-[10px] font-semibold tracking-[0.16em] uppercase ${
        tone === "amber"
          ? "bg-amber-400/16 text-amber-100 ring-1 ring-amber-300/30"
          : "bg-white/7 text-slate-200 ring-1 ring-white/10"
      }`}
    >
      <span>{label}</span>
      {value != null ? <span className="tabular-nums text-white">{value}</span> : null}
    </span>
  );
}
