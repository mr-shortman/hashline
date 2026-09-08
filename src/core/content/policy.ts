import DOMPurify from 'dompurify';
import { ALLOWED_ATTR, ALLOWED_TAGS } from '../markdown/opbuffer';
import type { ParsedMarkdown } from '../markdown/types';
import type { DocumentGateway, FileDocument } from '../../platform/gateway';

declare const clean: unique symbol;
export type SanitizedHtml = string & { readonly [clean]: true };
export interface CleanContent {
  html: SanitizedHtml;
  blockedImages: number;
}

export function classifyUrl(
  value: string,
): 'fragment' | 'external' | 'local' | 'blocked' {
  if (
    !value ||
    [...value].some(
      (char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127,
    )
  )
    return 'blocked';
  if (value.startsWith('#')) return 'fragment';
  if (/^(https?:|mailto:)/i.test(value)) {
    try {
      const url = new URL(value);
      return url.protocol === 'mailto:' || !!url.hostname
        ? 'external'
        : 'blocked';
    } catch {
      return 'blocked';
    }
  }
  if (
    /^[a-z][a-z\d+.-]*:/i.test(value) ||
    value.startsWith('//') ||
    value.includes('\\')
  )
    return 'blocked';
  return 'local';
}

// WebKit does not reliably paint alt text for an <img> without src.
// Keep the resource node for deferred loading; expose a real text placeholder.
export function unavailableImage(img: HTMLImageElement, message: string): void {
  img.removeAttribute('src');
  img.dataset.unavailable = '';
  img.hidden = true;
  img.setAttribute('aria-hidden', 'true');
  let label = img.nextElementSibling;
  if (!label?.classList.contains('image-placeholder')) {
    label = document.createElement('span');
    label.className = 'image-placeholder';
    label.setAttribute('data-search-ignore', '');
    img.after(label);
  }
  label.textContent = `${img.alt || 'Bild'} — ${message}`;
}

export interface SanitizedFragment {
  fragment: DocumentFragment;
  blockedImages: number;
  remoteImages: number;
  hasImages: boolean;
  tables: HTMLTableElement[];
  pres: HTMLPreElement[];
}

export function sanitizeFragment(
  parsed: ParsedMarkdown,
  file: FileDocument,
  gateway: DocumentGateway,
): SanitizedFragment {
  const fragment = DOMPurify.sanitize(parsed.html, {
    RETURN_DOM_FRAGMENT: true,
    // The op buffer encodes exactly these tables; a tag or attribute name
    // outside them is not expressible there at all (docs/decisions/007).
    ALLOWED_TAGS: [...ALLOWED_TAGS],
    ALLOWED_ATTR: [...ALLOWED_ATTR],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    FORBID_CONTENTS: [
      'script',
      'style',
      'iframe',
      'object',
      'embed',
      'form',
      'button',
      'textarea',
      'select',
      'svg',
      'math',
      'template',
    ],
  });
  // Only parser-generated heading IDs survive; even passive HTML cannot claim app IDs.
  const remaining = new Map(parsed.headings.map((h) => [h.id, h.level]));
  let blockedImages = 0;
  let remoteImages = 0;
  let hasImages = false;
  const tables: HTMLTableElement[] = [];
  const pres: HTMLPreElement[] = [];
  const rejectedInputs: Element[] = [];
  const unavailableImages: HTMLImageElement[] = [];
  const walker = document.createTreeWalker(fragment, NodeFilter.SHOW_ELEMENT);
  while (walker.nextNode()) {
    const element = walker.currentNode as Element;
    // Most parser elements have no attributes. Preserve the heading-ID and
    // attribute policy here, before the fragment can enter the live document.
    if (
      element.tagName === 'INPUT' ||
      (element.hasAttributes() &&
        element.matches(
          '[id], [class], [width], [height], [colspan], [rowspan]',
        ))
    ) {
      const id = element.getAttribute('id');
      if (id) {
        if (
          remaining.get(id) === Number(element.tagName.slice(1)) &&
          /^H[1-6]$/.test(element.tagName)
        )
          remaining.delete(id);
        else element.removeAttribute('id');
      }
      const cls = element.getAttribute('class');
      if (cls !== null) element.removeAttribute('class');
      if (
        element.tagName === 'CODE' &&
        cls &&
        /^language-[a-zA-Z0-9_+-]{1,40}$/.test(cls)
      )
        element.setAttribute('class', cls);
      if (element.tagName === 'INPUT') {
        if (element.getAttribute('type') !== 'checkbox') {
          rejectedInputs.push(element);
          continue;
        }
        element.setAttribute('disabled', '');
        element.setAttribute('tabindex', '-1');
      }
      for (const attr of ['width', 'height', 'colspan', 'rowspan']) {
        const value = element.getAttribute(attr);
        if (value && (!/^\d{1,4}$/.test(value) || Number(value) > 4096))
          element.removeAttribute(attr);
      }
    }
    switch (element.tagName) {
      case 'TABLE':
        tables.push(element as HTMLTableElement);
        break;
      case 'PRE':
        pres.push(element as HTMLPreElement);
        break;
      case 'A': {
        const link = element as HTMLAnchorElement;
        const href = link.getAttribute('href') || '';
        const kind = classifyUrl(href);
        if (kind === 'blocked') link.removeAttribute('href');
        else {
          link.setAttribute('data-link', href);
          link.setAttribute('href', '#');
        }
        if (kind === 'external')
          link.setAttribute('title', `${href} · In Systemanwendung öffnen`);
        break;
      }
      case 'IMG': {
        const img = element as HTMLImageElement;
        hasImages = true;
        const source = img.getAttribute('src') || '';
        img.removeAttribute('src');
        if (/^https?:/i.test(source) && classifyUrl(source) === 'external') {
          remoteImages++;
          img.dataset.remoteSource = source;
          img.dataset.remoteAlt = img.alt;
        }
        const url =
          classifyUrl(source) === 'local'
            ? gateway.imageUrl(file, source)
            : null;
        if (url && /^(hashline-image:\/\/localhost\/|blob:)/.test(url)) {
          img.src = url;
          img.loading = 'lazy';
          img.decoding = 'async';
        } else {
          blockedImages++;
          unavailableImages.push(img);
        }
        break;
      }
    }
  }
  // Tree edits are deferred until traversal ends: removing an input or inserting
  // a placeholder must never skip siblings or feed app-owned markup into policy.
  for (const input of rejectedInputs) input.remove();
  for (const img of unavailableImages)
    unavailableImage(img, 'Bildzugriff nicht freigegeben');
  return { fragment, blockedImages, remoteImages, hasImages, tables, pres };
}

// String adapter for contract comparisons; the renderer inserts the fragment
// directly, avoiding serialization and a second HTML parse.
export function sanitizeContent(
  parsed: ParsedMarkdown,
  file: FileDocument,
  gateway: DocumentGateway,
): CleanContent {
  const { fragment, blockedImages } = sanitizeFragment(parsed, file, gateway);
  const container = document.createElement('div');
  container.append(fragment);
  return { html: container.innerHTML as SanitizedHtml, blockedImages };
}
