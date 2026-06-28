// Reusable theme customizer button with standardized placement
// Fixed position in top-right corner, below navigation bar

import React from 'react';
import { Edit3 } from 'lucide-react';
import { useScale, scaleW, scaleH } from '../../../hooks/useScale';

type Props = {
  onClick: () => void;
  tooltipText?: string;
  accentColor?: string;
  textColor?: string;
  show?: boolean; // Control visibility (for permission checks)
};

export default function ThemeCustomizerButton({
  onClick,
  tooltipText = "Customize Theme",
  accentColor = "#f97316",
  textColor = "#ffffff",
  show = true,
}: Props) {
  const scale = useScale();

  if (!show) return null;

  // Base dimensions at 2560x1440
  const baseTop = 100;
  const baseRight = 32;
  const baseSize = 56;
  const baseIconSize = 24;

  return (
    <button
      onClick={onClick}
      className="group fixed z-40 rounded-full shadow-lg backdrop-blur-sm transition-all duration-300 hover:scale-110 hover:shadow-xl"
      style={{
        top: `${scaleH(baseTop, scale.h)}px`,
        right: `${scaleW(baseRight, scale.w)}px`,
        backgroundColor: accentColor,
        color: textColor,
        width: `${scaleW(baseSize, scale.w)}px`,
        height: `${scaleH(baseSize, scale.h)}px`,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
      title={tooltipText}
      aria-label={tooltipText}
    >
      <Edit3
        size={Math.round(scaleW(baseIconSize, scale.w))}
        className="transition-transform group-hover:rotate-12"
      />

      {/* Tooltip */}
      <span
        className="absolute right-full mr-3 whitespace-nowrap rounded-lg px-3 py-2 text-sm font-medium opacity-0 transition-opacity group-hover:opacity-100 pointer-events-none"
        style={{
          backgroundColor: accentColor,
          color: textColor,
        }}
      >
        {tooltipText}
      </span>
    </button>
  );
}
