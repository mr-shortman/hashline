import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
import { DocumentController } from './features/document/controller';
import { WorkerMarkdownService } from './core/markdown/service';
import { createGateway, desktop } from './platform/tauri';
import './app/styles.css';

const gateway = createGateway();
const controller = new DocumentController(gateway, new WorkerMarkdownService());
if (import.meta.env.DEV && desktop) {
  let lastPath: string | undefined;
  controller.subscribe(() => {
    const path = controller.getSnapshot().document?.file.path;
    if (path && path !== lastPath) {
      lastPath = path;
      try {
        sessionStorage.setItem('hashline.dev.file', path);
      } catch {
        /* Optional dev convenience. */
      }
    }
  });
}
ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App controller={controller} gateway={gateway} />
  </React.StrictMode>,
);
window.addEventListener('pagehide', () => controller.dispose(), { once: true });
if (import.meta.hot) import.meta.hot.dispose(() => controller.dispose());
