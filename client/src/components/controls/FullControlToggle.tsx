import { useUiStore } from "../../stores/uiStore.ts";

export function FullControlToggle() {
  const fullControl = useUiStore((s) => s.fullControl);
  const toggleFullControl = useUiStore((s) => s.toggleFullControl);

  return (
    <button
      onClick={toggleFullControl}
      className={`rounded-full border px-3 py-1 text-[10px] font-semibold uppercase tracking-[0.18em] backdrop-blur-xl transition-all duration-200 lg:px-3.5 lg:py-1.5 lg:text-[11px] ${
        fullControl
          ? "border-amber-300/35 bg-amber-500/18 text-amber-100 shadow-[0_10px_24px_rgba(245,158,11,0.2)]"
          : "border-white/10 bg-slate-950/64 text-slate-300 hover:border-white/20 hover:text-white"
      }`}
    >
      Full Control {fullControl ? "On" : "Off"}
    </button>
  );
}
