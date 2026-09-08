import { useEffect, useRef } from 'react';
import { Icon } from '../../ui/Icon';

export function SearchBar({
  query,
  setQuery,
  count,
  active,
  pending,
  step,
  close,
}: {
  query: string;
  setQuery(value: string): void;
  count: number;
  active: number;
  pending: boolean;
  step(delta: number): void;
  close(): void;
}) {
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
  }, []);
  return (
    <div className="search-bar" role="search">
      <Icon name="search" />
      <input
        ref={input}
        aria-label="Dokument durchsuchen"
        placeholder="Im Dokument suchen …"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            step(e.shiftKey ? -1 : 1);
          }
        }}
      />
      <span className="search-count" role="status" aria-busy={pending}>
        {query
          ? pending
            ? 'Suche läuft …'
            : count
              ? `${active + 1} / ${count}`
              : 'Keine Treffer'
          : ''}
      </span>
      <button
        className="icon-button"
        aria-label="Vorheriger Treffer"
        disabled={pending || !count}
        onClick={() => step(-1)}
      >
        <Icon name="up" />
      </button>
      <button
        className="icon-button"
        aria-label="Nächster Treffer"
        disabled={pending || !count}
        onClick={() => step(1)}
      >
        <Icon name="down" />
      </button>
      <button
        className="icon-button"
        aria-label="Suche schließen"
        onClick={close}
      >
        <Icon name="close" />
      </button>
    </div>
  );
}
