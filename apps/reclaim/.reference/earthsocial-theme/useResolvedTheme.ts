"use client"
import { useMemo } from "react"
import { DEFAULT_THEME, PRESET_THEMES, type ThemeTokens } from "./tokens"

// Pass in whatever is available on this page
export function useResolvedTheme(opts: {
  site?: ThemeTokens
  user?: ThemeTokens          // user preference
  community?: ThemeTokens     // group/chat theme
  profile?: ThemeTokens       // profile page theme
  presetKey?: keyof typeof PRESET_THEMES // fallback to a preset by key
}) {
  return useMemo<ThemeTokens>(() => {
    const base = opts.site ?? DEFAULT_THEME
    const preset = opts.presetKey ? PRESET_THEMES[opts.presetKey] : undefined
    // Precedence: profile > community > user > preset > site > default
    return { 
      ...base,
      ...(preset ?? {}),
      ...(opts.user ?? {}),
      ...(opts.community ?? {}),
      ...(opts.profile ?? {}),
    }
  }, [opts.site, opts.user, opts.community, opts.profile, opts.presetKey])
}
