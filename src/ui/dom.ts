// Minimal imperative element helpers. They replace JSX without introducing a
// framework: every region below owns its nodes and mutates them directly.
// Attribute values of `false`, `null` and `undefined` are dropped, `true`
// becomes an empty attribute, and `on…` keys register listeners.
export type Attrs = Record<string, unknown>;
export type Child = Node | string | false | null | undefined;

function apply(node: Element, attrs: Attrs): void {
  for (const [key, value] of Object.entries(attrs)) {
    if (key.startsWith('on') && typeof value === 'function') {
      node.addEventListener(
        key.slice(2),
        value as EventListenerOrEventListenerObject,
      );
    } else if (key === 'style' && value && typeof value === 'object') {
      Object.assign((node as HTMLElement).style, value);
    } else if (value === false || value === null || value === undefined) {
      continue;
    } else if (value === true) {
      node.setAttribute(key, '');
    } else {
      node.setAttribute(key, String(value));
    }
  }
}

function fill(node: Element, children: Child[]): void {
  for (const child of children) {
    if (child === false || child === null || child === undefined) continue;
    node.append(child);
  }
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  apply(node, attrs);
  fill(node, children);
  return node;
}

const SVG = 'http://www.w3.org/2000/svg';

export function svg(
  tag: string,
  attrs: Attrs = {},
  ...children: Child[]
): SVGElement {
  const node = document.createElementNS(SVG, tag) as SVGElement;
  apply(node, attrs);
  fill(node, children);
  return node;
}

// Replace a slot's content without disturbing its siblings.
export function swap(slot: Element, node: Node | null): void {
  if (node) slot.replaceChildren(node);
  else slot.replaceChildren();
}
