import { useState, useEffect, useRef, useCallback } from 'react';
import { createRoot } from 'react-dom/client';
import { getDesignConstant, PixelatedBubble } from '@theme/design-constants';
import type { AnimationConfig } from '@theme/animation-config';

type Props = {
  animations: {
    characters: AnimationConfig[];
    bubbles: AnimationConfig | null;
    decorations: AnimationConfig[];
  };
  isMobile: boolean;
  bubbleColor?: string;
  turtleColor?: string;
  enabled?: boolean;
  zIndex?: number;
};

interface AnimationElement {
  id: string;
  element: HTMLDivElement;
  root?: ReturnType<typeof createRoot>;
  config: AnimationConfig;
  index: number;
  position: { x: string; y: string };
  size: number;
}

function AnimationLayer({ animations, isMobile, bubbleColor = '#67e8f9', turtleColor = '#10b981', enabled = true, zIndex = 9 }: Props) {
  const [isVisible, setIsVisible] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Track all animation elements by ID
  const elementsRef = useRef<Map<string, AnimationElement>>(new Map());
  const bubblesRef = useRef<Map<number, AnimationElement>>(new Map());

  // Track previous configs to detect changes
  const prevConfigsRef = useRef<{
    characters: AnimationConfig[];
    decorations: AnimationConfig[];
    bubbles: AnimationConfig | null;
  }>({ characters: [], decorations: [], bubbles: null });

  // Wait for component to mount
  useEffect(() => {
    if (!enabled) {
      setIsVisible(false);
      return;
    }

    let timeoutId: NodeJS.Timeout | null = null;
    let rafId2: number | null = null;

    const rafId = requestAnimationFrame(() => {
      rafId2 = requestAnimationFrame(() => {
        timeoutId = setTimeout(() => {
          setIsVisible(true);
        }, 50);
      });
    });

    return () => {
      cancelAnimationFrame(rafId);
      if (rafId2) cancelAnimationFrame(rafId2);
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, [enabled]);

  const getAnimationStyle = useCallback((type: AnimationConfig['type'], index: number, speed: number) => {
    const baseSpeed = 15;
    const duration = baseSpeed / speed;

    switch (type) {
      case 'swimming':
        return {
          animationName: 'swim',
          animationDuration: `${duration}s`,
          animationIterationCount: 'infinite' as const,
          animationTimingFunction: 'ease-in-out',
          animationDirection: index % 2 === 0 ? 'normal' as const : 'reverse' as const,
          animationDelay: `${index * 2}s`
        };
      case 'floating':
        return {
          animationName: 'float',
          animationDuration: `${duration}s`,
          animationIterationCount: 'infinite' as const,
          animationTimingFunction: 'ease-in-out',
          animationDelay: `${index * 1.5}s`
        };
      case 'rising':
        return {
          animationName: 'bubble',
          animationDuration: `${duration}s`,
          animationIterationCount: 'infinite' as const,
          animationTimingFunction: 'ease-in-out',
          animationDelay: `${Math.random() * 5}s`
        };
      case 'falling':
        return {
          animationName: 'fall',
          animationDuration: `${duration / 2}s`,
          animationIterationCount: 'infinite' as const,
          animationTimingFunction: 'ease-in',
          animationDelay: `${index * 0.5}s`
        };
      default:
        return {};
    }
  }, []);

  const generatePosition = useCallback((config: AnimationConfig, index: number): { x: string; y: string } => {
    if (config.positions?.[index]) {
      return config.positions[index];
    }

    if (config.type === 'falling') {
      return {
        x: `${Math.random() * 90 + 5}%`,
        y: '2rem'
      };
    } else if (config.id === 'eagle') {
      return {
        x: `${Math.random() * 90 + 5}%`,
        y: `${Math.random() * 50 + 15}%`
      };
    } else if (config.id === 'cloud') {
      return {
        x: `${Math.random() * 90 + 5}%`,
        y: `${Math.random() * 80 + 10}%`
      };
    } else {
      return {
        x: `${Math.random() * 90 + 5}%`,
        y: `${Math.random() * 80 + 10}%`
      };
    }
  }, []);

  const updateSpeed = useCallback((elementId: string, speed: number, type: AnimationConfig['type']) => {
    const animEl = elementsRef.current.get(elementId);
    if (!animEl) return;

    const baseSpeed = 15;
    const duration = baseSpeed / speed;
    const finalDuration = type === 'falling' ? duration / 2 : duration;

    // Directly update animation duration without re-render
    animEl.element.style.animationDuration = `${finalDuration}s`;
  }, []);

  const createAnimationElement = useCallback((
    config: AnimationConfig,
    index: number,
    container: HTMLElement
  ): AnimationElement | null => {
    const design = getDesignConstant(config.id);
    if (!design || !config.enabled) return null;

    const element = document.createElement('div');
    element.className = 'absolute pointer-events-none';

    const elementId = `${config.id}-${index}`;
    const position = generatePosition(config, index);
    const size = isMobile
      ? (config.size.min + config.size.max) / 3
      : Math.random() * (config.size.max - config.size.min) + config.size.min;

    // Z-index logic
    let zIndexVal: number | undefined;
    if (config.id === 'sun') {
      zIndexVal = 15;
    } else if (config.id === 'cloud') {
      zIndexVal = Math.random() < 0.1 ? 15 : Math.floor(Math.random() * 5) + 1;
    } else if (config.id === 'eagle' || config.id === 'turtle') {
      zIndexVal = Math.random() < 0.1 ? 15 : Math.floor(Math.random() * 5) + 1;
    }

    // Apply styles
    Object.assign(element.style, {
      left: position.x,
      top: position.y,
      opacity: '1',
      zIndex: zIndexVal?.toString() || '',
      ...getAnimationStyle(config.type, index, config.speed)
    });

    // Render React component into element
    const DesignComponent = design.component;
    const componentProps: any = { size };
    if (config.id === 'turtle') {
      componentProps.color = turtleColor;
    }

    const root = createRoot(element);
    root.render(<DesignComponent {...componentProps} />);

    container.appendChild(element);

    return {
      id: elementId,
      element,
      root,
      config,
      index,
      position,
      size
    };
  }, [isMobile, turtleColor, getAnimationStyle, generatePosition]);

  const createBubbleElement = useCallback((
    bubbleIndex: number,
    config: AnimationConfig,
    container: HTMLElement
  ): AnimationElement => {
    const element = document.createElement('div');
    element.className = 'absolute';

    const x = Math.random() * 100;
    const y = -10 + Math.random() * 20;
    const size = Math.random() * (config.size.max - config.size.min) + config.size.min;
    const duration = (Math.random() * 10 + 15) / config.speed;

    Object.assign(element.style, {
      left: `${x}%`,
      bottom: `${y}%`,
      animationName: 'bubble',
      animationDuration: `${duration}s`,
      animationIterationCount: 'infinite',
      animationTimingFunction: 'ease-in-out',
      animationDelay: `${Math.random() * 5}s`
    });

    const root = createRoot(element);
    root.render(
      <PixelatedBubble
        size={size}
        color={bubbleColor}
        className="pixelated"
      />
    );

    container.appendChild(element);

    return {
      id: `bubble-${bubbleIndex}`,
      element,
      root,
      config,
      index: bubbleIndex,
      position: { x: `${x}%`, y: `${y}%` },
      size
    };
  }, [bubbleColor]);

  // Detect theme changes and clear all animations
  const prevThemeRef = useRef<string>('');
  useEffect(() => {
    // Create a signature of the current theme based on which animations are configured
    const currentThemeSignature = [
      ...animations.characters.map(c => c.id).sort(),
      ...animations.decorations.map(d => d.id).sort()
    ].join(',');

    if (prevThemeRef.current && prevThemeRef.current !== currentThemeSignature) {
      // Theme changed - clear all existing animations
      elementsRef.current.forEach(el => {
        el.root?.unmount();
        el.element.remove();
      });
      elementsRef.current.clear();

      bubblesRef.current.forEach(bubble => {
        bubble.root?.unmount();
        bubble.element.remove();
      });
      bubblesRef.current.clear();

      // Reset prev configs
      prevConfigsRef.current = { characters: [], decorations: [], bubbles: null };
    }

    prevThemeRef.current = currentThemeSignature;
  }, [animations.characters, animations.decorations]);

  // Update bubbles when config changes (count/size/enabled, but NOT speed)
  useEffect(() => {
    if (!containerRef.current || !isVisible) return;

    const currentConfig = animations.bubbles;
    const prevConfig = prevConfigsRef.current.bubbles;

    if (!currentConfig?.enabled) {
      // Remove all bubbles
      bubblesRef.current.forEach(bubble => {
        bubble.root?.unmount();
        bubble.element.remove();
      });
      bubblesRef.current.clear();
      prevConfigsRef.current = {
        ...prevConfigsRef.current,
        bubbles: null
      };
      return;
    }

    const currentCount = isMobile ? Math.floor(currentConfig.count / 2) : currentConfig.count;
    const prevCount = prevConfig ? (isMobile ? Math.floor(prevConfig.count / 2) : prevConfig.count) : 0;

    let configChanged = false;

    // Handle count changes
    if (currentCount !== prevCount) {
      if (currentCount > prevCount) {
        // Add new bubbles
        for (let i = prevCount; i < currentCount; i++) {
          const bubble = createBubbleElement(i, currentConfig, containerRef.current);
          bubblesRef.current.set(i, bubble);
        }
      } else {
        // Remove excess bubbles
        for (let i = currentCount; i < prevCount; i++) {
          const bubble = bubblesRef.current.get(i);
          if (bubble) {
            bubble.root?.unmount();
            bubble.element.remove();
            bubblesRef.current.delete(i);
          }
        }
      }
      configChanged = true;
    }

    // Handle size changes (need to recreate bubbles)
    if (prevConfig && (currentConfig.size.min !== prevConfig.size.min || currentConfig.size.max !== prevConfig.size.max)) {
      bubblesRef.current.forEach((bubble, index) => {
        bubble.root?.unmount();
        bubble.element.remove();
        const newBubble = createBubbleElement(index, currentConfig, containerRef.current!);
        bubblesRef.current.set(index, newBubble);
      });
      configChanged = true;
    }

    // Only update prevConfigsRef if something actually changed
    if (configChanged || !prevConfig) {
      prevConfigsRef.current = {
        ...prevConfigsRef.current,
        bubbles: { ...currentConfig }
      };
    }

  }, [animations.bubbles?.enabled, animations.bubbles?.count, animations.bubbles?.size.min, animations.bubbles?.size.max, isMobile, isVisible, createBubbleElement, animations.bubbles]);

  // Separate effect to handle ONLY bubble speed changes via direct DOM manipulation
  useEffect(() => {
    if (!isVisible || !animations.bubbles?.enabled) return;

    const currentConfig = animations.bubbles;
    const prevConfig = prevConfigsRef.current.bubbles;

    // Handle speed changes (update duration without recreating)
    if (prevConfig && currentConfig.speed !== prevConfig.speed) {
      bubblesRef.current.forEach((bubble) => {
        const duration = (Math.random() * 10 + 15) / currentConfig.speed;
        bubble.element.style.animationDuration = `${duration}s`;
      });

      // Update only the bubble config in prevConfigsRef after applying changes
      prevConfigsRef.current = {
        ...prevConfigsRef.current,
        bubbles: { ...currentConfig }
      };
    }
  }, [animations.bubbles?.speed, isVisible, animations.bubbles]);

  // Update characters and decorations (count/enabled, but NOT speed)
  useEffect(() => {
    if (!containerRef.current || !isVisible) return;

    const allConfigs = [...animations.characters, ...animations.decorations];
    const prevConfigs = [...prevConfigsRef.current.characters, ...prevConfigsRef.current.decorations];

    let configChanged = false;

    allConfigs.forEach((config) => {
      const prevConfig = prevConfigs.find(c => c.id === config.id);

      if (!config.enabled) {
        // Remove all elements for this config
        let removed = false;
        elementsRef.current.forEach((el, key) => {
          if (el.config.id === config.id) {
            el.root?.unmount();
            el.element.remove();
            elementsRef.current.delete(key);
            removed = true;
          }
        });
        if (removed) configChanged = true;
        return;
      }

      const prevCount = prevConfig?.count || 0;
      const currentCount = config.count;

      // Handle count changes
      if (currentCount !== prevCount) {
        if (currentCount > prevCount) {
          // Add new elements
          for (let i = prevCount; i < currentCount; i++) {
            const animEl = createAnimationElement(config, i, containerRef.current!);
            if (animEl) {
              elementsRef.current.set(animEl.id, animEl);
            }
          }
        } else {
          // Remove excess elements
          for (let i = currentCount; i < prevCount; i++) {
            const elementId = `${config.id}-${i}`;
            const el = elementsRef.current.get(elementId);
            if (el) {
              el.root?.unmount();
              el.element.remove();
              elementsRef.current.delete(elementId);
            }
          }
        }
        configChanged = true;
      }
    });

    // Only update prevConfigsRef if something actually changed
    if (configChanged || prevConfigs.length === 0) {
      prevConfigsRef.current = {
        characters: animations.characters.map(c => ({ ...c })),
        decorations: animations.decorations.map(d => ({ ...d })),
        bubbles: prevConfigsRef.current.bubbles // Don't overwrite bubble config
      };
    }

  }, [
    ...animations.characters.map(c => `${c.id}-${c.enabled}-${c.count}`),
    ...animations.decorations.map(d => `${d.id}-${d.enabled}-${d.count}`),
    isVisible,
    createAnimationElement
  ]);

  // Separate effect to handle ONLY speed changes via direct DOM manipulation
  useEffect(() => {
    if (!isVisible) return;

    const allConfigs = [...animations.characters, ...animations.decorations];
    const prevConfigs = [...prevConfigsRef.current.characters, ...prevConfigsRef.current.decorations];

    allConfigs.forEach((config) => {
      const prevConfig = prevConfigs.find(c => c.id === config.id);

      // Handle speed changes (update without recreating)
      if (prevConfig && config.speed !== prevConfig.speed) {
        for (let i = 0; i < config.count; i++) {
          const elementId = `${config.id}-${i}`;
          updateSpeed(elementId, config.speed, config.type);
        }
      }
    });

    // Update prevConfigsRef with new speeds after applying changes
    prevConfigsRef.current = {
      characters: animations.characters.map(c => ({ ...c })),
      decorations: animations.decorations.map(d => ({ ...d })),
      bubbles: prevConfigsRef.current.bubbles, // Keep bubble config unchanged
    };
  }, [animations.characters.map(c => c.speed).join(','), animations.decorations.map(d => d.speed).join(','), isVisible, updateSpeed, animations.characters, animations.decorations]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      elementsRef.current.forEach(el => {
        el.root?.unmount();
        el.element.remove();
      });
      elementsRef.current.clear();

      bubblesRef.current.forEach(bubble => {
        bubble.root?.unmount();
        bubble.element.remove();
      });
      bubblesRef.current.clear();
    };
  }, []);

  return (
    <>
      <style>{`
        @keyframes float {
          0%, 100% { transform: translateY(0) rotate(0deg); }
          50% { transform: translateY(-20px) rotate(10deg); }
        }
        @keyframes swim {
          0%, 100% { transform: translateX(0) translateY(0) scaleX(1); }
          25% { transform: translateX(30px) translateY(-10px) scaleX(1); }
          50% { transform: translateX(0) translateY(-20px) scaleX(-1); }
          75% { transform: translateX(-30px) translateY(-10px) scaleX(-1); }
        }
        @keyframes bubble {
          0% { transform: translateY(0) translateX(0); opacity: 0.8; }
          100% { transform: translateY(-100vh) translateX(20px); opacity: 0; }
        }
        @keyframes fall {
          0% { transform: translateY(0) rotate(0deg); opacity: 0; }
          10% { opacity: 1; }
          90% { opacity: 1; }
          100% { transform: translateY(calc(100vh - 80px)) rotate(180deg); opacity: 0; }
        }
        @keyframes fadeIn {
          from { opacity: 0; }
          to { opacity: 1; }
        }
      `}</style>

      <div
        ref={containerRef}
        className="fixed inset-0 pointer-events-none overflow-hidden"
        style={{
          zIndex: zIndex,
          opacity: (isVisible && enabled) ? 1 : 0,
          animationName: (isVisible && enabled) ? 'fadeIn' : 'none',
          animationDuration: (isVisible && enabled) ? '0.3s' : '0s',
          animationTimingFunction: (isVisible && enabled) ? 'ease-in' : 'linear',
          display: enabled ? 'block' : 'none'
        }}
      />
    </>
  );
}

export default AnimationLayer;
