import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '../lib/tauri';

interface IncognitoToggleProps {
  profileId: number;
  onStatusChange?: (isIncognito: boolean) => void;
}

export function IncognitoToggle({ profileId, onStatusChange }: IncognitoToggleProps) {
  const [isIncognito, setIsIncognito] = useState(false);
  const [isForced, setIsForced] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [showTooltip, setShowTooltip] = useState(false);
  const [tooltipPosition, setTooltipPosition] = useState({ top: 0, right: 0 });
  const buttonRef = useRef<HTMLButtonElement>(null);

  // Load initial status when profileId changes
  useEffect(() => {
    loadStatus();
  }, [profileId]);

  const loadStatus = async () => {
    try {
      const [status, forced] = await Promise.all([
        invoke<boolean>('get_incognito_status', { profileId }),
        invoke<boolean>('incognito_is_forced', { profileId }).catch(() => false),
      ]);
      setIsForced(forced);
      setIsIncognito(status);
    } catch (err) {
      console.error('Failed to get incognito status:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleToggle = async () => {
    // The dedicated Incognito profile is always private — the toggle is locked on.
    if (isForced) return;
    try {
      const newStatus = await invoke<boolean>('toggle_incognito', { profileId });
      setIsIncognito(newStatus);
      onStatusChange?.(newStatus);
    } catch (err) {
      console.error('Failed to toggle incognito:', err);
    }
  };

  if (isLoading) {
    return (
      <button
        disabled
        className="p-2 rounded-lg bg-white/5 border border-white/10 text-gray-500"
      >
        <svg className="w-5 h-5 animate-pulse" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
          />
        </svg>
      </button>
    );
  }

  const handleMouseEnter = () => {
    if (buttonRef.current) {
      const rect = buttonRef.current.getBoundingClientRect();
      setTooltipPosition({
        top: rect.bottom + 8,
        right: window.innerWidth - rect.right,
      });
      setShowTooltip(true);
    }
  };

  const handleMouseLeave = () => {
    setShowTooltip(false);
  };

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          console.log('Incognito toggle clicked, current state:', isIncognito);
          handleToggle();
        }}
        onMouseDown={(e) => {
          e.stopPropagation();
        }}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        className={`p-2.5 rounded-xl transition-all relative z-50 ${
          isForced ? 'cursor-default' : 'cursor-pointer'
        } ${
          isIncognito
            ? 'bg-purple-600/30 border-2 border-purple-500 text-purple-300 shadow-lg shadow-purple-500/20'
            : 'bg-white/10 border border-white/20 text-white/80 hover:bg-white/15 hover:text-white'
        }`}
        title={isForced ? 'This profile is always in Incognito Mode' : isIncognito ? 'Exit Incognito Mode' : 'Enter Incognito Mode'}
        style={{ WebkitAppRegion: 'no-drag', pointerEvents: 'auto' } as React.CSSProperties}
      >
        {isIncognito ? (
          // Incognito active icon (eye with slash)
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"
            />
          </svg>
        ) : (
          // Normal mode icon (eye)
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
            />
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
            />
          </svg>
        )}
      </button>

      {/* Tooltip - rendered via portal to escape overflow hidden */}
      {showTooltip && createPortal(
        <div
          className="fixed px-3 py-2 bg-gray-900 border border-white/10 rounded-lg text-sm whitespace-nowrap pointer-events-none z-[9999] shadow-xl"
          style={{ top: tooltipPosition.top, right: tooltipPosition.right }}
        >
          {isForced ? (
            <div className="text-purple-300">
              <div className="font-medium">Incognito Profile</div>
              <div className="text-xs text-gray-400">This profile is always private — can't be turned off</div>
            </div>
          ) : isIncognito ? (
            <div className="text-purple-300">
              <div className="font-medium">Incognito Mode Active</div>
              <div className="text-xs text-gray-400">Click to exit incognito mode</div>
            </div>
          ) : (
            <div className="text-gray-300">
              <div className="font-medium">Normal Mode</div>
              <div className="text-xs text-gray-400">Click to enable incognito</div>
            </div>
          )}
        </div>,
        document.body
      )}
    </>
  );
}

// Incognito banner to show when mode is active
export function IncognitoBanner({ isVisible }: { isVisible: boolean }) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const collapseTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Auto-collapse after 3 seconds
  useEffect(() => {
    if (isVisible && !isCollapsed) {
      collapseTimeoutRef.current = setTimeout(() => {
        setIsCollapsed(true);
      }, 3000);
    }

    return () => {
      if (collapseTimeoutRef.current) {
        clearTimeout(collapseTimeoutRef.current);
      }
    };
  }, [isVisible, isCollapsed]);

  // Reset collapsed state when visibility changes
  useEffect(() => {
    if (isVisible) {
      setIsCollapsed(false);
    }
  }, [isVisible]);

  if (!isVisible) return null;

  return (
    <div
      className={`bg-gradient-to-r from-purple-900/50 to-purple-800/50 border-b border-purple-500/30 transition-all duration-300 cursor-pointer overflow-hidden ${
        isCollapsed && !isHovered ? 'h-1.5' : 'h-auto'
      }`}
      onClick={() => setIsCollapsed(!isCollapsed)}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      title={isCollapsed ? 'Click to expand' : 'Click to collapse'}
    >
      <div className={`container mx-auto flex items-center justify-center gap-2 text-purple-200 transition-all duration-300 ${
        isCollapsed && !isHovered ? 'opacity-0 py-0' : 'opacity-100 px-4 py-2'
      }`}>
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"
          />
        </svg>
        <span className="text-sm font-medium">
          Incognito Mode - Your browsing activity will not be saved
        </span>
      </div>
    </div>
  );
}

export default IncognitoToggle;
