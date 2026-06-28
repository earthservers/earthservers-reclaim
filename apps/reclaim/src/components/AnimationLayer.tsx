// Simplified Animation Layer for EarthServers Local
// Provides animated background effects based on theme

import { useEffect, useRef, useState } from 'react';

interface AnimationLayerProps {
  enabled?: boolean;
  theme?: 'ocean-turtle' | 'mountain-eagle' | 'sun-fire' | 'lightning-bolt' | 'air-clouds' | 'earthservers-default';
  primaryColor?: string;
  secondaryColor?: string;
}

// Particle configuration per theme
const THEME_PARTICLES: Record<string, { type: string; count: number; color: string; speed: number }[]> = {
  'ocean-turtle': [
    { type: 'bubble', count: 15, color: 'rgba(38, 198, 218, 0.4)', speed: 0.5 },
  ],
  'mountain-eagle': [
    { type: 'snow', count: 20, color: 'rgba(255, 255, 255, 0.3)', speed: 0.3 },
  ],
  'sun-fire': [
    { type: 'ember', count: 12, color: 'rgba(255, 152, 0, 0.5)', speed: 0.4 },
  ],
  'lightning-bolt': [
    { type: 'spark', count: 8, color: 'rgba(124, 77, 255, 0.5)', speed: 0.6 },
  ],
  'air-clouds': [
    { type: 'float', count: 10, color: 'rgba(100, 181, 246, 0.3)', speed: 0.2 },
  ],
  'earthservers-default': [
    { type: 'glow', count: 6, color: 'rgba(15, 171, 137, 0.3)', speed: 0.3 },
  ],
};

export function AnimationLayer({
  enabled = true,
  theme = 'earthservers-default',
  primaryColor,
  secondaryColor
}: AnimationLayerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>();
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    if (!enabled) {
      setIsVisible(false);
      return;
    }

    // Delay showing for smooth mount
    const timeout = setTimeout(() => setIsVisible(true), 100);
    return () => clearTimeout(timeout);
  }, [enabled]);

  useEffect(() => {
    if (!enabled || !canvasRef.current || !isVisible) return;

    const canvas = canvasRef.current;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Set canvas size
    const resize = () => {
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
    };
    resize();
    window.addEventListener('resize', resize);

    // Get particles for current theme
    const particleConfigs = THEME_PARTICLES[theme] || THEME_PARTICLES['earthservers-default'];

    // Create particles
    const particles: Array<{
      x: number;
      y: number;
      size: number;
      speedX: number;
      speedY: number;
      opacity: number;
      color: string;
      type: string;
    }> = [];

    particleConfigs.forEach(config => {
      for (let i = 0; i < config.count; i++) {
        particles.push({
          x: Math.random() * canvas.width,
          y: Math.random() * canvas.height,
          size: Math.random() * 4 + 2,
          speedX: (Math.random() - 0.5) * config.speed,
          speedY: config.type === 'bubble' ? -Math.random() * config.speed - 0.2 :
                  config.type === 'snow' || config.type === 'ember' ? Math.random() * config.speed + 0.1 :
                  (Math.random() - 0.5) * config.speed * 0.5,
          opacity: Math.random() * 0.5 + 0.3,
          color: config.color,
          type: config.type,
        });
      }
    });

    // Animation loop
    const animate = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);

      particles.forEach(p => {
        // Update position
        p.x += p.speedX;
        p.y += p.speedY;

        // Wrap around edges
        if (p.y < -10) p.y = canvas.height + 10;
        if (p.y > canvas.height + 10) p.y = -10;
        if (p.x < -10) p.x = canvas.width + 10;
        if (p.x > canvas.width + 10) p.x = -10;

        // Draw particle
        ctx.beginPath();
        ctx.globalAlpha = p.opacity;
        ctx.fillStyle = p.color;

        if (p.type === 'bubble') {
          // Circle for bubbles
          ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
          ctx.fill();
          // Inner highlight
          ctx.beginPath();
          ctx.globalAlpha = p.opacity * 0.5;
          ctx.fillStyle = 'rgba(255, 255, 255, 0.5)';
          ctx.arc(p.x - p.size * 0.3, p.y - p.size * 0.3, p.size * 0.3, 0, Math.PI * 2);
          ctx.fill();
        } else if (p.type === 'snow' || p.type === 'float') {
          // Soft circle
          const gradient = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, p.size);
          gradient.addColorStop(0, p.color);
          gradient.addColorStop(1, 'transparent');
          ctx.fillStyle = gradient;
          ctx.arc(p.x, p.y, p.size * 2, 0, Math.PI * 2);
          ctx.fill();
        } else if (p.type === 'ember' || p.type === 'spark') {
          // Glowing dot
          const gradient = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, p.size * 1.5);
          gradient.addColorStop(0, p.color);
          gradient.addColorStop(0.5, p.color.replace('0.5)', '0.2)'));
          gradient.addColorStop(1, 'transparent');
          ctx.fillStyle = gradient;
          ctx.arc(p.x, p.y, p.size * 2, 0, Math.PI * 2);
          ctx.fill();
        } else {
          // Default glow
          const gradient = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, p.size * 3);
          gradient.addColorStop(0, p.color);
          gradient.addColorStop(1, 'transparent');
          ctx.fillStyle = gradient;
          ctx.arc(p.x, p.y, p.size * 3, 0, Math.PI * 2);
          ctx.fill();
        }
      });

      ctx.globalAlpha = 1;
      animationRef.current = requestAnimationFrame(animate);
    };

    animate();

    return () => {
      window.removeEventListener('resize', resize);
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
    };
  }, [enabled, theme, isVisible, primaryColor, secondaryColor]);

  if (!enabled) return null;

  return (
    <canvas
      ref={canvasRef}
      className="fixed inset-0 pointer-events-none"
      style={{
        zIndex: 1,
        opacity: isVisible ? 1 : 0,
        transition: 'opacity 0.5s ease-in',
      }}
    />
  );
}

export default AnimationLayer;
