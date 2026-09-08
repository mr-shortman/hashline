import { createApp } from './app/app';
import { DocumentController } from './features/document/controller';
import { WorkerMarkdownService } from './core/markdown/service';
import { createGateway, desktop } from './platform/tauri';
import './app/styles.css';

performance.mark('hashline.frontend-ready');
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
const app = createApp(controller, gateway);
document.getElementById('root')!.append(app.element);
window.addEventListener('pagehide', () => controller.dispose(), { once: true });
if (import.meta.hot)
  import.meta.hot.dispose(() => {
    app.destroy();
    controller.dispose();
  });
