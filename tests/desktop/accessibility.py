"""Installed WebKitGTK acceptance. Start tauri-driver in the desired session first.

python3 tests/desktop/accessibility.py ~/.local/bin/hashline --label wayland-125-light
The label describes the independently configured compositor scale/system theme;
actual WebView geometry, DPR and system preference are recorded, not inferred.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import time
from smoke import Desktop, request
from native_input import NativeInput

CTRL, SHIFT, TAB, ENTER, ESC, SPACE = '\ue009', '\ue008', '\ue004', '\ue007', '\ue00c', ' '


def keys(app, *values):
    actions = [{'type': 'keyDown', 'value': key} for key in values]
    actions += [{'type': 'keyUp', 'value': key} for key in reversed(values)]
    request('POST', f'/session/{app.session}/actions', {'actions': [{'type': 'key', 'id': 'keyboard', 'actions': actions}]})


def focus(app, selector):
    app.js('document.querySelector(arguments[0]).focus()', selector)


def press(app, selector, key=ENTER):
    focus(app, selector)
    keys(app, key)


def theme(app, value):
    press(app, '[aria-label="Darstellung und Optionen"]')
    app.wait('document.querySelector(".settings")')
    app.js('document.querySelectorAll(".theme-options button")[arguments[0]].focus()', ['system', 'light', 'dark'].index(value))
    keys(app, ENTER)
    app.wait('document.documentElement.dataset.theme === ' + json.dumps(value))
    keys(app, ESC)


AUDIT = r"""
const rgba = s => (s.match(/[\d.]+/g) || []).map(Number);
function background(el) {
  const c = rgba(getComputedStyle(el).backgroundColor);
  const a = c[3] ?? 1;
  if (a === 1 || !el.parentElement) return c;
  const p = background(el.parentElement);
  return c.slice(0, 3).map((v, i) => v * a + p[i] * (1 - a));
}
function luminance(c) {
  const v = c.slice(0, 3).map(v => v / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
  return v[0] * .2126 + v[1] * .7152 + v[2] * .0722;
}
const results = [];
for (const el of document.querySelectorAll('body *')) {
  if (el.closest('[aria-hidden=true],button:disabled,input:disabled') || !el.getClientRects().length) continue;
  const style = getComputedStyle(el);
  if (style.visibility === 'hidden' || style.display === 'none') continue;
  const text = [...el.childNodes].some(n => n.nodeType === 3 && n.textContent.trim());
  if (!text && !el.matches('button, input, a, summary')) continue;
  const fg = luminance(rgba(style.color));
  const bg = luminance(background(el));
  results.push({element: el.tagName + '.' + el.className, color:style.color, background:background(el), ratio: (Math.max(fg,bg)+.05)/(Math.min(fg,bg)+.05)});
}
const tableWords = [...document.querySelectorAll('th')].map(el => {
  const range = document.createRange();
  range.setStart(el.firstChild,0);
  range.setEnd(el.firstChild,el.textContent.split(' ')[0].length);
  return range.getClientRects().length;
});
const controls = [...document.querySelectorAll('.titlebar button, .search-bar button, .search-bar input')].map(el => {
  const r = el.getBoundingClientRect();
  return {name: el.getAttribute('aria-label'), left:r.left, right:r.right, top:r.top, bottom:r.bottom};
});
return {width:innerWidth, height:innerHeight, dpr:devicePixelRatio,
  screen:{width:screen.width,height:screen.height},
  systemDark:matchMedia('(prefers-color-scheme: dark)').matches,
  font:getComputedStyle(document.querySelector('article')).fontSize,
  horizontalOverflow:document.body.scrollWidth > innerWidth,
  controls, tableWords, contrast:results, minimumContrast:Math.min(...results.map(r=>r.ratio)),
  background:getComputedStyle(document.documentElement).backgroundColor};
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binary')
    parser.add_argument('--label', required=True)
    parser.add_argument('--windows-only', action='store_true')
    parser.add_argument('--wide', type=int, default=1440)
    parser.add_argument('--height', type=int, default=780)
    parser.add_argument('--output', default=os.environ.get('HASHLINE_ACCEPTANCE_OUTPUT','test-results/acceptance'))
    args = parser.parse_args()
    out = Path(args.output) / args.label
    out.mkdir(parents=True, exist_ok=True)
    binary = Path(args.binary).resolve()
    native = NativeInput()
    app = Desktop(binary, [str(Path('tests/fixtures/accessibility.md').resolve())])
    result = {'binary':str(binary), 'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
              'backend':os.environ.get('GDK_BACKEND'), 'cases':[], 'passed':[], 'completed':False}
    try:
        app.wait('document.querySelector("article")?.dataset.renderState === "complete"')
        app.wait('document.querySelector(".hljs-keyword")')
        time.sleep(.5)
        native.focus(app)
        result['engine'] = app.js('return navigator.userAgent')
        for width in ([] if args.windows_only else [360, args.wide]):
            request('POST', f'/session/{app.session}/window/rect', {'width':width,'height':args.height})
            app.wait(f'innerWidth === {width}')
            for mode in ['light', 'dark', 'system']:
                theme(app, mode)
                for zoom in [100, 200]:
                    keys(app, CTRL, '0')
                    if zoom == 200:
                        for _ in range(10): keys(app, CTRL, '+')
                    app.wait(f'getComputedStyle(document.querySelector("article")).fontSize === "{17 * zoom / 100:g}px"')
                    keys(app, CTRL, 'f')
                    app.wait('document.activeElement === document.querySelector(".search-bar input")')
                    time.sleep(.2)  # Let the 120ms theme/hover transitions settle.
                    audit = app.js(AUDIT)
                    (out / 'latest-audit.json').write_text(json.dumps(audit,indent=2))
                    app.screenshot(out / 'latest.png')
                    assert not audit['horizontalOverflow'], audit
                    assert all(lines == 1 for lines in audit['tableWords']), audit['tableWords']
                    assert audit['minimumContrast'] >= 4.5, [r for r in audit['contrast'] if r['ratio'] < 4.5]
                    assert all(c['left'] >= 0 and c['right'] <= audit['width'] and c['bottom'] <= audit['height'] for c in audit['controls']), audit['controls']
                    if mode == 'system':
                        assert audit['background'] == ('rgb(28, 32, 30)' if audit['systemDark'] else 'rgb(251, 250, 248)')
                    audit.update({'theme':mode,'zoom':zoom})
                    result['cases'].append(audit)
                    app.js('document.querySelector(".document-scroll").scrollTop = 0')
                    app.screenshot(out / f'{width}-{mode}-{zoom}.png')
                    for section in ['doc-code-und-tabellen','doc-bilder-und-abschluss']:
                        app.js('document.getElementById(arguments[0]).scrollIntoView()',section)
                        if section == 'doc-bilder-und-abschluss':
                            app.wait('document.querySelector(".image-placeholder")?.textContent.includes("Fehlendes Bild")')
                        app.screenshot(out / f'{width}-{mode}-{zoom}-{section}.png')
                    keys(app, ESC)
                    app.wait('document.activeElement?.getAttribute("aria-label") === "Suche"')
                    keys(app, CTRL, SHIFT, 'o')
                    app.wait('document.querySelector(".outline")')
                    if width < 900:
                        assert app.js('return document.querySelector(".viewport-shell").inert && document.querySelector(".titlebar").inert')
                        keys(app, SHIFT, TAB)
                        app.wait('document.activeElement === document.querySelector(".outline nav button:last-child")')
                        keys(app, TAB)
                        app.wait('document.activeElement === document.querySelector(".outline .icon-button")')
                    app.screenshot(out / f'{width}-{mode}-{zoom}-outline.png')
                    keys(app, ESC)
                    app.wait('document.activeElement?.getAttribute("aria-label") === "Inhaltsverzeichnis"')
                    press(app, '[aria-label="Darstellung und Optionen"]')
                    keys(app, SHIFT, TAB)
                    app.wait('document.activeElement?.textContent === "Dateipfad kopieren"')
                    app.screenshot(out / f'{width}-{mode}-{zoom}-menu.png')
                    keys(app, ESC)
                    app.wait('document.activeElement?.getAttribute("aria-label") === "Darstellung und Optionen"')
        if not args.windows_only:
            result['passed'].append('Themes/system theme, >=4.5 text contrast, narrow/wide geometry, 100/200% document text, overlay and menu focus trap/return')
        if args.windows_only:
            request('POST', f'/session/{app.session}/window/rect', {'width':800,'height':600})
            for _ in range(10): keys(app, CTRL, '+')
        request('POST', f'/session/{app.session}/window/rect', {'width':360,'height':args.height})
        keys(app, CTRL, 'f')
        keys(app, CTRL, SHIFT, 'o')
        app.wait('document.querySelector(".outline")')
        keys(app, CTRL, 'f')
        app.wait('!document.querySelector(".outline") && document.activeElement === document.querySelector(".search-bar input")')
        keys(app, ESC)
        request('POST', f'/session/{app.session}/window/rect', {'width':args.wide,'height':args.height})
        app.wait(f'innerWidth === {args.wide}')
        result['passed'].append('Ctrl+F restores an already open search from the narrow outline')
        native.focus(app)
        # Keyboard scrolling within a wide table and code; visible focus is retained.
        keys(app, TAB)
        app.js('const table=Array.from(document.querySelectorAll(".table-scroll")).filter(el=>el.scrollWidth>el.clientWidth).at(-1);table.scrollLeft=0;table.focus()')
        app.wait('document.activeElement?.classList.contains("table-scroll")')
        keys(app, '\ue014')  # Right arrow through WebKit's native input adapter.
        app.wait('document.activeElement.scrollLeft > 0')
        app.wait('getComputedStyle(document.activeElement).outlineStyle !== "none"')
        focus(app, 'summary')
        keys(app, SPACE)
        app.wait('document.querySelector("details").open')
        keys(app, SPACE)
        result['passed'].append('Keyboard horizontal table scrolling, visible focus and details toggle')
        press(app, '.window-maximize')
        app.wait('document.querySelector(".window-maximize").getAttribute("aria-label") === "Fenster wiederherstellen"')
        assert app.js('return !document.querySelector(".window-resize-handles")')
        if os.environ.get('HASHLINE_CAPTURE_COMPOSITOR'):
            app.js('document.querySelector(".document-scroll").scrollTop=0')
            native.move(200,120)
            time.sleep(.4)
            from compositor_capture import capture
            for monitor in os.environ['HASHLINE_CAPTURE_COMPOSITOR'].split(','):
                capture(monitor,out/f'compositor-{monitor}.png')
        keys(app, SPACE)
        app.wait('document.querySelector(".window-maximize").getAttribute("aria-label") === "Fenster maximieren"')
        assert app.js('return document.querySelectorAll(".window-resize-handle").length === 8')
        result['passed'].append('Native maximize/restore by Enter/Space; resize regions follow native state')
        press(app, '.window-minimize')
        time.sleep(.3)
        app.open(Path('tests/fixtures/accessibility.md').resolve())
        result['cliFocusAfterMinimize'] = app.js('return document.hasFocus()')
        native.focus(app)
        app.wait('document.querySelector(".titlebar").dataset.focused === "true"')
        result['passed'].append('Native minimize by keyboard and restore/focus through desktop application switcher')
        native_width = min(args.wide, 700)
        request('POST', f'/session/{app.session}/window/rect', {'width':native_width,'height':min(args.height,380)})
        app.wait(f'innerWidth === {native_width} && innerHeight === {min(args.height,380)}')
        native.focus(app)
        # Genuine compositor input supplies the Wayland seat serial required for
        # GTK move/resize. WebDriver DOM input alone cannot prove these actions.
        app.js('window.__pointer=null;addEventListener("pointermove",e=>window.__pointer={x:e.clientX,y:e.clientY})')
        if result['backend'] == 'x11':
            # X11 supports explicit window coordinates; avoid an interactive move
            # across outputs while Mutter translates Xwayland's root coordinates.
            request('POST', f'/session/{app.session}/window/rect', {
                'x':round(native.monitor_origin[0]+(native.monitor_size[0]-native_width)/2),
                'y':round(native.monitor_origin[1]+(native.monitor_size[1]-min(args.height,380))/2),
                'width':native_width,'height':min(args.height,380)})
        else:
            native.keys(0xffe9, 0xffc4)  # Move the active test window to the monitor center.
        native.center()
        if result['backend'] != 'x11': native.keys(0xff0d)
        native.move(1,1)
        try:
            app.wait('window.__pointer',timeout=3)
        except AssertionError:
            from compositor_capture import capture
            capture(native.monitor_name,out/'pointer-diagnostic.png')
            raise
        origin = app.js('return window.__pointer')
        native.move(20,20)
        moved = app.js('return window.__pointer')
        factor_x = 20 / (moved['x']-origin['x'])
        factor_y = 20 / (moved['y']-origin['y'])
        result['inputCoordinateScale'] = [factor_x,factor_y]
        def pointer_to(x,y):
            # GNOME pointer acceleration varies with the size of relative moves.
            # Use short closed-loop steps, matching the calibration distance.
            for _ in range(80):
                point = app.js('return window.__pointer')
                if abs(x-point['x']) < 1 and abs(y-point['y']) < 1: return
                dx = max(-20,min(20,x-point['x']))
                dy = max(-20,min(20,y-point['y']))
                native.move(dx*factor_x,dy*factor_y)
            point = app.js('return window.__pointer')
            assert abs(x-point['x']) < 2 and abs(y-point['y']) < 2, (x,y,point)
        pointer_to(int(native_width/2),24)
        native.button(True)
        native.move(30,30)
        native.button(False)
        native.move(1,1)
        point = app.js('return window.__pointer')
        assert abs(point['y']-25) <= 3, point
        result['drag'] = {'pointerAfterMove':point,'expectedY':25}
        result['passed'].append('Native titlebar drag follows real compositor pointer input')
        before = app.js('return {width:innerWidth,height:innerHeight}')
        pointer_to(before['width']-2,before['height']-2)
        result['resizePointer'] = app.js('return window.__pointer')
        native.button(True)
        native.move(-50,-40)
        native.button(False)
        app.wait(f'innerWidth < {before["width"]} && innerHeight < {before["height"]}')
        result['resize'] = {'before':before,'after':app.js('return {width:innerWidth,height:innerHeight}')}
        result['passed'].append('Native corner resize follows real compositor pointer input')
        pointer_to(int(app.js('return innerWidth')/2),24)
        # No delay between native clicks beyond the 150ms method cadence.
        native.button(True)
        native.button(False)
        native.button(True)
        native.button(False)
        app.wait('document.querySelector(".window-maximize").getAttribute("aria-label") === "Fenster wiederherstellen"')
        result['passed'].append('Native titlebar double-click maximizes')
        if os.environ.get('HASHLINE_CAPTURE_COMPOSITOR'):
            from compositor_capture import capture
            app.js('document.querySelector(".document-scroll").scrollTop=0')
            native.move(200,120)
            time.sleep(.4)
            capture(native.monitor_name,out/'compositor-test-window.png')
        try:
            press(app, '.window-close')
            handles = request('GET', f'/session/{app.session}/window/handles')
            assert not handles, handles
        except RuntimeError as error:
            assert any(word in str(error).lower() for word in ['no such window', 'invalid session', 'failed to connect', 'not found', 'session terminated without a reply']), str(error)
        result['passed'].append('Native close by keyboard')
        result['completed'] = True
    finally:
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        native.close()
        try: app.close()
        except RuntimeError: pass
    print(json.dumps({'label':args.label,'cases':len(result['cases']),'passed':result['passed']}, indent=2))


if __name__ == '__main__': main()
