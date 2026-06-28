// src/theme/ThemeScope.tsx
"use client";

import React from "react";
import type { ThemeTokens } from "./tokens";

// Allow CSS custom properties strongly-typed
type CSSVars = React.CSSProperties & Record<`--${string}`, string | number>;

function cssVars(t: ThemeTokens): CSSVars {
  const vars: CSSVars = {
    ["--text-color"]: t.textColor,
    ["--accent"]: t.accentColor,
    ["--card-bg"]: t.cardBg,
    ["--app-bg"]: t.appBg ?? "",
    ["--radius"]: t.radius ?? "12px",
    ["--bg1"]: t.bg1,
    ["--bg2"]: t.bg2,
    ["--aurora-speed"]: t.auroraSpeed ?? "16s",
  };
  return vars;
}

export type ThemeScopeProps = {
  theme: ThemeTokens;
  id?: string; // for debugging: data-theme attribute
  className?: string;
  children: React.ReactNode;
  as?: React.ElementType; // ✅ was: keyof JSX.IntrinsicElements
};

export function ThemeScope({
  theme,
  id,
  className,
  children,
  as: Tag = "div",
}: ThemeScopeProps) {
  const dataTheme = id ?? theme.name?.toLowerCase().replace(/\s+/g, "-");
  return (
    <Tag className={className} style={cssVars(theme)} data-theme={dataTheme}>
      {children}
    </Tag>
  );
}
