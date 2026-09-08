"""Read-only provenance; unknown hardware fields never become reference claims."""
import hashlib
import os
from pathlib import Path
import platform
import subprocess


def command(*args):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=10)
        return {'command': list(args), 'exitCode': result.returncode,
                'stdout': result.stdout.strip(), 'stderr': result.stderr.strip()}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {'command': list(args), 'error': str(error)}


def environment(binary):
    path = Path(binary).resolve()
    return {
        'binary': str(path), 'binarySha256': hashlib.sha256(path.read_bytes()).hexdigest(),
        'kernel': platform.platform(), 'distribution': platform.freedesktop_os_release(),
        'sessionType': os.environ.get('XDG_SESSION_TYPE'),
        'gdkBackend': os.environ.get('GDK_BACKEND'),
        'loadAverage': list(os.getloadavg()),
        'competingHashlineProcesses': command('pgrep', '-a', '-x', 'hashline'),
        'desktopUnattendedVerified': False,
        'cpu': command('lscpu'), 'memory': Path('/proc/meminfo').read_text(),
        'graphics': command('lspci', '-nn'),
        'storage': command('lsblk', '-d', '-o', 'NAME,MODEL,ROTA,SIZE'),
        'webkit': command('dpkg-query', '-W', 'libwebkit2gtk-4.1-0'),
        'displays': command('gdbus', 'call', '--session', '--dest', 'org.gnome.Mutter.DisplayConfig',
                            '--object-path', '/org/gnome/Mutter/DisplayConfig', '--method',
                            'org.gnome.Mutter.DisplayConfig.GetCurrentState'),
        'installedPackage': command('dpkg-query', '-W', 'hashline'),
        'referenceVerified': False,
    }
