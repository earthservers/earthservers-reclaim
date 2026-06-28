import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App.tsx';
import './index.css';

// Mount React app with error handling
function mountApp() {
  const rootElement = document.getElementById('root');

  if (!rootElement) {
    console.error('[Reclaim] Root element not found!');
    return;
  }

  try {
    console.log('[Reclaim] Mounting React app...');
    const root = ReactDOM.createRoot(rootElement);
    root.render(
      <React.StrictMode>
        <App />
      </React.StrictMode>
    );
    console.log('[Reclaim] React app mounted successfully');
    // Signal to the HTML fallback that we mounted
    if (typeof window !== 'undefined' && (window as any).__reclaimMarkMounted) {
      (window as any).__reclaimMarkMounted();
    }
  } catch (error) {
    console.error('[Reclaim] Failed to mount React app:', error);
    // Show error in the UI
    rootElement.innerHTML = `
      <div style="display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100vh; color: white; font-family: system-ui, sans-serif; background: #0a0a0f;">
        <div style="font-size: 2rem; font-weight: bold; margin-bottom: 1rem; color: #ef4444;">Error</div>
        <div style="color: #888; max-width: 400px; text-align: center;">Failed to load application. Please refresh the page.</div>
        <button onclick="location.reload()" style="margin-top: 1rem; padding: 0.5rem 1rem; background: #3b82f6; color: white; border: none; border-radius: 0.5rem; cursor: pointer;">Refresh</button>
      </div>
    `;
  }
}

// Ensure DOM is ready before mounting
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', mountApp);
} else {
  // DOM already loaded, mount immediately
  mountApp();
}
