import { svg } from './dom';

type Name =
  | 'open'
  | 'search'
  | 'outline'
  | 'menu'
  | 'close'
  | 'up'
  | 'down'
  | 'check'
  | 'file';
const paths: Record<Name, string> = {
  open: 'M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v2M3 7h18l-3 12H3V7Z',
  search: 'm20 20-5-5M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0',
  outline: 'M8 5h13M8 12h13M8 19h13M3 5h.01M3 12h.01M3 19h.01',
  menu: 'M5 12h.01M12 12h.01M19 12h.01',
  close: 'm6 6 12 12M6 18 18 6',
  up: 'm6 14 6-6 6 6',
  down: 'm6 10 6 6 6-6',
  check: 'm5 12 4 4L19 6',
  file: 'M14 3H5v18h14V8l-5-5Zm0 0v6h5M8 13h8M8 17h6',
};
export function icon(name: Name): SVGElement {
  return svg(
    'svg',
    {
      width: '20',
      height: '20',
      viewBox: '0 0 24 24',
      fill: 'none',
      stroke: 'currentColor',
      'stroke-width': '1.7',
      'stroke-linecap': 'round',
      'stroke-linejoin': 'round',
      'aria-hidden': 'true',
    },
    svg('path', { d: paths[name] }),
  );
}
