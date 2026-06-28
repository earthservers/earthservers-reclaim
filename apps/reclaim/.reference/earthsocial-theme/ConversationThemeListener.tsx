// src/components/ConversationThemeListener.tsx
import { useEffect } from "react";
import { getSocket, trackRoom, untrackRoom } from "@/lib/socket";

/** Payload shape from the backend for conversation theme updates */
export type ThemeUpdate = {
  id: string;
  name: string | null;
  themePresetKey: string | null;
  themeOverrides: unknown;
};

export function ConversationThemeListener({
  conversationId,
  onTheme,
}: {
  conversationId: string;
  onTheme: (u: ThemeUpdate) => void;
}) {
  useEffect(() => {
    const s = getSocket();
    if (!s) return;

    trackRoom(conversationId); // join + auto-rejoin

    const handler = (payload: ThemeUpdate) => {
      if (payload.id === conversationId) onTheme(payload);
    };

    s.on("conversation:theme", handler);
    return () => {
      if (!s) return;
      s.off("conversation:theme", handler);
      untrackRoom(conversationId);
    };
  }, [conversationId, onTheme]);

  return null;
}
