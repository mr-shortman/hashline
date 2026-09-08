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
async function load() {
  const [
    core,
    javascript,
    typescript,
    json,
    bash,
    python,
    rust,
    css,
    xml,
    yaml,
  ] = await Promise.all([
    import('highlight.js/lib/core'),
    import('highlight.js/lib/languages/javascript'),
    import('highlight.js/lib/languages/typescript'),
    import('highlight.js/lib/languages/json'),
    import('highlight.js/lib/languages/bash'),
    import('highlight.js/lib/languages/python'),
    import('highlight.js/lib/languages/rust'),
    import('highlight.js/lib/languages/css'),
    import('highlight.js/lib/languages/xml'),
    import('highlight.js/lib/languages/yaml'),
  ]);
  for (const [name, module] of Object.entries({
    javascript,
    typescript,
    json,
    bash,
    python,
    rust,
    css,
    xml,
    yaml,
  }))
    core.default.registerLanguage(name, module.default);
  return core.default;
}
export async function highlightCode(
  code: HTMLElement,
  allowed: () => boolean = () => true,
): Promise<boolean> {
  const language = languageOf(code);
  if (!language || (code.textContent?.length || 0) > 12_000) return false;
  engine ??= load();
  const hljs = await engine;
  if (!code.isConnected || !allowed() || !hljs.getLanguage(language))
    return false;
  const selection = getSelection();
  if (selection && !selection.isCollapsed && selection.containsNode(code, true))
    return false;
  hljs.highlightElement(code);
  return true;
}
