import { useState, useRef, useCallback, useEffect, ReactNode } from 'react';

interface DraggableModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  defaultPosition?: { x: number; y: number };
  defaultSize?: { width: number; height: number };
  minSize?: { width: number; height: number };
  resizable?: boolean;
  className?: string;
}

export function DraggableModal({
  isOpen,
  onClose,
  title,
  children,
  defaultPosition = { x: 100, y: 100 },
  defaultSize = { width: 400, height: 500 },
  minSize = { width: 300, height: 200 },
  resizable = true,
  className = '',
}: DraggableModalProps) {
  const [position, setPosition] = useState(defaultPosition);
  const [size, setSize] = useState(defaultSize);
  const [isDragging, setIsDragging] = useState(false);
  const [isResizing, setIsResizing] = useState(false);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [resizeStart, setResizeStart] = useState({ x: 0, y: 0, width: 0, height: 0 });
  const modalRef = useRef<HTMLDivElement>(null);

  // Handle drag start
  const handleDragStart = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest('.modal-content')) return;
    e.preventDefault();
    setIsDragging(true);
    setDragOffset({
      x: e.clientX - position.x,
      y: e.clientY - position.y,
    });
  }, [position]);

  // Handle resize start
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsResizing(true);
    setResizeStart({
      x: e.clientX,
      y: e.clientY,
      width: size.width,
      height: size.height,
    });
  }, [size]);

  // Handle mouse move
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging) {
        const newX = Math.max(0, Math.min(window.innerWidth - size.width, e.clientX - dragOffset.x));
        const newY = Math.max(0, Math.min(window.innerHeight - size.height, e.clientY - dragOffset.y));
        setPosition({ x: newX, y: newY });
      }
      if (isResizing) {
        const deltaX = e.clientX - resizeStart.x;
        const deltaY = e.clientY - resizeStart.y;
        const newWidth = Math.max(minSize.width, resizeStart.width + deltaX);
        const newHeight = Math.max(minSize.height, resizeStart.height + deltaY);
        setSize({ width: newWidth, height: newHeight });
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      setIsResizing(false);
    };

    if (isDragging || isResizing) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    }

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, isResizing, dragOffset, resizeStart, minSize, size.width, size.height]);

  // Reset position when opened
  useEffect(() => {
    if (isOpen) {
      // Center if first time or off screen
      const centerX = Math.max(0, (window.innerWidth - defaultSize.width) / 2);
      const centerY = Math.max(0, (window.innerHeight - defaultSize.height) / 2);
      if (position.x > window.innerWidth || position.y > window.innerHeight) {
        setPosition({ x: centerX, y: centerY });
      }
    }
  }, [isOpen, defaultSize.width, defaultSize.height, position.x, position.y]);

  if (!isOpen) return null;

  return (
    <div
      ref={modalRef}
      className={`fixed z-50 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl flex flex-col ${className}`}
      style={{
        left: position.x,
        top: position.y,
        width: size.width,
        height: size.height,
      }}
    >
      {/* Draggable Header */}
      <div
        className="flex items-center justify-between px-4 py-3 border-b border-gray-700 cursor-move select-none bg-black/20 rounded-t-xl"
        onMouseDown={handleDragStart}
      >
        <h3 className="font-semibold text-[var(--text-color)]">{title}</h3>
        <button
          onClick={onClose}
          className="p-1 text-gray-400 hover:text-white hover:bg-gray-700/50 rounded transition-colors"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* Content */}
      <div className="modal-content flex-1 overflow-auto p-4">
        {children}
      </div>

      {/* Resize Handle */}
      {resizable && (
        <div
          className="absolute bottom-0 right-0 w-4 h-4 cursor-se-resize"
          onMouseDown={handleResizeStart}
        >
          <svg className="w-4 h-4 text-gray-500" viewBox="0 0 24 24" fill="currentColor">
            <path d="M22 22H20V20H22V22ZM22 18H20V16H22V18ZM18 22H16V20H18V22ZM22 14H20V12H22V14ZM18 18H16V16H18V18ZM14 22H12V20H14V22Z" />
          </svg>
        </div>
      )}
    </div>
  );
}

export default DraggableModal;
