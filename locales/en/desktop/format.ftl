### Bisa — the desktop: numbers, dates and spans as sentences
### (`desktop/src/i18n/format.mjs`). A span's unit letters are messages so a
### language may spell them; the digits are Fluent's, in the locale's shape.

format-just-now = just now
format-today = Today
format-yesterday = Yesterday

# A moment relative to now: "5m" for one that passed, "in 6h" for one to come.
format-relative =
    { $direction ->
        [future] in { $span }
       *[past] { $span }
    }
# A moment that passed, as the tail of a sentence.
format-ago = { $span } ago
# A date as the tail of a sentence.
format-on-date = on { $date }

format-span-seconds = { $n }s
format-span-minutes = { $n }m
format-span-hours = { $n }h
format-span-days = { $n }d
