import { motion } from "framer-motion";

import type { PlayerSlot } from "../../stores/multiplayerStore";
import { MenuPanel } from "../menu/MenuShell";
import { menuButtonClass } from "../menu/buttonStyles";
import { ReadyRoom } from "./ReadyRoom";
import type { ChatMessage } from "./ReadyRoom";

interface WaitingScreenProps {
  gameCode: string;
  isPublic: boolean;
  onCancel: () => void;
  /** When provided, shows the ReadyRoom instead of the simple waiting screen. */
  playerSlots?: PlayerSlot[];
  currentPlayerId?: string;
  isHost?: boolean;
  minPlayers?: number;
  onToggleReady?: () => void;
  onStartGame?: () => void;
  onSendChat?: (message: string) => void;
  chatMessages?: ChatMessage[];
}

export function WaitingScreen({
  gameCode,
  isPublic,
  onCancel,
  playerSlots,
  currentPlayerId,
  isHost,
  minPlayers,
  onToggleReady,
  onStartGame,
  onSendChat,
  chatMessages,
}: WaitingScreenProps) {
  // Use ReadyRoom when player slots are available (multiplayer with ready-up)
  if (playerSlots && currentPlayerId && onToggleReady && onStartGame && onSendChat) {
    return (
      <ReadyRoom
        gameCode={gameCode}
        playerSlots={playerSlots}
        currentPlayerId={currentPlayerId}
        isHost={isHost ?? false}
        minPlayers={minPlayers ?? 2}
        onToggleReady={onToggleReady}
        onStartGame={onStartGame}
        onCancel={onCancel}
        onSendChat={onSendChat}
        chatMessages={chatMessages ?? []}
      />
    );
  }

  // Simple waiting screen for 2-player P2P games
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/80" />

      <MenuPanel className="relative z-10 flex flex-col items-center gap-6 p-10">
        <h2 className="text-xl font-bold text-white">Waiting for Opponent</h2>

        {/* Game code */}
        <div className="text-center">
          <p className="mb-2 text-xs font-medium uppercase tracking-wider text-gray-400">
            Game Code
          </p>
          <p className="font-mono text-4xl font-bold tracking-widest text-emerald-400">
            {gameCode}
          </p>
        </div>

        {/* Local IP hint */}
        <p className="text-xs text-gray-500">
          LAN: {window.location.hostname || "localhost"}
        </p>

        {/* Player count */}
        {playerSlots && playerSlots.length > 0 && (
          <div className="flex flex-col items-center gap-2">
            <p className="text-xs font-medium uppercase tracking-wider text-gray-400">
              Players
            </p>
            <div className="flex flex-col gap-1.5">
              {playerSlots.map((slot, i) => {
                const claimed = slot.isAi || slot.name;
                return (
                  <div
                    key={slot.playerId}
                    className="flex items-center gap-2 rounded-lg border border-white/10 bg-black/18 px-3 py-1.5"
                  >
                    <div className={`h-2 w-2 rounded-full ${claimed ? "bg-emerald-400" : "bg-gray-600"}`} />
                    <span className="text-xs text-gray-500">{i + 1}</span>
                    <span className="min-w-[6rem] text-sm text-gray-200">
                      {slot.isAi
                        ? `AI (${slot.aiDifficulty})`
                        : slot.name || "Waiting..."}
                    </span>
                    {slot.playerId === currentPlayerId && (
                      <span className="text-xs text-cyan-400">(you)</span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Public badge */}
        {isPublic && (
          <span className="rounded-full bg-emerald-500/20 px-3 py-1 text-xs font-medium text-emerald-300">
            Listed in lobby
          </span>
        )}

        {/* Helper text */}
        <p className="max-w-xs text-center text-sm text-gray-400">
          Share the code with a friend, or wait for someone from the lobby.
        </p>

        {/* Animated waiting dots */}
        <div className="flex gap-2">
          {[0, 1, 2].map((i) => (
            <motion.div
              key={i}
              className="h-2.5 w-2.5 rounded-full bg-emerald-400"
              animate={{ scale: [1, 1.5, 1], opacity: [0.4, 1, 0.4] }}
              transition={{
                duration: 1.2,
                repeat: Infinity,
                delay: i * 0.2,
                ease: "easeInOut",
              }}
            />
          ))}
        </div>

        {/* Cancel button */}
        <button
          onClick={onCancel}
          className={menuButtonClass({ tone: "neutral", size: "sm" })}
        >
          Cancel
        </button>
      </MenuPanel>
    </div>
  );
}
