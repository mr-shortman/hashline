"""Native WebKit contracts for progressive sections; same driver setup as smoke.py."""
import json
import re
from pathlib import Path
import sys
import tempfile
import time
from smoke import Desktop


def main():
    with tempfile.TemporaryDirectory(prefix='hashline-sections-') as directory:
        root = Path(directory)
        path = root / 'sections.md'
        source = '# Begin\n\n[jump](#last)\n\n' + ''.join(
            f'## Section {i}\n\nPrefix **needle** suffix [reference][end].\n\n- list\n- text\n\n'
            for i in range(3000)) + '# Last\n\nTHE-END\n\n[end]: destination.md\n'
        path.write_text(source)
        replacement = root / 'replacement.md'
        replacement.write_text('# Replacement\n\nOnly this text.')
        app = Desktop(sys.argv[1], [str(path)])
        passed = []
        try:
            app.wait("document.querySelector('article a')")
            app.js("document.querySelector('article a').click()")
            app.wait("document.activeElement?.id === 'doc-last'")
            app.wait("document.querySelector('article').dataset.renderState === 'complete'", 60)
            rect = app.js("return document.getElementById('doc-last').getBoundingClientRect().top")
            assert 0 <= rect < 780, rect
            passed.append('Deferred fragment target is inserted, focused and remains visible')
            app.js("""const p=document.querySelector('article p strong').parentNode;
              const r=document.createRange();r.selectNodeContents(p);
              getSelection().removeAllRanges();getSelection().addRange(r);""")
            selected = app.js('return getSelection().toString()')
            time.sleep(.2)
            assert app.js('return getSelection().toString()') == selected
            passed.append('Inline selection survives deferred highlighting')
            app.js("getSelection().removeAllRanges();document.querySelector('[aria-label=Suche]').click()")
            app.js("const input=document.querySelector('.search-bar input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'Prefix needle suffix');input.dispatchEvent(new Event('input',{bubbles:true}))")
            app.wait("document.querySelector('.search-count').textContent === '1 / 3000'")
            app.js("document.querySelector('[aria-label=\"Vorheriger Treffer\"]').click()")
            app.wait("document.querySelector('.search-count').textContent === '3000 / 3000'")
            painted = app.js("return CSS.highlights?.get('search')?.size ?? null")
            assert painted is None or 0 < painted < 3000, painted
            passed.append('Document-wide inline search with bounded visible highlights and last-match navigation')
            app.js("document.querySelector('[aria-label=\"Suche schließen\"]').click()")
            app.wait("!document.querySelector('.search-bar')")
            app.js("document.querySelector('.document-scroll').focus();window.dispatchEvent(new KeyboardEvent('keydown',{key:'a',ctrlKey:true,bubbles:true}))")
            selected = app.js('return getSelection().toString()')
            assert 'Begin' in selected and 'THE-END' in selected and 'Section 1500' in selected
            assert set(re.findall(r'Section (\d+)', selected)) == {str(i) for i in range(3000)}
            passed.append('Select-all includes skipped sections through the last paragraph')
            app.js("getSelection().removeAllRanges();const link=document.querySelector('article a');link.dataset.link='#section-2900';link.click()")
            time.sleep(.4)
            before = app.js("return document.getElementById('doc-section-2900').getBoundingClientRect().top")
            path.write_text('# Inserted\n\nNew paragraph.\n\n' + source + '\nRELOADED\n')
            app.wait("document.querySelector('article').textContent.includes('RELOADED')", 60)
            app.wait("document.querySelector('article').dataset.renderState === 'complete'", 60)
            time.sleep(.3)
            after = app.js("return document.getElementById('doc-section-2900').getBoundingClientRect().top")
            assert 0 <= before < 780 and abs(before - after) < 8, (before, after)
            passed.append('Reload anchor and offset survive changed section boundaries')
            # Same parsed content under another path retains safe text sections.
            duplicate = root / 'duplicate.md'
            duplicate.write_text(path.read_text())
            app.js("window.__retainedHeading=document.getElementById('doc-section-1500')")
            app.open(duplicate)
            app.wait("document.querySelector('article').dataset.renderState === 'complete'", 60)
            assert app.js("return window.__retainedHeading === document.getElementById('doc-section-1500')")
            assert app.js("return Number(document.querySelector('article').dataset.reusedSections) > 0")
            app.open(replacement)
            app.wait("document.querySelector('article').dataset.renderState === 'complete'")
            assert app.js("return document.querySelector('article').textContent.includes('THE-END')") is False
            passed.append('Replacement releases previous sections without late insertion')
            # Since P2.4 the search reads the parser's text buffer rather than
            # the DOM, so a complete result arrives while the build is still
            # running. Needs a document whose build takes noticeably longer than
            # the search: the 3000-heading file above is thirty sections and is
            # finished before the query can be typed.
            large = root / 'large.md'
            large.write_text(source * 6)
            app.open(large)
            # One round trip: `wait` would poll past the moment this observes.
            observed = app.js_async("""const done = arguments[arguments.length - 1];
              window.dispatchEvent(new KeyboardEvent('keydown',{key:'f',ctrlKey:true,bubbles:true}));
              const input = document.querySelector('.search-bar input');
              Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'Prefix needle suffix');
              input.dispatchEvent(new Event('input',{bubbles:true}));
              const counter = document.querySelector('.search-count');
              const poll = () => {
                if (/^\\d+ \\/ \\d+$/.test(counter.textContent || '')) {
                  const article = document.querySelector('article');
                  done({
                    count: counter.textContent,
                    state: article.dataset.renderState,
                    built: article.querySelectorAll('[data-populated=true]').length,
                    sections: article.querySelectorAll('.markdown-section').length,
                  });
                } else setTimeout(poll, 2);
              };
              poll();""")
            assert observed['count'] == '1 / 18000', observed
            assert observed['state'] != 'complete', observed
            assert observed['built'] < observed['sections'], observed
            passed.append('Full search result before the document is built')
        finally:
            app.close()
            output = Path(sys.argv[2]) if len(sys.argv) > 2 else Path('test-results/native-sections.json')
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps({'passed': passed, 'complete': len(passed) == 6}, indent=2) + '\n')
        print(json.dumps(passed, indent=2))


if __name__ == '__main__':
    main()
