// src/theme/ThemeScope.tsx
"use client";
import { jsx as _jsx } from "react/jsx-runtime";
function cssVars(t) {
    const vars = {
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
export function ThemeScope({ theme, id, className, children, as: Tag = "div", }) {
    const dataTheme = id ?? theme.name?.toLowerCase().replace(/\s+/g, "-");
    return (_jsx(Tag, { className: className, style: cssVars(theme), "data-theme": dataTheme, children: children }));
}
