import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.request

root=Path(__file__).resolve().parents[2]
os.chdir(root)
backend=sys.argv[1]
label=sys.argv[2]
with tempfile.TemporaryDirectory(prefix='hashline-acceptance-session-') as tmp:
    env=os.environ.copy()
    for key, leaf in [('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache')]:
        env[key]=str(Path(tmp)/leaf)
    env['GDK_BACKEND']=backend
    env['HASHLINE_WEBDRIVER_URL']='http://127.0.0.1:4476'
    env['GTK_THEME']='Adwaita:dark' if label.endswith('dark') else 'Adwaita'
    (root/'test-results').mkdir(exist_ok=True)
    log=open(root/'test-results/acceptance-driver.log','w')
    driver=subprocess.Popen([os.environ.get('HASHLINE_TAURI_DRIVER','/tmp/hashline-cargo/bin/tauri-driver'),'--port','4476','--native-port','4477','--native-driver',os.environ.get('HASHLINE_NATIVE_DRIVER','/tmp/hashline-sysroot/usr/bin/WebKitWebDriver')],env=env,stdout=log,stderr=log)
    original_theme=subprocess.check_output(['gsettings','get','org.gnome.desktop.interface','color-scheme'],text=True).strip()
    subprocess.run(['gsettings','set','org.gnome.desktop.interface','color-scheme','prefer-dark' if label.endswith('dark') else 'prefer-light'],check=True)
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen('http://127.0.0.1:4476/status',timeout=1).close()
                break
            except OSError: time.sleep(.1)
        binary=os.environ.get('HASHLINE_INSTALLED_BINARY',str(Path.home()/'.local/bin/hashline'))
        if label=='remote':
            cmd=['python3','tests/desktop/remote_images.py',binary]
        elif label=='smoke':
            cmd=['python3','tests/desktop/smoke.py',binary]
        else:
            cmd=['python3','tests/desktop/accessibility.py',binary,'--label',label]+sys.argv[3:]
        subprocess.run(cmd,env=env,check=True)
    finally:
        subprocess.run(['gsettings','set','org.gnome.desktop.interface','color-scheme',original_theme],check=True)
        driver.terminate()
        try: driver.wait(timeout=5)
        except subprocess.TimeoutExpired: driver.kill();driver.wait()
        log.close()
