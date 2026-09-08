// Builds the WebAssembly parser into src/generated/. The artifact is committed
// so that `npm test` and `npm run build` work without a Rust toolchain; only
// changes to src-tauri/markdown require running this
// (docs/decisions/008-parser-reference.md, section 3).
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveDesktopEnvironment } from './desktop-env.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const { env, messages } = resolveDesktopEnvironment(root);
for (const message of messages) console.log(message);

const build = spawnSync(
  'cargo',
  [
    'build',
    '--release',
    '--target',
    'wasm32-unknown-unknown',
    '--manifest-path',
    'src-tauri/markdown-wasm/Cargo.toml',
  ],
  { cwd: root, env, stdio: 'inherit' },
);
if (build.status !== 0) process.exit(build.status ?? 1);

const source = join(
  root,
  'src-tauri/markdown-wasm/target/wasm32-unknown-unknown/release/hashline_markdown_wasm.wasm',
);
const target = join(root, 'src/generated/hashline-markdown.wasm');
mkdirSync(dirname(target), { recursive: true });
copyFileSync(source, target);
console.log(
  `src/generated/hashline-markdown.wasm — ${statSync(target).size} Bytes`,
);
