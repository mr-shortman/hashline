import { useEffect, useRef } from 'react';
import type { Heading } from '../../core/markdown/types';
import { Icon } from '../../ui/Icon';

export function Outline({
  headings,
  active,
  close,
  jump,
}: {
  headings: readonly Heading[];
  active: string;
  close(): void;
  jump(id: string): void;
}) {
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    root.current?.querySelector<HTMLButtonElement>('button')?.focus();
    const panel = root.current!;
    const media = window.matchMedia('(max-width: 899px)');
    const background = Array.from(
      document.querySelectorAll<HTMLElement>(
        '.titlebar, .search-bar, .viewport-shell, .settings, .error-banner',
      ),
    );
    const mode = () => {
      background.forEach((element) => {
        element.inert = media.matches;
      });
      if (media.matches) {
        panel.setAttribute('role', 'dialog');
        panel.setAttribute('aria-modal', 'true');
      } else {
        panel.removeAttribute('role');
        panel.removeAttribute('aria-modal');
      }
    };
    const trap = (event: KeyboardEvent) => {
      if (!media.matches || event.key !== 'Tab') return;
      const buttons = Array.from(panel.querySelectorAll('button'));
      const first = buttons[0];
      const last = buttons[buttons.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    mode();
    media.addEventListener('change', mode);
    panel.addEventListener('keydown', trap);
    return () => {
      background.forEach((element) => {
        element.inert = false;
      });
      media.removeEventListener('change', mode);
      panel.removeEventListener('keydown', trap);
    };
  }, []);
  return (
    <>
      <button
        className="outline-backdrop"
        tabIndex={-1}
        aria-label="Inhaltsverzeichnis schließen"
        onClick={close}
      />
      <aside className="outline" ref={root} aria-label="Inhaltsverzeichnis">
        <div className="panel-heading">
          <span>In diesem Dokument</span>
          <button
            className="icon-button"
            aria-label="Inhaltsverzeichnis schließen"
            onClick={close}
          >
            <Icon name="close" />
          </button>
        </div>
        <nav aria-label="Abschnitte">
          {headings.length ? (
            headings.map((h) => (
              <button
                key={h.id}
                className={
                  active === h.id ? 'outline-link active' : 'outline-link'
                }
                style={{ paddingLeft: `${16 + (h.level - 1) * 12}px` }}
                aria-current={active === h.id ? 'location' : undefined}
                onClick={() => jump(h.id)}
              >
                {h.text || 'Ohne Titel'}
              </button>
            ))
          ) : (
            <p className="muted panel-empty">Keine Überschriften vorhanden.</p>
          )}
        </nav>
      </aside>
    </>
  );
}
