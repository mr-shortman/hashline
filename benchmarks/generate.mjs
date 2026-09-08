import { mkdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { marked } from 'marked';
import { JSDOM } from 'jsdom';
import { deflateSync } from 'node:zlib';

await mkdir('benchmarks/generated', { recursive: true });
const metadata = [];
const block = (i) =>
  `\n## Abschnitt ${i}\n\nLesbarer Text mit **Hervorhebung**, [einem Link](#abschnitt-${i}) und Unicode: Grüße 日本語. Dieser Absatz enthält eine Suchnadel und bleibt ein zusammenhängender Text.\n\n- Erster Punkt\n- Zweiter Punkt\n  - Verschachtelt\n\n| Name | Wert |\n| --- | --- |\n| Beispiel | ${i} |\n\n\`\`\`typescript\nconst section = ${i};\nconsole.log(section);\n\`\`\`\n`;
async function fixture(name, source, extra = {}) {
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
    ...extra,
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
// Deterministic RGB noise creates real compressed-byte and decode pressure.
function png(width, height, seed) {
  let value = seed;
  const raw = Buffer.alloc(height * (width * 3 + 1));
  for (let y = 0; y < height; y++) {
    const offset = y * (width * 3 + 1);
    for (let x = 1; x <= width * 3; x++) {
      value ^= value << 13;
      value ^= value >>> 17;
      value ^= value << 5;
      raw[offset + x] = value & 255;
    }
  }
  function chunk(type, data) {
    const tag = Buffer.from(type);
    const bytes = Buffer.concat([tag, data]);
    let crc = 0xffffffff;
    for (const byte of bytes) {
      crc ^= byte;
      for (let b = 0; b < 8; b++)
        crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    const check = Buffer.alloc(4);
    check.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
    return Buffer.concat([length, bytes, check]);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 2;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}
for (const [name, count, width, height] of [
  ['many-images', 100, 256, 256],
  ['large-images', 4, 2048, 2048],
]) {
  const images = [];
  const lines = [`# ${name}\n`];
  for (let i = 0; i < count; i++) {
    const filename = `${name}-${i}.png`;
    const bytes = png(width, height, i + 1);
    await writeFile(`benchmarks/generated/${filename}`, bytes);
    images.push({
      filename,
      width,
      height,
      compressedBytes: bytes.length,
      decodedRgbaBytes: width * height * 4,
      sha256: createHash('sha256').update(bytes).digest('hex'),
    });
    lines.push(`## Bild ${i}\n\n![Bild ${i}](${filename})\n`);
  }
  await fixture(name, lines.join('\n'), { images });
}
await fixture(
  'large-code',
  '# Großer Codeblock\n\n```text\n' +
    'long code line\n'.repeat(70_000) +
    '```\n',
);
await writeFile(
  'benchmarks/generated/metadata.json',
  JSON.stringify(metadata, null, 2) + '\n',
);
console.log(JSON.stringify(metadata, null, 2));
