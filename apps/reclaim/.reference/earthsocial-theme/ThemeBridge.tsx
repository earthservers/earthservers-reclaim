/**
 * ThemeBridge - Provides theme context to Three.js components
 *
 * IMPORTANT: Must be placed INSIDE <Canvas> for Html components to access theme
 *
 * The Html component from @react-three/drei creates a separate React portal,
 * which means it doesn't inherit context from the parent app. This bridge
 * component creates a new context tree inside the Canvas that Html portals
 * can access.
 *
 * Usage:
 * ```tsx
 * <Canvas>
 *   <ThemeBridge theme={yourTheme}>
 *     <YourScene />
 *   </ThemeBridge>
 * </Canvas>
 * ```
 */

import React, { createContext, useContext, ReactNode } from 'react';
import { DEFAULT_THEME, type ThemeTokens } from '../../../shared/theme/tokens';

// Create context for Three.js components
const ThreeThemeContext = createContext<ThemeTokens>(DEFAULT_THEME);

/**
 * Hook to use theme inside Three.js components (including Html portals)
 *
 * This hook works everywhere inside the Canvas, including inside Html components.
 */
export const useThreeTheme = (): ThemeTokens => {
  const theme = useContext(ThreeThemeContext);
  return theme || DEFAULT_THEME;
};

// Bridge component props
interface ThemeBridgeProps {
  theme?: ThemeTokens;
  children: ReactNode;
}

/**
 * ThemeBridge - Provides theme to Three.js components
 *
 * Place this component INSIDE <Canvas> to make theme available to all
 * Three.js components, including Html portals.
 */
export const ThemeBridge: React.FC<ThemeBridgeProps> = ({
  theme = DEFAULT_THEME,
  children
}) => {
  return (
    <ThreeThemeContext.Provider value={theme}>
      {children}
    </ThreeThemeContext.Provider>
  );
};
