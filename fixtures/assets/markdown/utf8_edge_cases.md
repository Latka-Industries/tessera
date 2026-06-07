# Unicode and encoding edge cases

Smart “quotes” and ‘apostrophes’ from Word exports.

## Combining characters

café (precomposed) vs cafe\u0301 (e + combining acute).

## Bidirectional text

English intro, then Hebrew snippet: שלום, then back to English.

## Zero-width and special spaces

Word|non-breaking|space — figure dash – en dash — em dash —.

## Emoji sequences

Family: 👨‍👩‍👧‍👦  Flags: 🇺🇸  Keycap: 1️⃣

## Replacement and escape

Literal backslash-n: \n not a newline. Tab	separated values in prose.

## Right-to-left override test

\u202E reversed text marker (U+202E) — importers should normalize or strip bidi overrides.

## Currency and numbers

€1 234,56 · ¥1000 · −40 °C · 3.14159 × 10⁶

## Empty elements

<!-- HTML comment only -->

<span></span>

## Lone surrogate pair territory (valid UTF-8)

Musical symbols: 𝄞 𝄢

End.
