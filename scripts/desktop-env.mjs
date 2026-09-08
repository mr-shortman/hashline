import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { parseEnv } from 'node:util';

export function resolveDesktopEnvironment(root, inherited = process.env) {
  const env = { ...inherited };
  const messages = [];
  const local = join(root, '.env.desktop.local');
  if (existsSync(local)) {
    const values = parseEnv(readFileSync(local, 'utf8'));
    for (const key of ['CARGO_HOME', 'RUSTUP_HOME', 'HASHLINE_DEV_SYSROOT']) {
      if (!env[key] && values[key]) env[key] = values[key];
    }
  }
  const probe = (command, args) =>
    spawnSync(command, args, { cwd: root, env, encoding: 'utf8' });
  const prepend = (key, paths) => {
    env[key] = [...paths, env[key]].filter(Boolean).join(delimiter);
  };

  if (env.CARGO_HOME) prepend('PATH', [join(env.CARGO_HOME, 'bin')]);
  if (probe('cargo', ['--version']).error?.code === 'ENOENT') {
    if (!env.CARGO_HOME && existsSync(join(homedir(), '.cargo/bin/cargo'))) {
      prepend('PATH', [join(homedir(), '.cargo/bin')]);
    } else if (
      !env.CARGO_HOME &&
      !env.RUSTUP_HOME &&
      existsSync('/tmp/hashline-cargo/bin/cargo') &&
      existsSync('/tmp/hashline-rustup')
    ) {
      env.CARGO_HOME = '/tmp/hashline-cargo';
      env.RUSTUP_HOME = '/tmp/hashline-rustup';
      prepend('PATH', [join(env.CARGO_HOME, 'bin')]);
      messages.push(
        'Verwende die temporäre Rust-Toolchain unter /tmp/hashline-cargo.',
      );
    }
  }
  if (
    !env.HASHLINE_DEV_SYSROOT &&
    probe('pkg-config', ['--exists', 'webkit2gtk-4.1', 'gtk+-3.0']).status !==
      0 &&
    existsSync(
      '/tmp/hashline-sysroot/usr/lib/x86_64-linux-gnu/pkgconfig/webkit2gtk-4.1.pc',
    )
  ) {
    env.HASHLINE_DEV_SYSROOT = '/tmp/hashline-sysroot';
    messages.push(
      'Verwende die temporären GTK-/WebKitGTK-Buildpakete unter /tmp/hashline-sysroot.',
    );
  }
  if (env.HASHLINE_DEV_SYSROOT) {
    prepend('PKG_CONFIG_PATH', [
      join(env.HASHLINE_DEV_SYSROOT, 'usr/lib/x86_64-linux-gnu/pkgconfig'),
      join(env.HASHLINE_DEV_SYSROOT, 'usr/share/pkgconfig'),
    ]);
  }

  const checks = [
    ['Cargo', 'cargo', ['--version']],
    ['Rust', 'rustc', ['--version']],
    ['WebKitGTK', 'pkg-config', ['--modversion', 'webkit2gtk-4.1']],
    ['GTK', 'pkg-config', ['--modversion', 'gtk+-3.0']],
  ].map(([label, command, args]) => {
    const result = probe(command, args);
    return {
      label,
      ok: result.status === 0,
      version:
        result.status === 0
          ? result.stdout.trim()
          : 'nicht verfügbar (siehe README → Entwickeln)',
    };
  });
  return { env, messages, checks };
}
