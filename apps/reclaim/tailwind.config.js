/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        earth: {
          teal: '#0fab89',
          pink: '#e91e63',
          blue: '#0178C6',
          gold: '#ffd700',
          dark: '#0a0a0f',
        },
        // Dynamic theme colors via CSS variables
        theme: {
          primary: 'var(--color-primary, #0fab89)',
          secondary: 'var(--color-secondary, #e91e63)',
          accent: 'var(--color-accent, #0178C6)',
          text: 'var(--color-text, #f0f0f0)',
          background: 'var(--color-background, #0a0a0f)',
          card: 'var(--color-card, #1a1a2e)',
          navbar: 'var(--color-navbar, #0a0a0f)',
        },
      },
      backgroundImage: {
        'theme-gradient': 'var(--background-gradient)',
      },
      opacity: {
        card: 'var(--card-opacity, 0.8)',
        navbar: 'var(--navbar-opacity, 0.9)',
      },
    },
  },
  plugins: [],
};
