import { mkdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { marked } from 'marked';
import { JSDOM } from 'jsdom';

await mkdir('benchmarks/generated', { recursive: true });
const metadata = [];
const block = (i) =>
  `\n## Abschnitt ${i}\n\nLesbarer Text mit **Hervorhebung**, [einem Link](#abschnitt-${i}) und Unicode: Grüße 日本語. Dieser Absatz enthält eine Suchnadel und bleibt ein zusammenhängender Text.\n\n- Erster Punkt\n- Zweiter Punkt\n  - Verschachtelt\n\n| Name | Wert |\n| --- | --- |\n| Beispiel | ${i} |\n\n\`\`\`typescript\nconst section = ${i};\nconsole.log(section);\n\`\`\`\n`;
async function fixture(name, source) {
  const html = marked.parse(source);
  const dom = new JSDOM(html);
  const walker = dom.window.document.createTreeWalker(
    dom.window.document.body,
    dom.window.NodeFilter.SHOW_ALL,
  );
  let nodes = 0;
  while (walker.nextNode()) nodes++;
  metadata.push({
    name,
    bytes: Buffer.byteLength(source),
    blocks: marked.lexer(source).length,
    parserDomNodes: nodes,
    sha256: createHash('sha256').update(source).digest('hex'),
  });
  dom.window.close();
  await writeFile(`benchmarks/generated/${name}.md`, source);
}
for (const [name, size] of [
  ['small', 100 * 1024],
  ['medium', 1024 * 1024],
  ['large', 10 * 1024 * 1024],
]) {
  const chunks = ['# Hashline Benchmark\n'];
  let bytes = Buffer.byteLength(chunks[0]);
  let i = 0;
  while (bytes < size) {
    const text = block(i++);
    chunks.push(text);
    bytes += Buffer.byteLength(text);
  }
  await fixture(name, chunks.join(''));
}
await fixture('long-line', '# Lange Zeile\n\n' + 'abcdefghij'.repeat(100_000));
await fixture(
  'deep-list',
  '# Tiefe Liste\n\n' +
    Array.from({ length: 100 }, (_, i) => '  '.repeat(i) + '- Ebene ' + i).join(
      '\n',
    ),
);
await fixture(
  'wide-table',
  '# Breite Tabelle\n\n|' +
    ' Spalte |'.repeat(100) +
    '\n|' +
    ' --- |'.repeat(100) +
    '\n' +
    ('|' + ' Inhalt |'.repeat(100) + '\n').repeat(100),
);
await fixture(
  'many-blocks',
  '# Kleine Blöcke\n\n' + 'Absatz.\n\n'.repeat(20_000),
);
await writeFile(
  'benchmarks/generated/metadata.json',
  JSON.stringify(metadata, null, 2) + '\n',
);
console.log(JSON.stringify(metadata, null, 2));
