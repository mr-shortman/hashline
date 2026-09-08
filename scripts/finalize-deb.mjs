import { mkdtemp, readdir, rename, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

// Tauri derives the desktop filename from productName. Wayland launchers instead
// need the GTK application ID as the filename, while Name remains "Hashline".
const directory = resolve('src-tauri/target/release/bundle/deb');
for (const file of await readdir(directory)) {
  if (!file.endsWith('.deb')) continue;
  const staging = await mkdtemp(join(tmpdir(), 'hashline-package-'));
  const target = join(directory, file);
  try {
    execFileSync('dpkg-deb', ['--raw-extract', target, staging]);
    const applications = join(staging, 'usr/share/applications');
    const names = await readdir(applications);
    if (names.includes('Hashline.desktop')) {
      await rename(
        join(applications, 'Hashline.desktop'),
        join(applications, 'de.kalendium.Hashline.desktop'),
      );
    }
    execFileSync(
      'desktop-file-validate',
      [join(applications, 'de.kalendium.Hashline.desktop')],
      { stdio: 'inherit' },
    );
    execFileSync(
      'dpkg-deb',
      ['--root-owner-group', '--build', staging, target],
      { stdio: 'inherit' },
    );
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}
