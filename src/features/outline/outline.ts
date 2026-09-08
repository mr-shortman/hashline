import type { Heading } from '../../core/markdown/types';
import { el } from '../../ui/dom';
import { icon } from '../../ui/icon';

export interface OutlineOptions {
  headings: readonly Heading[];
  active: string;
  close(): void;
  jump(id: string): void;
}
export interface Outline {
  nodes: HTMLElement[];
  setHeadings(headings: readonly Heading[]): void;
  setActive(id: string): void;
  destroy(): void;
}

export function createOutline({
  headings,
  active,
  close,
  jump,
}: OutlineOptions): Outline {
  const nav = el('nav', { 'aria-label': 'Abschnitte' });
  const panel = el(
    'aside',
    { class: 'outline', 'aria-label': 'Inhaltsverzeichnis' },
    el(
      'div',
      { class: 'panel-heading' },
      el('span', {}, 'In diesem Dokument'),
      el(
        'button',
        {
          class: 'icon-button',
          'aria-label': 'Inhaltsverzeichnis schließen',
          onclick: close,
        },
        icon('close'),
      ),
    ),
    nav,
  );
  const backdrop = el('button', {
    class: 'outline-backdrop',
    tabindex: '-1',
    'aria-label': 'Inhaltsverzeichnis schließen',
    onclick: close,
  });

  let links = new Map<string, HTMLButtonElement>();
  let current = active;
  const fill = (list: readonly Heading[]) => {
    links = new Map();
    if (!list.length) {
      nav.replaceChildren(
        el(
          'p',
          { class: 'muted panel-empty' },
          'Keine Überschriften vorhanden.',
        ),
      );
      return;
    }
    const fragment = document.createDocumentFragment();
    for (const heading of list) {
      const button = el(
        'button',
        {
          class:
            current === heading.id ? 'outline-link active' : 'outline-link',
          style: { paddingLeft: `${16 + (heading.level - 1) * 12}px` },
          'aria-current': current === heading.id ? 'location' : undefined,
          onclick: () => jump(heading.id),
        },
        heading.text || 'Ohne Titel',
      );
      links.set(heading.id, button);
      fragment.append(button);
    }
    nav.replaceChildren(fragment);
  };
  fill(headings);

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

  return {
    nodes: [backdrop, panel],
    setHeadings(list) {
      fill(list);
    },
    setActive(id) {
      if (id === current) return;
      links.get(current)?.classList.remove('active');
      links.get(current)?.removeAttribute('aria-current');
      current = id;
      links.get(current)?.classList.add('active');
      links.get(current)?.setAttribute('aria-current', 'location');
    },
    destroy() {
      background.forEach((element) => {
        element.inert = false;
      });
      media.removeEventListener('change', mode);
      panel.removeEventListener('keydown', trap);
      backdrop.remove();
      panel.remove();
    },
  };
}
