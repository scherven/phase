import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";

import { ConnectionDot } from "../multiplayer/ConnectionDot.tsx";
import { VolumeControl } from "./VolumeControl.tsx";
import { clearGame } from "../../stores/gameStore.ts";
import { useCardDataMeta } from "../../hooks/useCardDataMeta.ts";

interface GameMenuProps {
  gameId: string;
  isAiMode: boolean;
  isOnlineMode: boolean;
  showAiHand: boolean;
  onToggleAiHand: () => void;
  onSettingsClick: () => void;
  onConcede?: () => void;
}

export function GameMenu({
  gameId,
  isAiMode,
  isOnlineMode,
  showAiHand,
  onToggleAiHand,
  onSettingsClick,
  onConcede,
}: GameMenuProps) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const cardDataMeta = useCardDataMeta();

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div
      ref={menuRef}
      className="fixed z-40"
      style={{
        left: "calc(env(safe-area-inset-left) + 0.5rem)",
        top: "calc(env(safe-area-inset-top) + var(--game-top-overlay-offset, 0px) + 0.5rem)",
      }}
    >
      <div className="flex items-center gap-2">
        <button
          onClick={() => setOpen(!open)}
          className="flex h-9 w-9 items-center justify-center rounded-lg bg-gray-800/80 text-gray-400 transition-colors hover:bg-gray-700/80 hover:text-gray-200"
          aria-label="Game menu"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 20 20"
            fill="currentColor"
            className="h-5 w-5"
          >
            <path
              fillRule="evenodd"
              d="M2 4.75A.75.75 0 0 1 2.75 4h14.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 4.75ZM2 10a.75.75 0 0 1 .75-.75h14.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 10Zm0 5.25a.75.75 0 0 1 .75-.75h14.5a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1-.75-.75Z"
              clipRule="evenodd"
            />
          </svg>
        </button>
        <VolumeControl variant="game" />
        {isOnlineMode && <ConnectionDot />}
      </div>
      {open && (
        <div className="absolute left-0 top-full mt-1 w-52 rounded-lg border border-gray-700 bg-gray-900/95 py-1 shadow-xl backdrop-blur-sm">
          <MenuButton label="Resume" onClick={() => setOpen(false)} />
          <MenuButton
            label="Settings"
            onClick={() => {
              setOpen(false);
              onSettingsClick();
            }}
          />
          {isAiMode && (
            <MenuButton
              label={showAiHand ? "Hide AI Hand" : "Show AI Hand"}
              onClick={() => {
                onToggleAiHand();
                setOpen(false);
              }}
            />
          )}
          <div className="my-1 border-t border-gray-700" />
          <MenuButton
            label="Concede"
            variant="danger"
            onClick={() => {
              setOpen(false);
              if (isOnlineMode && onConcede) {
                onConcede();
              } else {
                clearGame(gameId);
                navigate("/");
              }
            }}
          />
          <MenuButton
            label="Main Menu"
            onClick={() => navigate("/")}
          />
          <div className="my-1 border-t border-gray-700" />
          <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 px-3 py-1.5 text-[10px] text-slate-500">
            <a
              href={`${__GIT_REPO_URL__}/commit/${__BUILD_HASH__}`}
              target="_blank"
              rel="noopener noreferrer"
              className="transition-colors hover:text-white"
            >
              v{__APP_VERSION__} {__BUILD_HASH__}
            </a>
            {cardDataMeta && (
              <>
                <span className="text-slate-700">·</span>
                <a
                  href={`${__GIT_REPO_URL__}/commit/${cardDataMeta.commit}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="transition-colors hover:text-white"
                  title={`Card data: ${cardDataMeta.generated_at}`}
                >
                  cards {cardDataMeta.commit_short}
                </a>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function MenuButton({
  label,
  onClick,
  variant,
}: {
  label: string;
  onClick: () => void;
  variant?: "danger";
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full px-3 py-2 text-left text-sm transition-colors ${
        variant === "danger"
          ? "text-red-400 hover:bg-red-900/30 hover:text-red-300"
          : "text-gray-300 hover:bg-gray-800 hover:text-white"
      }`}
    >
      {label}
    </button>
  );
}
