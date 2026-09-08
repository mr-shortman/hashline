import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
import { DocumentController } from './features/document/controller';
import { WorkerMarkdownService } from './core/markdown/service';
import { createGateway } from './platform/tauri';
import './app/styles.css';

const gateway = createGateway();
const controller = new DocumentController(gateway, new WorkerMarkdownService());
ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App controller={controller} gateway={gateway} />
  </React.StrictMode>,
);
window.addEventListener('pagehide', () => controller.dispose(), { once: true });
if (import.meta.hot) import.meta.hot.dispose(() => controller.dispose());
