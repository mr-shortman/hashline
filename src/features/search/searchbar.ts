import { el } from '../../ui/dom';
import { icon } from '../../ui/icon';

export interface SearchBarOptions {
  query: string;
  setQuery(value: string): void;
  step(delta: number): void;
  close(): void;
}
export interface SearchBar {
  element: HTMLElement;
  setMatches(count: number, active: number, pending: boolean): void;
  focus(): void;
  destroy(): void;
}

export function createSearchBar({
  query,
  setQuery,
  step,
  close,
}: SearchBarOptions): SearchBar {
  const input = el('input', {
    'aria-label': 'Dokument durchsuchen',
    placeholder: 'Im Dokument suchen …',
    oninput: () => setQuery(input.value),
    onkeydown: ((event: KeyboardEvent) => {
      if (event.key !== 'Enter') return;
      event.preventDefault();
      step(event.shiftKey ? -1 : 1);
    }) as EventListener,
  });
  input.value = query;
  const count = el('span', {
    class: 'search-count',
    role: 'status',
    'aria-busy': 'false',
  });
  const previous = el(
    'button',
    {
      class: 'icon-button',
      'aria-label': 'Vorheriger Treffer',
      disabled: true,
      onclick: () => step(-1),
    },
    icon('up'),
  );
  const next = el(
    'button',
    {
      class: 'icon-button',
      'aria-label': 'Nächster Treffer',
      disabled: true,
      onclick: () => step(1),
    },
    icon('down'),
  );
  const element = el(
    'div',
    { class: 'search-bar', role: 'search' },
    icon('search'),
    input,
    count,
    previous,
    next,
    el(
      'button',
      { class: 'icon-button', 'aria-label': 'Suche schließen', onclick: close },
      icon('close'),
    ),
  );
  return {
    element,
    setMatches(total, active, pending) {
      count.setAttribute('aria-busy', String(pending));
      count.textContent = !input.value
        ? ''
        : pending
          ? 'Suche läuft …'
          : total
            ? `${active + 1} / ${total}`
            : 'Keine Treffer';
      previous.disabled = pending || !total;
      next.disabled = pending || !total;
    },
    focus() {
      input.focus();
    },
    destroy() {
      element.remove();
    },
  };
}
