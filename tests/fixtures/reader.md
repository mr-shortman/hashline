# Hashline · Lesetest

Eine ruhige **Markdown-Ansicht** mit _Hervorhebung_, ~~Streichung~~ und `Inline-Code`.
Dieser Softbreak bleibt Teil desselben Absatzes.  
Dieser Hardbreak beginnt eine neue Zeile.

## Übersicht

- Ein erster Punkt
  - Ein verschachtelter Punkt
- [x] Fertige Aufgabe
- [ ] Offene Aufgabe (schreibgeschützt)

1. Lesen
2. Auswählen und kopieren

> Gute Typografie hilft beim Lesen.
>
> Auch über mehrere Absätze hinweg.

## Tabelle

| Sprache | Beispiel       | Suche |
| ------- | -------------- | ----- |
| Deutsch | Grüße aus Köln | Nadel |
| 日本語  | こんにちは     | Nadel |

## Code

```typescript
const message: string = 'Nadel';
console.log(message);
```

```unknown-language
<unverändert> & gut lesbar
```

## Links und Bilder

[Zur Tabelle](#tabelle) · [Relative Datei](zweite%20Datei.md#ziel) · [Extern](https://example.com)

![Lokales Bild](pixel.png)
![Fehlendes Bild](missing.png)
![Remote-Bild](https://example.com/tracker.png)

## Doppelt

Erster Abschnitt.

## Doppelt

Zweiter Abschnitt.

## Grüße 日本語

Unicode-Überschrift.

## !!!

Leerer Slug.

<details><summary>Weitere Informationen</summary>

VerborgeneNadel zählt erst nach dem Aufklappen zur Suche.

</details>

---

Ein letzter Absatz für die Auswahl über Blockgrenzen.
