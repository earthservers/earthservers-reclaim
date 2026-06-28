// Right-dock coordination: the native browser surface (WebKitGTK) renders ABOVE
// the DOM, so any DOM panel over the content area gets covered. Instead of hiding
// the page, right-docked panels (NoScript, Privacy, the quick-bookmark popup)
// register here; App then shrinks the surface from the right (insets it) so the
// panel sits in the freed strip and the page stays visible (just narrower).

import { createContext, useCallback, useContext, useEffect, useRef, type ReactNode } from 'react';

/// Default width (CSS px) of the freed right strip. Panels position themselves
/// within it; App insets the browser surface by the open panel's width.
export const RIGHT_DOCK_WIDTH = 380;

/// Wider variant for content-heavy panels (password manager, notes, bookmark
/// manager, theme customizer). App insets the surface by this when one is open.
export const RIGHT_DOCK_WIDTH_WIDE = 560;

export interface RightDockApi {
  /// Register a panel open/closed. When opening, App enforces single-open by
  /// closing whichever other panel is currently open — it does so by invoking
  /// that panel's `onClose` (the panel owns its own open state), so pass yours.
  /// `width` lets a panel widen the freed strip (and the surface inset).
  setOpen: (id: string, open: boolean, opts?: { onClose?: () => void; width?: number }) => void;
  /// Y offset (CSS px) where docked panels should start — just under the browser
  /// chrome / URL bar. Provided by App from the measured chrome height.
  top: number;
}

export const RightDockContext = createContext<RightDockApi | null>(null);

/// Register a panel as open/closed. While any panel is open, App insets the
/// browser surface. Pass `onClose` so single-open enforcement can close this
/// panel when another opens. Safe no-op outside the provider.
export function useRightDock(id: string, open: boolean, onClose?: () => void) {
  const ctx = useContext(RightDockContext);
  // Keep the latest onClose without re-registering on every render (callers
  // typically pass an inline closure). The stable `closer` is what App stores.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const closer = useCallback(() => onCloseRef.current?.(), []);
  useEffect(() => {
    ctx?.setOpen(id, open, { onClose: closer });
    return () => ctx?.setOpen(id, false);
  }, [id, open, ctx, closer]);
}

/// The shared right-panel chrome: a full-height docked panel on the right edge
/// (matching NoScript). Registers via `useRightDock` so the browser surface
/// insets. Use for any panel that should open as "the right panel".
export function RightDockPanel({
  id,
  open,
  top,
  width = RIGHT_DOCK_WIDTH,
  title,
  subtitle,
  onClose,
  children,
}: {
  id: string;
  open: boolean;
  top?: number;
  /// Width (CSS px) of the docked strip for this panel. App insets the browser
  /// surface by the same amount so the panel never overlaps the page.
  width?: number;
  title?: string;
  subtitle?: string;
  onClose?: () => void;
  children: ReactNode;
}) {
  const ctx = useContext(RightDockContext);
  // Register with a stable closer (see useRightDock) so single-open enforcement
  // can close this panel — driven by the panel's own `onClose` — when another opens.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const closer = useCallback(() => onCloseRef.current?.(), []);
  useEffect(() => {
    ctx?.setOpen(id, open, { onClose: closer, width });
    return () => ctx?.setOpen(id, false);
  }, [id, open, ctx, closer, width]);
  if (!open) return null;
  // Default to docking just under the chrome / URL bar (from context).
  const effectiveTop = top ?? ctx?.top ?? 56;
  return (
    <div
      onMouseDown={(e) => e.stopPropagation()}
      style={{ position: 'fixed', top: effectiveTop, right: 8, bottom: 8, width: width - 16 }}
      className="flex flex-col bg-gray-900/97 border border-white/15 rounded-lg shadow-2xl backdrop-blur-sm z-[99999] p-2"
    >
      {title && (
        <div className="px-2 py-1 mb-1 border-b border-white/10 flex-shrink-0 flex items-start justify-between gap-2">
          <div className="min-w-0">
            <span className="text-sm font-medium text-white">{title}</span>
            {subtitle && <p className="text-[11px] text-gray-500 mt-0.5">{subtitle}</p>}
          </div>
          {onClose && (
            <button onClick={onClose} className="p-1 -mr-1 rounded hover:bg-white/10 text-gray-400 hover:text-white flex-shrink-0">
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-y-auto">{children}</div>
    </div>
  );
}
