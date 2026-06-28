"use client";
import { useMemo } from "react";
import { DEFAULT_THEME, PRESET_THEMES } from "./tokens.js";
// Pass in whatever is available on this page
export function useResolvedTheme(opts) {
    return useMemo(() => {
        const base = opts.site ?? DEFAULT_THEME;
        const preset = opts.presetKey ? PRESET_THEMES[opts.presetKey] : undefined;
        // Precedence: profile > community > user > preset > site > default
        return {
            ...base,
            ...(preset ?? {}),
            ...(opts.user ?? {}),
            ...(opts.community ?? {}),
            ...(opts.profile ?? {}),
        };
    }, [opts.site, opts.user, opts.community, opts.profile, opts.presetKey]);
}
