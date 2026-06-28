/**
 * Native Event Handlers for WebKitGTK Compatibility
 *
 * WebKitGTK on Linux (especially with NVIDIA GPUs) has issues with React synthetic events
 * and CSS properties like backdrop-filter that can break click handling.
 *
 * These components use native DOM event listeners to bypass React's event system.
 */

import { useEffect, useRef, ReactNode, CSSProperties } from 'react';

interface NativeButtonProps {
  onClick: () => void;
  className?: string;
  title?: string;
  children: ReactNode;
  style?: CSSProperties;
  disabled?: boolean;
}

/**
 * Button component that uses native DOM events directly.
 * Use this for buttons that don't work in WebKitGTK.
 */
export function NativeButton({
  onClick,
  className,
  title,
  children,
  style,
  disabled = false,
}: NativeButtonProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const button = buttonRef.current;
    if (!button || disabled) return;

    const handleClick = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      console.log('NativeButton click:', title || 'unnamed');
      onClick();
    };

    const handleMouseDown = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
    };

    const handleMouseUp = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
    };

    // Add listeners in capture phase for priority
    button.addEventListener('click', handleClick, { capture: true });
    button.addEventListener('mousedown', handleMouseDown, { capture: true });
    button.addEventListener('mouseup', handleMouseUp, { capture: true });

    return () => {
      button.removeEventListener('click', handleClick, { capture: true });
      button.removeEventListener('mousedown', handleMouseDown, { capture: true });
      button.removeEventListener('mouseup', handleMouseUp, { capture: true });
    };
  }, [onClick, title, disabled]);

  return (
    <button
      ref={buttonRef}
      type="button"
      className={className}
      title={title}
      disabled={disabled}
      style={{
        ...style,
        pointerEvents: disabled ? 'none' : 'auto',
        position: 'relative',
        zIndex: 50,
      }}
    >
      {children}
    </button>
  );
}

interface NativeClickableDivProps {
  onClick: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  className?: string;
  title?: string;
  children: ReactNode;
  style?: CSSProperties;
}

/**
 * Clickable div component that uses native DOM events directly.
 * Use this for clickable divs (like tabs) that don't work in WebKitGTK.
 */
export function NativeClickableDiv({
  onClick,
  onContextMenu,
  className,
  title,
  children,
  style,
}: NativeClickableDivProps) {
  const divRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const div = divRef.current;
    if (!div) return;

    const handleClick = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      console.log('NativeClickableDiv click:', title || 'unnamed');
      onClick();
    };

    const handleMouseDown = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
    };

    div.addEventListener('click', handleClick, { capture: true });
    div.addEventListener('mousedown', handleMouseDown, { capture: true });

    return () => {
      div.removeEventListener('click', handleClick, { capture: true });
      div.removeEventListener('mousedown', handleMouseDown, { capture: true });
    };
  }, [onClick, title]);

  return (
    <div
      ref={divRef}
      className={className}
      title={title}
      onContextMenu={onContextMenu}
      style={{
        ...style,
        pointerEvents: 'auto',
        position: 'relative',
        zIndex: 10,
        cursor: 'pointer',
      }}
    >
      {children}
    </div>
  );
}

interface NativeAnchorProps {
  onClick: () => void;
  href?: string;
  className?: string;
  title?: string;
  children: ReactNode;
  style?: CSSProperties;
}

/**
 * Anchor-like component that uses native DOM events directly.
 * Use this for link-style buttons that don't work in WebKitGTK.
 */
export function NativeAnchor({
  onClick,
  href = '#',
  className,
  title,
  children,
  style,
}: NativeAnchorProps) {
  const anchorRef = useRef<HTMLAnchorElement>(null);

  useEffect(() => {
    const anchor = anchorRef.current;
    if (!anchor) return;

    const handleClick = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      console.log('NativeAnchor click:', title || 'unnamed');
      onClick();
    };

    anchor.addEventListener('click', handleClick, { capture: true });

    return () => {
      anchor.removeEventListener('click', handleClick, { capture: true });
    };
  }, [onClick, title]);

  return (
    <a
      ref={anchorRef}
      href={href}
      className={className}
      title={title}
      style={{
        ...style,
        pointerEvents: 'auto',
        position: 'relative',
        zIndex: 10,
        cursor: 'pointer',
      }}
    >
      {children}
    </a>
  );
}
