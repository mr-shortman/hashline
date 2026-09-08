let engine:
  Promise<(typeof import('highlight.js/lib/core'))['default']> | undefined;
const aliases: Record<string, string> = {
  js: 'javascript',
  ts: 'typescript',
  sh: 'bash',
  shell: 'bash',
  py: 'python',
  rs: 'rust',
  html: 'xml',
  yml: 'yaml',
};
export function languageOf(code: HTMLElement): string | undefined {
  const language = Array.from(code.classList)
    .find((c) => c.startsWith('language-'))
    ?.slice(9)
    .toLowerCase();
  return language ? aliases[language] || language : undefined;
}
const loaders: Record<
  string,
  () => Promise<{ default: import('highlight.js').LanguageFn }>
> = {
  javascript: () => import('highlight.js/lib/languages/javascript'),
  typescript: () => import('highlight.js/lib/languages/typescript'),
  json: () => import('highlight.js/lib/languages/json'),
  bash: () => import('highlight.js/lib/languages/bash'),
  python: () => import('highlight.js/lib/languages/python'),
  rust: () => import('highlight.js/lib/languages/rust'),
  css: () => import('highlight.js/lib/languages/css'),
  xml: () => import('highlight.js/lib/languages/xml'),
  yaml: () => import('highlight.js/lib/languages/yaml'),
};
const loading = new Map<string, Promise<void>>();
async function load(language: string) {
  engine ??= import('highlight.js/lib/core').then((module) => module.default);
  const hljs = await engine;
  if (!hljs.getLanguage(language)) {
    let pending = loading.get(language);
    if (!pending) {
      pending = loaders[language]()
        .then((module) => {
          hljs.registerLanguage(language, module.default);
        })
        .finally(() => loading.delete(language));
      loading.set(language, pending);
    }
    await pending;
  }
  return hljs;
}
export async function highlightCode(
  code: HTMLElement,
  allowed: () => boolean = () => true,
): Promise<boolean> {
  const language = languageOf(code);
  if (
    !language ||
    !Object.hasOwn(loaders, language) ||
    (code.textContent?.length || 0) > 12_000
  )
    return false;
  const hljs = await load(language);
  if (!code.isConnected || !allowed() || !hljs.getLanguage(language))
    return false;
  const selection = getSelection();
  if (selection && !selection.isCollapsed && selection.containsNode(code, true))
    return false;
  hljs.highlightElement(code);
  return true;
}
