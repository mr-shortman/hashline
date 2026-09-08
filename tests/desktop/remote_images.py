"""Deterministic remote-image acceptance in the installed desktop app.

The HTTP server binds loopback only; no external service or private file is used.
"""
import hashlib
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import struct
import sys
import tempfile
import threading
import time
import zlib
from smoke import Desktop
from accessibility import press


def main():
    requests = []
    concurrency = {'active': 0, 'maximum': 0}
    lock = threading.Lock()
    pixel = Path('tests/fixtures/pixel.png').read_bytes()
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def do_GET(self):
            requests.append({'path':self.path,'headers':dict(self.headers)})
            if self.path == '/redirect':
                self.send_response(302)
                self.send_header('Location','/pixel.png')
                self.end_headers()
                return
            if self.path == '/loop':
                self.send_response(302)
                self.send_header('Location','/loop')
                self.end_headers()
                return
            if self.path == '/slow': time.sleep(3)
            if self.path.startswith('/budget-'):
                with lock:
                    concurrency['active'] += 1
                    concurrency['maximum'] = max(concurrency['maximum'], concurrency['active'])
                time.sleep(.04)
                with lock: concurrency['active'] -= 1
            body = pixel
            if self.path == '/vector.svg': body = b'<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>'
            if self.path == '/fake.png': body = b'<html><script>alert(1)</script></html>'
            if self.path == '/pixels.png':
                header = struct.pack('>IIBBBBB', 6000, 5000, 8, 2, 0, 0, 0)
                body = b'\x89PNG\r\n\x1a\n' + struct.pack('>I', len(header)) + b'IHDR' + header + struct.pack('>I', zlib.crc32(b'IHDR'+header)) + pixel[33:]
            self.send_response(200)
            self.send_header('Content-Type', 'image/png')  # MIME must not bypass signature checks.
            self.send_header('Set-Cookie','tracking=forbidden')
            self.send_header('Content-Length',str(17*1024*1024 if self.path == '/large.png' else len(body)))
            self.end_headers()
            try: self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError): pass
    server = ThreadingHTTPServer(('127.0.0.1',0),Handler)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    origin = f'http://127.0.0.1:{server.server_port}'
    passed = []
    output = Path(os.environ.get('HASHLINE_ACCEPTANCE_OUTPUT','test-results/acceptance'))
    output.mkdir(parents=True,exist_ok=True)
    binary = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix='hashline-remote-') as temp:
        root = Path(temp)
        doc = root/'remote.md'
        doc.write_text('# Remote\n\n'+ '\n\n'.join(f'![{path}]({origin}/{path})' for path in ['pixel.png','redirect','vector.svg','fake.png','large.png','pixels.png','loop']))
        other = root/'other.md'
        other.write_text(f'# Andere Datei\n\n![Andere]({origin}/other.png)')
        app = Desktop(sys.argv[1],[str(doc)])
        try:
            app.wait('document.querySelector(".remote-images button")')
            time.sleep(.3)
            assert not requests, requests
            assert app.js('return document.querySelectorAll("article img[src]").length === 0')
            app.screenshot(output/'remote-blocked.png')
            # Bypassing the UI does not bypass native per-document approval.
            app.js('''const img = new Image(); window.__probe=img;
                const id=document.querySelector('article img[data-remote-source]');
                // The document id is the current sequence, initially zero in a new process.
                img.src='hashline-image://localhost/0/'+encodeURIComponent(arguments[0]);''',origin+'/probe')
            time.sleep(.3)
            assert not requests
            passed.append('No network before explicit approval, including native protocol bypass attempt')
            app.js('document.querySelector("article h1").dataset.sentinel="kept"')
            press(app,'.remote-images button')
            app.wait('!document.querySelector(".remote-images")')
            app.js('document.querySelectorAll("article img").forEach(img=>img.loading="eager")')
            app.wait('document.querySelectorAll("article img")[0].naturalWidth === 1 && document.querySelectorAll("article img")[1].naturalWidth === 1')
            app.wait('document.querySelectorAll("article img[data-unavailable]").length === 5')
            assert app.js('return document.querySelector("article h1").dataset.sentinel === "kept"')
            assert app.js('return !document.querySelector("article script, article svg")')
            app.screenshot(output/'remote-approved.png')
            assert all(not any(h.lower() in ['cookie','referer','authorization'] for h in row['headers']) for row in requests)
            passed.append('Keyboard approval preserves DOM; raster/redirect load; SVG, forged MIME, >16 MiB, >24 MP and redirect loop rejected; no cookie/referrer/auth')
            # Actual changed revision must ask again, while the current document remains readable.
            before = len(requests)
            doc.write_text(doc.read_text()+'\nRevision geändert\n')
            app.wait('document.querySelector("article").textContent.includes("Revision geändert") && document.querySelector(".remote-images button")')
            time.sleep(.3)
            assert len(requests) == before
            passed.append('Changed revision resets consent without network requests')
            press(app,'.remote-images button')
            app.wait('!document.querySelector(".remote-images")')
            app.open(other)
            app.wait('document.querySelector(".remote-images button")')
            assert not any(r['path'] == '/other.png' for r in requests)
            app.open(doc)
            app.wait('document.querySelector(".remote-images button")')
            passed.append('File switch and reopening reset consent; approval grants no access to another document')
            app.js("window.__denied=null;window.__TAURI_INTERNALS__.invoke('allow_remote_images',{id:'nonexistent'}).then(()=>window.__denied=false,()=>window.__denied=true)")
            app.wait('window.__denied === true')
            passed.append('Native approval rejects unknown/closed document handles')
            budget = root/'budget.md'
            budget.write_text('# Budget\n\n'+'\n\n'.join(f'![Bild]({origin}/budget-{i}.png)' for i in range(70)))
            app.open(budget)
            app.wait('document.querySelector(".remote-images button")')
            press(app,'.remote-images button')
            app.wait('!document.querySelector(".remote-images")')
            app.js('document.querySelectorAll("article img").forEach(img=>img.loading="eager")')
            app.wait('document.querySelectorAll("article img[data-unavailable]").length === 6 && Array.from(document.querySelectorAll("article img")).filter(i=>i.naturalWidth===1).length === 64')
            assert len([r for r in requests if r['path'].startswith('/budget-')]) == 64
            assert 1 <= concurrency['maximum'] <= 4, concurrency
            passed.append('64-request limit and at most four simultaneous downloads enforced in native protocol')
            slow = root/'slow.md'
            slow.write_text(f'# Langsam\n\n![Spät]({origin}/slow)')
            app.open(slow)
            app.wait('document.querySelector(".remote-images button")')
            press(app,'.remote-images button')
            app.wait('!document.querySelector(".remote-images")')
            deadline = time.monotonic()+5
            while not any(r['path']=='/slow' for r in requests) and time.monotonic()<deadline: time.sleep(.02)
            assert any(r['path']=='/slow' for r in requests)
            app.open(other)
            app.wait('document.querySelector(".remote-images button")')
            time.sleep(3.2)
            assert app.js('return document.querySelector("article h1").textContent === "Andere Datei" && !document.querySelector("article img[src]")')
            passed.append('Switch during active download keeps new document blocked and rejects stale image results')
            (output/'remote-images.json').write_text(json.dumps({'binary':str(binary),'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'completed':True,'passed':passed,'concurrency':concurrency,'requests':requests},indent=2)+'\n')
            print(json.dumps({'passed':passed},indent=2))
        finally:
            app.close()
            server.shutdown()


if __name__ == '__main__': main()
