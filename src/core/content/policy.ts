import DOMPurify from 'dompurify';
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

export function sanitizeContent(
  parsed: ParsedMarkdown,
  file: FileDocument,
  gateway: DocumentGateway,
): CleanContent {
  const fragment = DOMPurify.sanitize(parsed.html, {
    RETURN_DOM_FRAGMENT: true,
    ALLOWED_TAGS: [
      'p',
      'h1',
      'h2',
      'h3',
      'h4',
      'h5',
      'h6',
      'em',
      'strong',
      'del',
      's',
      'code',
      'pre',
      'blockquote',
      'ul',
      'ol',
      'li',
      'hr',
      'br',
      'a',
      'img',
      'table',
      'thead',
      'tbody',
      'tfoot',
      'tr',
      'th',
      'td',
      'input',
      'sup',
      'sub',
      'kbd',
      'details',
      'summary',
      'div',
      'span',
      'dl',
      'dt',
      'dd',
    ],
    ALLOWED_ATTR: [
      'id',
      'href',
      'src',
      'alt',
      'title',
      'class',
      'start',
      'type',
      'checked',
      'disabled',
      'colspan',
      'rowspan',
      'align',
      'width',
      'height',
      'open',
    ],
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
  for (const element of fragment.querySelectorAll('*')) {
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
    element.removeAttribute('class');
    if (
      element.tagName === 'CODE' &&
      cls &&
      /^language-[a-zA-Z0-9_+-]{1,40}$/.test(cls)
    )
      element.setAttribute('class', cls);
    if (element.tagName === 'INPUT') {
      if (element.getAttribute('type') !== 'checkbox') {
        element.remove();
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
  for (const link of fragment.querySelectorAll('a')) {
    const href = link.getAttribute('href') || '';
    const kind = classifyUrl(href);
    if (kind === 'blocked') link.removeAttribute('href');
    else {
      link.setAttribute('data-link', href);
      link.setAttribute('href', '#');
    }
    if (kind === 'external')
      link.setAttribute('title', `${href} · In Systemanwendung öffnen`);
  }
  let blockedImages = 0;
  for (const img of fragment.querySelectorAll('img')) {
    const source = img.getAttribute('src') || '';
    img.removeAttribute('src');
    const url =
      classifyUrl(source) === 'local' ? gateway.imageUrl(file, source) : null;
    if (url && /^(hashline-image:\/\/localhost\/|blob:)/.test(url)) {
      img.src = url;
      img.loading = 'lazy';
      img.decoding = 'async';
    } else {
      blockedImages++;
      img.setAttribute('data-unavailable', '');
      img.alt = `${img.alt || 'Bild'} — Bildzugriff nicht freigegeben`;
    }
  }
  const container = document.createElement('div');
  container.append(fragment);
  return { html: container.innerHTML as SanitizedHtml, blockedImages };
}
