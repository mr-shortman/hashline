"""Run against tauri-driver: python3 tests/desktop/smoke.py /absolute/path/to/hashline.

Uses the real packaged frontend and native adapter. Requires a graphical session.
"""
import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

BASE = os.environ.get('HASHLINE_WEBDRIVER_URL', 'http://127.0.0.1:4444')


def request(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data, {'Content-Type': 'application/json'}, method=method)
    try:
        with urllib.request.urlopen(req, timeout=60) as response:
            return json.load(response)['value']
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error


class Desktop:
    def __init__(self, binary, args=None):
        self.binary = str(Path(binary).resolve())
        result = request('POST', '/session', {'capabilities': {'alwaysMatch': {'tauri:options': {'application': self.binary, 'args': args or []}}}})
        self.session = result['sessionId']

    def js(self, script, *args):
        return request('POST', f'/session/{self.session}/execute/sync', {'script': script, 'args': list(args)})

    def js_async(self, script, *args):
        """Runs a script that reports through the callback WebDriver appends.

        Needed where a single round trip must observe a moment the polling of
        `wait` would already have missed."""
        return request('POST', f'/session/{self.session}/execute/async', {'script': script, 'args': list(args)})

    def wait(self, expression, timeout=15):
        start = time.monotonic()
        while time.monotonic() - start < timeout:
            result = self.js('return ' + expression)
            if result:
                return result
            time.sleep(.025)
        raise AssertionError('Timed out: ' + expression)

    def open(self, path, cwd=None):
        subprocess.run([self.binary, str(path)], cwd=cwd, check=True, timeout=10, stdout=subprocess.DEVNULL)
        name = Path(path).name
        self.wait('document.querySelector(".file-title")?.textContent === ' + json.dumps(name))

    def screenshot(self, path):
        encoded = request('GET', f'/session/{self.session}/screenshot')
        Path(path).write_bytes(base64.b64decode(encoded))

    def close(self):
        request('DELETE', f'/session/{self.session}')


def main():
    with tempfile.TemporaryDirectory(prefix='hashline-desktop-') as directory:
        root = Path(directory)
        source = Path('tests/fixtures/reader.md').read_text()
        (root / 'reader.md').write_text(source)
        (root / 'zweite Datei.md').write_text('# Zweite Datei\n\n## Ziel\n\n[Zurück](reader.md#tabelle)')
        shutil.copy('tests/fixtures/pixel.png', root)
        app = Desktop(sys.argv[1], [str(root / 'reader.md')])
        passed = []
        try:
            app.wait('document.querySelector("article h1")')
            app.js('document.querySelector("article img").scrollIntoView()')
            app.wait('document.querySelector("article img").naturalWidth === 1')
            passed.append('CLI opening and controlled local image URL')
            app.js("document.querySelector('[aria-label=Suche]').click(); const input=document.querySelector('.search-bar input'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'Nadel'); input.dispatchEvent(new Event('input',{bubbles:true}));")
            app.wait("document.querySelector('.search-count').textContent === '1 / 3'")
            app.js("document.querySelector('[aria-label=\"Nächster Treffer\"]').click()")
            app.wait("document.querySelector('.search-count').textContent === '2 / 3'")
            app.js("document.querySelector('[aria-label=\"Suche schließen\"]').click()")
            passed.append('WebKit text search in tables and code')
            app.js("const r=document.createRange();r.selectNodeContents(document.querySelector('article'));getSelection().removeAllRanges();getSelection().addRange(r)")
            selected = app.js('return getSelection().toString()')
            assert 'Hashline · Lesetest' in selected and 'Ein letzter Absatz' in selected
            app.js('getSelection().removeAllRanges()')
            passed.append('Selection across paragraphs, lists, tables and code')
            app.open('zweite Datei.md', cwd=root)
            app.js("document.querySelector('article a').click()")
            app.wait("document.querySelector('.file-title').textContent === 'reader.md'")
            app.wait("document.activeElement?.id === 'doc-tabelle'")
            passed.append('Relative caller cwd, Unicode/spaces, relative Markdown link and fragment')
            app.js("document.querySelector('.document-scroll').scrollTop = document.getElementById('doc-code').offsetTop")
            time.sleep(.3)
            before = app.js("return document.getElementById('doc-code').getBoundingClientRect().top")
            replacement = root / 'replacement.md'
            replacement.write_text(source + '\n\nRELOAD-MARKER\n')
            replacement.replace(root / 'reader.md')
            app.wait("document.querySelector('article').textContent.includes('RELOAD-MARKER')")
            after = app.js("return document.getElementById('doc-code').getBoundingClientRect().top")
            assert abs(before - after) < 8, (before, after)
            passed.append('Atomic rename reload preserves reading position')
            (root / 'reader.md').unlink()
            app.wait("document.querySelector('[role=alert]')")
            assert app.js("return document.querySelector('article').textContent.includes('RELOAD-MARKER')")
            (root / 'reader.md').write_text(source + '\nRECREATED-MARKER\n')
            app.wait("document.querySelector('article').textContent.includes('RECREATED-MARKER')")
            passed.append('Delete/recreate keeps content on error and recovers')
            app.js("window.__denied=null; window.__TAURI_INTERNALS__.invoke('read_document',{path:'/etc/passwd'}).then(()=>window.__denied=false,()=>window.__denied=true)")
            app.wait('window.__denied === true')
            passed.append('Native command rejects unapproved paths')
            Path('test-results').mkdir(exist_ok=True)
            app.screenshot('test-results/desktop.png')
            result = {'engine': app.js('return navigator.userAgent'), 'cssHighlights': app.js('return !!CSS.highlights'), 'passed': passed}
            Path('test-results/desktop.json').write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result, indent=2))
        finally:
            app.close()


if __name__ == '__main__':
    main()
