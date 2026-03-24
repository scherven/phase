import { useState } from "react";
import { createPortal } from "react-dom";
import { motion, AnimatePresence } from "framer-motion";

import { menuButtonClass } from "./buttonStyles";
import { FEED_REGISTRY } from "../../data/feedRegistry";
import {
  listSubscriptions,
  subscribe,
  unsubscribe,
  refreshFeed,
} from "../../services/feedService";
import type { FeedSubscription } from "../../types/feed";

interface FeedManagerModalProps {
  open: boolean;
  onClose: () => void;
}

export function FeedManagerModal({ open, onClose }: FeedManagerModalProps) {
  const [subs, setSubs] = useState<FeedSubscription[]>(() => listSubscriptions());
  const [customUrl, setCustomUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<string | null>(null);

  const subscribedIds = new Set(subs.map((s) => s.sourceId));

  const handleSubscribe = async (sourceId: string) => {
    setLoading(sourceId);
    setError(null);
    try {
      await subscribe(sourceId);
      setSubs(listSubscriptions());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(null);
    }
  };

  const handleUnsubscribe = (feedId: string) => {
    unsubscribe(feedId);
    setSubs(listSubscriptions());
  };

  const handleRefresh = async (feedId: string) => {
    setLoading(feedId);
    setError(null);
    try {
      await refreshFeed(feedId);
      setSubs(listSubscriptions());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(null);
    }
  };

  const handleCustomSubscribe = async () => {
    const url = customUrl.trim();
    if (!url) return;
    setLoading("custom");
    setError(null);
    try {
      await subscribe(url);
      setSubs(listSubscriptions());
      setCustomUrl("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(null);
    }
  };

  return createPortal(
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
        >
          <motion.div
            className="mx-4 w-full max-w-lg rounded-2xl border border-white/10 bg-[#0c1120] p-6 shadow-2xl"
            initial={{ scale: 0.95, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            onClick={(e) => e.stopPropagation()}
          >
            <h2 className="mb-4 text-lg font-semibold text-white">Manage Feeds</h2>

            {error && (
              <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">
                {error}
              </div>
            )}

            <div className="flex flex-col gap-3">
              {FEED_REGISTRY.map((source) => {
                const isSubscribed = subscribedIds.has(source.id);
                const sub = subs.find((s) => s.sourceId === source.id);
                const isLoading = loading === source.id;

                return (
                  <div
                    key={source.id}
                    className="flex items-center justify-between rounded-xl border border-white/10 bg-black/20 px-4 py-3"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        {source.icon && (
                          <span className="inline-flex h-5 w-5 items-center justify-center rounded bg-white/10 text-[10px] font-bold text-white">
                            {source.icon}
                          </span>
                        )}
                        <span className="text-sm font-medium text-white">{source.name}</span>
                        <span className="rounded bg-white/5 px-1.5 py-0.5 text-[10px] text-slate-500">
                          {source.type}
                        </span>
                      </div>
                      {source.description && (
                        <p className="mt-0.5 text-xs text-slate-500">{source.description}</p>
                      )}
                      {sub && (
                        <p className="mt-0.5 text-[10px] text-slate-600">
                          Last refreshed: {new Date(sub.lastRefreshedAt).toLocaleDateString()}
                          {sub.error && <span className="ml-2 text-red-400">{sub.error}</span>}
                        </p>
                      )}
                    </div>
                    <div className="ml-3 flex shrink-0 gap-2">
                      {isSubscribed && (
                        <button
                          onClick={() => handleRefresh(source.id)}
                          disabled={isLoading}
                          className="rounded px-2 py-1 text-xs text-slate-400 ring-1 ring-white/10 transition-colors hover:bg-white/5 hover:text-white disabled:opacity-40"
                        >
                          {isLoading ? "…" : "Refresh"}
                        </button>
                      )}
                      <button
                        onClick={() => isSubscribed ? handleUnsubscribe(source.id) : handleSubscribe(source.id)}
                        disabled={isLoading}
                        className={`rounded px-3 py-1 text-xs font-medium transition-colors disabled:opacity-40 ${
                          isSubscribed
                            ? "text-red-300 ring-1 ring-red-500/30 hover:bg-red-500/10"
                            : "text-emerald-300 ring-1 ring-emerald-500/30 hover:bg-emerald-500/10"
                        }`}
                      >
                        {isSubscribed ? "Unsubscribe" : "Subscribe"}
                      </button>
                    </div>
                  </div>
                );
              })}

              {/* Custom URL feeds */}
              {subs
                .filter((sub) => !FEED_REGISTRY.some((s) => s.id === sub.sourceId))
                .map((sub) => (
                  <div
                    key={sub.sourceId}
                    className="flex items-center justify-between rounded-xl border border-white/10 bg-black/20 px-4 py-3"
                  >
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-white">{sub.sourceId}</div>
                      <p className="mt-0.5 truncate text-[10px] text-slate-600">{sub.url}</p>
                    </div>
                    <div className="ml-3 flex shrink-0 gap-2">
                      <button
                        onClick={() => handleRefresh(sub.sourceId)}
                        disabled={loading === sub.sourceId}
                        className="rounded px-2 py-1 text-xs text-slate-400 ring-1 ring-white/10 transition-colors hover:bg-white/5 hover:text-white disabled:opacity-40"
                      >
                        {loading === sub.sourceId ? "…" : "Refresh"}
                      </button>
                      <button
                        onClick={() => handleUnsubscribe(sub.sourceId)}
                        className="rounded px-3 py-1 text-xs font-medium text-red-300 ring-1 ring-red-500/30 transition-colors hover:bg-red-500/10"
                      >
                        Unsubscribe
                      </button>
                    </div>
                  </div>
                ))}
            </div>

            <div className="mt-4 border-t border-white/10 pt-4">
              <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400">Add Custom Feed</h3>
              <div className="flex gap-2">
                <input
                  type="url"
                  value={customUrl}
                  onChange={(e) => setCustomUrl(e.target.value)}
                  placeholder="https://example.com/feed.json"
                  className="flex-1 rounded-lg border border-white/10 bg-black/30 px-3 py-2 text-sm text-white placeholder-slate-600 outline-none focus:border-white/20"
                  onKeyDown={(e) => e.key === "Enter" && handleCustomSubscribe()}
                />
                <button
                  onClick={handleCustomSubscribe}
                  disabled={!customUrl.trim() || loading === "custom"}
                  className={menuButtonClass({ tone: "indigo", size: "sm", disabled: !customUrl.trim() || loading === "custom" })}
                >
                  {loading === "custom" ? "…" : "Add"}
                </button>
              </div>
            </div>

            <div className="mt-4 flex justify-end">
              <button
                onClick={onClose}
                className={menuButtonClass({ tone: "neutral", size: "sm" })}
              >
                Done
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
