import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { resolveDesktopEnvironment } from './desktop-env.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log(
    'Hashline Desktop Dev\n\n  npm run desktop                         Dev-Fenster öffnen\n  npm run dev:demo                        Markdown-Fixture öffnen\n  npm run desktop -- --file ./README.md   Eigene Datei öffnen\n  npm run dev:doctor                      Build-Werkzeuge prüfen\n\nWeitere Optionen gehen an tauri dev, z. B. --no-watch oder --verbose.',
  );
  process.exit(0);
}
const fileIndex = args.indexOf('--file');
let file;
if (fileIndex >= 0) {
  const value = args[fileIndex + 1];
  if (!value || value.startsWith('--')) {
    console.error('--file benötigt einen Dateipfad.');
    process.exit(1);
  }
  file = resolve(process.cwd(), value);
  if (!existsSync(file)) {
    console.error('Die angegebene Testdatei existiert nicht.');
    process.exit(1);
  }
  args.splice(fileIndex, 2);
}
const doctor = args.includes('--doctor');
const { env, messages, checks } = resolveDesktopEnvironment(root);
for (const message of messages) console.log(message);
for (const check of checks)
  console.log(`${check.ok ? '✓' : '✗'} ${check.label}: ${check.version}`);
if (checks.some((check) => !check.ok)) process.exit(1);
if (doctor) process.exit(0);

console.log(
  '\nHashline Dev · Frontend-HMR + Rust-Neustart · Inspector: Strg+Umschalt+I\n',
);
const child = spawn(
  resolve(root, 'node_modules/.bin/tauri'),
  [
    'dev',
    '--config',
    resolve(root, 'src-tauri/tauri.dev.conf.json'),
    ...args,
    ...(file ? ['--', '--', file] : []),
  ],
  { cwd: root, env, stdio: 'inherit' },
);
child.on('error', (error) => {
  console.error(`Dev-Start fehlgeschlagen: ${error.message}`);
  process.exitCode = 1;
});
// Tauri owns the Vite process and native watcher. Forward shutdown so it can clean up both.
const interrupt = () => child.kill('SIGINT');
const terminate = () => child.kill('SIGTERM');
process.on('SIGINT', interrupt);
process.on('SIGTERM', terminate);
child.on('exit', (code, signal) => {
  process.removeListener('SIGINT', interrupt);
  process.removeListener('SIGTERM', terminate);
  process.exitCode =
    code ?? (signal === 'SIGINT' || signal === 'SIGTERM' ? 0 : 1);
});
