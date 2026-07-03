// EarthMultiMedia ImageViewer Component
// Privacy-focused image viewer with zoom, pan, and comparison features

import { useState, useRef, useEffect, useCallback } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { fsrUpscaleToCanvas } from '../../lib/fsrCanvas';

interface ImageViewerProps {
  source: string;
  title?: string;
  onError?: (error: string) => void;
  onLoad?: () => void;
  onNext?: () => void;  // Navigate to next image
  onPrev?: () => void;  // Navigate to previous image
  className?: string;
  showControls?: boolean;
  /// Show the built-in title/dimensions overlay (top-left). Off when the parent
  /// already labels the image (pane indicator / fullscreen header) to avoid a
  /// duplicated title stacked in the corner.
  showTitle?: boolean;
  /// Super-resolve the photo (FSR shaders on a WebGL canvas — the photo twin of
  /// the video pipeline's Enhance). Falls back to the plain image on failure.
  enhance?: boolean;
  /// RCAS sharpening in stops (0 = sharpest, 2 = softest); mirrors the video
  /// pipeline's Enhance sharpness setting. Default 0.2 (the FSR default).
  sharpness?: number;
}

interface ImageState {
  scale: number;
  translateX: number;
  translateY: number;
  rotation: number;
  isLoading: boolean;
  error: string | null;
  naturalWidth: number;
  naturalHeight: number;
}

const MIN_SCALE = 0.1;
const MAX_SCALE = 10;
const ZOOM_STEP = 0.25;

export function ImageViewer({
  source,
  title,
  onError,
  onLoad,
  onNext,
  onPrev,
  className = '',
  showControls = true,
  showTitle = true,
  enhance = false,
  sharpness = 0.2,
}: ImageViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  // FSR-enhanced render of the current photo. 0x0 = not available/off → the
  // plain <img> shows. The canvas mirrors the img's transform, with its scale
  // compensated so toggling never changes the on-screen size.
  const enhanceCanvasRef = useRef<HTMLCanvasElement>(null);
  const [enhancedSize, setEnhancedSize] = useState<{ w: number; h: number } | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [showOverlay, setShowOverlay] = useState(true);
  const overlayTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Resolve the source to something the <img> can actually load.
  //
  // Local files are read through the fs plugin into a blob: URL rather than
  // convertFileSrc: blob URLs are same-origin in EVERY window, while the
  // asset:// protocol only loads in `tauri://` pages — packaged secondary
  // windows (new window / tray / detached tabs) are served from the localhost
  // asset server (an http origin, see app_content_url), where WebKit rejects
  // asset:// subresources and every photo showed "Failed to load image".
  // The asset protocol stays as a fallback if the fs read fails (e.g. the
  // path is outside the fs scope).
  const [imageSrc, setImageSrc] = useState('');
  useEffect(() => {
    const filePath = source.startsWith('file://')
      ? source.replace('file://', '')
      : source.startsWith('/')
        ? source
        : null;
    if (!filePath) {
      // HTTP/HTTPS, blob and data URLs work directly (empty source stays empty).
      setImageSrc(source);
      return;
    }
    let cancelled = false;
    let objectUrl: string | null = null;
    (async () => {
      try {
        const { readFile } = await import('@tauri-apps/plugin-fs');
        const data = await readFile(filePath);
        const ext = (filePath.split('.').pop() || '').toLowerCase();
        const mime: Record<string, string> = {
          jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', gif: 'image/gif',
          webp: 'image/webp', bmp: 'image/bmp', svg: 'image/svg+xml',
          avif: 'image/avif', ico: 'image/x-icon', tif: 'image/tiff', tiff: 'image/tiff',
        };
        objectUrl = URL.createObjectURL(new Blob([data], { type: mime[ext] || 'application/octet-stream' }));
        if (cancelled) {
          URL.revokeObjectURL(objectUrl);
        } else {
          setImageSrc(objectUrl);
        }
      } catch (err) {
        console.warn('[ImageViewer] fs read failed, falling back to asset protocol:', err);
        if (!cancelled) setImageSrc(convertFileSrc(filePath));
      }
    })();
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [source]);

  const [state, setState] = useState<ImageState>({
    scale: 1,
    translateX: 0,
    translateY: 0,
    rotation: 0,
    isLoading: true,
    error: null,
    naturalWidth: 0,
    naturalHeight: 0,
  });

  // Reset transform
  const resetTransform = useCallback(() => {
    setState(s => ({
      ...s,
      scale: 1,
      translateX: 0,
      translateY: 0,
      rotation: 0,
    }));
  }, []);

  // Zoom in
  const zoomIn = useCallback(() => {
    setState(s => ({
      ...s,
      scale: Math.min(MAX_SCALE, s.scale + ZOOM_STEP),
    }));
  }, []);

  // Zoom out
  const zoomOut = useCallback(() => {
    setState(s => ({
      ...s,
      scale: Math.max(MIN_SCALE, s.scale - ZOOM_STEP),
    }));
  }, []);

  // Fit to container
  const fitToContainer = useCallback(() => {
    const container = containerRef.current;
    if (!container || !state.naturalWidth || !state.naturalHeight) return;

    const containerRect = container.getBoundingClientRect();
    const scaleX = containerRect.width / state.naturalWidth;
    const scaleY = containerRect.height / state.naturalHeight;
    // Fill the pane as much as possible while preserving aspect ratio (allow
    // scaling small images up so the photo takes the max available space).
    const scale = Math.min(scaleX, scaleY);

    setState(s => ({
      ...s,
      scale,
      translateX: 0,
      translateY: 0,
    }));
  }, [state.naturalWidth, state.naturalHeight]);

  // Rotate
  const rotate = useCallback((degrees: number) => {
    setState(s => ({
      ...s,
      rotation: (s.rotation + degrees) % 360,
    }));
  }, []);

  // Handle wheel zoom
  const handleWheel = useCallback((e: WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP;
    setState(s => ({
      ...s,
      scale: Math.max(MIN_SCALE, Math.min(MAX_SCALE, s.scale + delta)),
    }));
  }, []);

  // Handle mouse down for drag
  const handleMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return; // Only left click
    setIsDragging(true);
    setDragStart({
      x: e.clientX - state.translateX,
      y: e.clientY - state.translateY,
    });
  };

  // Handle mouse move for drag
  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!isDragging) return;
    setState(s => ({
      ...s,
      translateX: e.clientX - dragStart.x,
      translateY: e.clientY - dragStart.y,
    }));
  }, [isDragging, dragStart]);

  // Handle mouse up to end drag
  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // Handle touch events for mobile
  const handleTouchStart = (e: React.TouchEvent) => {
    if (e.touches.length === 1) {
      const touch = e.touches[0];
      setIsDragging(true);
      setDragStart({
        x: touch.clientX - state.translateX,
        y: touch.clientY - state.translateY,
      });
    }
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    if (!isDragging || e.touches.length !== 1) return;
    const touch = e.touches[0];
    setState(s => ({
      ...s,
      translateX: touch.clientX - dragStart.x,
      translateY: touch.clientY - dragStart.y,
    }));
  };

  const handleTouchEnd = () => {
    setIsDragging(false);
  };

  // Set up event listeners
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.addEventListener('wheel', handleWheel, { passive: false });
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);

    return () => {
      container.removeEventListener('wheel', handleWheel);
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [handleWheel, handleMouseMove, handleMouseUp]);

  // Auto-hide overlay after inactivity
  const resetOverlayTimeout = useCallback(() => {
    setShowOverlay(true);
    if (overlayTimeoutRef.current) {
      clearTimeout(overlayTimeoutRef.current);
    }
    overlayTimeoutRef.current = setTimeout(() => {
      setShowOverlay(false);
    }, 3000);
  }, []);

  // Cleanup overlay timeout
  useEffect(() => {
    return () => {
      if (overlayTimeoutRef.current) {
        clearTimeout(overlayTimeoutRef.current);
      }
    };
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      switch (e.key) {
        case '+':
        case '=':
          e.preventDefault();
          zoomIn();
          break;
        case '-':
          e.preventDefault();
          zoomOut();
          break;
        case '0':
          e.preventDefault();
          resetTransform();
          break;
        case 'f':
          e.preventDefault();
          fitToContainer();
          break;
        case 'r':
          e.preventDefault();
          rotate(90);
          break;
        case 'R':
          e.preventDefault();
          rotate(-90);
          break;
        case 'ArrowLeft':
          e.preventDefault();
          if (onPrev) {
            onPrev();
          } else {
            // Dispatch event for parent to handle navigation
            window.dispatchEvent(new CustomEvent('media-prev'));
          }
          break;
        case 'ArrowRight':
          e.preventDefault();
          if (onNext) {
            onNext();
          } else {
            // Dispatch event for parent to handle navigation
            window.dispatchEvent(new CustomEvent('media-next'));
          }
          break;
        case 'ArrowUp':
          e.preventDefault();
          zoomIn();
          break;
        case 'ArrowDown':
          e.preventDefault();
          zoomOut();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [zoomIn, zoomOut, resetTransform, fitToContainer, rotate, onNext, onPrev]);

  // Handle image load
  const handleImageLoad = (e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    const nw = img.naturalWidth;
    const nh = img.naturalHeight;

    // Compute the fit scale here from the freshly-loaded dimensions. (Calling
    // fitToContainer via setTimeout used a stale closure where naturalWidth was
    // still 0, so it bailed out — which is why the photo only fit after clicking
    // "fit to view".) Apply the fit in the same state update.
    let scale = 1;
    const container = containerRef.current;
    if (container && nw && nh) {
      const rect = container.getBoundingClientRect();
      scale = Math.min(rect.width / nw, rect.height / nh);
    }

    setState(s => ({
      ...s,
      isLoading: false,
      error: null,
      naturalWidth: nw,
      naturalHeight: nh,
      scale,
      translateX: 0,
      translateY: 0,
    }));
    onLoad?.();
  };

  // Handle image error
  const handleImageError = () => {
    const error = 'Failed to load image';
    setState(s => ({ ...s, isLoading: false, error }));
    onError?.(error);
  };

  // Load new source
  useEffect(() => {
    setState(s => ({ ...s, isLoading: true, error: null }));
    setEnhancedSize(null);
    resetTransform();
  }, [source, resetTransform]);

  // Super-resolve the loaded photo when Enhance is on. One-shot per
  // (photo, toggle); any failure just keeps the plain image.
  useEffect(() => {
    if (!enhance) {
      setEnhancedSize(null);
      return;
    }
    const img = imageRef.current;
    const canvas = enhanceCanvasRef.current;
    if (state.isLoading || state.error || !img || !canvas || !state.naturalWidth) return;
    if (fsrUpscaleToCanvas(img, canvas, sharpness)) {
      setEnhancedSize({ w: canvas.width, h: canvas.height });
    } else {
      setEnhancedSize(null);
    }
  }, [enhance, sharpness, state.isLoading, state.error, state.naturalWidth, source]);

  return (
    <div
      ref={containerRef}
      className={`relative bg-black/90 overflow-hidden select-none ${className}`}
      onMouseDown={handleMouseDown}
      onMouseMove={resetOverlayTimeout}
      onTouchStart={handleTouchStart}
      onTouchMove={handleTouchMove}
      onTouchEnd={handleTouchEnd}
      style={{ cursor: isDragging ? 'grabbing' : 'grab' }}
    >
      {/* Loading Spinner */}
      {state.isLoading && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="w-12 h-12 border-4 border-[var(--primary-color)] border-t-transparent rounded-full animate-spin" />
        </div>
      )}

      {/* Error Display */}
      {state.error && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="text-center p-6">
            <svg className="w-16 h-16 mx-auto text-red-500 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <p className="text-white text-lg">{state.error}</p>
          </div>
        </div>
      )}

      {/* Image */}
      <img
        ref={imageRef}
        // undefined (not "") while the blob is being read — an empty string src
        // resolves to the page URL and fires a spurious onError.
        src={imageSrc || undefined}
        alt={title || 'Image'}
        className="max-w-none"
        style={{
          transform: `translate(${state.translateX}px, ${state.translateY}px) scale(${state.scale}) rotate(${state.rotation}deg)`,
          transformOrigin: 'center center',
          position: 'absolute',
          left: '50%',
          top: '50%',
          marginLeft: `-${state.naturalWidth / 2}px`,
          marginTop: `-${state.naturalHeight / 2}px`,
          // Stay hidden until the freshly-loaded image has been fitted (scale +
          // centering applied in handleImageLoad's single setState). Otherwise the
          // new photo paints once at natural size, top-left, before the fit lands —
          // the "spawn full size then snap to fit" flash on each slideshow change.
          // Also hidden while the FSR-enhanced canvas is showing in its place.
          opacity: state.isLoading || (enhance && enhancedSize) ? 0 : 1,
          transition: 'opacity 120ms ease-out', // only opacity; transform never animates
        }}
        onLoad={handleImageLoad}
        onError={handleImageError}
        draggable={false}
      />

      {/* FSR-enhanced render — replaces the <img> visually when Enhance is on.
          Same transform pipeline; the scale is divided by the upscale factor so
          the on-screen size is identical, just with 2x the pixels behind it. */}
      <canvas
        ref={enhanceCanvasRef}
        style={{
          transform: `translate(${state.translateX}px, ${state.translateY}px) scale(${
            enhancedSize ? state.scale * (state.naturalWidth / enhancedSize.w) : state.scale
          }) rotate(${state.rotation}deg)`,
          transformOrigin: 'center center',
          position: 'absolute',
          left: '50%',
          top: '50%',
          marginLeft: `-${(enhancedSize?.w ?? 0) / 2}px`,
          marginTop: `-${(enhancedSize?.h ?? 0) / 2}px`,
          opacity: !state.isLoading && enhance && enhancedSize ? 1 : 0,
          pointerEvents: 'none',
        }}
      />

      {/* Controls */}
      {showControls && !state.isLoading && !state.error && (
        <div className={`absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-2 bg-black/60 backdrop-blur-sm rounded-full px-4 py-2 transition-opacity duration-300 ${showOverlay ? 'opacity-100' : 'opacity-0 pointer-events-none'}`}>
          {/* Zoom Out */}
          <button
            onClick={zoomOut}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Zoom out (-)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM13 10H7" />
            </svg>
          </button>

          {/* Zoom Level */}
          <span className="text-white text-sm font-mono min-w-[50px] text-center">
            {Math.round(state.scale * 100)}%
          </span>

          {/* Zoom In */}
          <button
            onClick={zoomIn}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Zoom in (+)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM10 7v6m3-3H7" />
            </svg>
          </button>

          <div className="w-px h-6 bg-white/30" />

          {/* Fit to Container */}
          <button
            onClick={fitToContainer}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Fit to view (F)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
            </svg>
          </button>

          {/* Reset */}
          <button
            onClick={resetTransform}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Reset (0)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>

          <div className="w-px h-6 bg-white/30" />

          {/* Rotate Left */}
          <button
            onClick={() => rotate(-90)}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Rotate left (Shift+R)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a8 8 0 018 8v2M3 10l6 6m-6-6l6-6" />
            </svg>
          </button>

          {/* Rotate Right */}
          <button
            onClick={() => rotate(90)}
            className="p-2 text-white hover:text-[var(--primary-color)] transition-colors"
            title="Rotate right (R)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 10h-10a8 8 0 00-8 8v2m18-10l-6 6m6-6l-6-6" />
            </svg>
          </button>
        </div>
      )}

      {/* Image Info - auto-hides with controls */}
      {showControls && showTitle && title && !state.isLoading && !state.error && (
        <div className={`absolute top-4 left-4 bg-black/60 backdrop-blur-sm rounded-lg px-3 py-2 transition-opacity duration-300 ${showOverlay ? 'opacity-100' : 'opacity-0 pointer-events-none'}`}>
          <p className="text-white text-sm font-medium truncate max-w-[200px]">{title}</p>
          <p className="text-gray-400 text-xs">
            {state.naturalWidth} x {state.naturalHeight}
          </p>
        </div>
      )}
    </div>
  );
}

export default ImageViewer;
