# bisa.dev — the website

The public site of Bisa, *A Decentralized Agentic IDE*: static HTML, one stylesheet and one small
script, no framework, no web font and nothing loaded from anyone else. This folder **is** the site —
any static host serves it as it is, at the root of `bisa.dev` — and it reads whole straight from the
disk too: open `index.html` in a browser. Every page but `404.html` writes its links from where it
stands, and the script is a plain one, not a module.

```sh
just website          # build it, then serve this folder on http://localhost:4321
just website-check    # the site's tests and the check that it is what its sources build
```

## Where things are

| | |
|---|---|
| `index.html`, `features/`, `harnesses/`, `security/`, `download/`, `404.html` | the pages — **built, never edited by hand** |
| `robots.txt`, `sitemap.xml`, `site.webmanifest`, `favicon.*`, `apple-touch-icon.png` | built too; the icons are copied from `logo/` and the desktop's icon set |
| `assets/site.css` | the look: the desktop's Glass tokens, light and dark following the system |
| `assets/site.js` | the few behaviours — the opening's slideshow, the tour's rail, shown once the reader is past the opening, the reading line, the asks' filter, the small screen's menu, Moonrice — **built** from `scripts/website/site.src.js` and `railModel.mjs` into one plain script; every page reads whole without it |
| `assets/social-card.png` | the picture a shared link shows, rendered by `just website-card` |
| `screenshots/` | the app's screenshots, which you add — see below |

The sources are in `scripts/website/`: a fragment per page under `pages/` (its title, description
and address at its head), the shared `partials/`, the worked example under `examples/`, and
`build.mjs`, which assembles them. A fragment says `{{count:templates}}`, `{{kinds}}`,
`{{figure:inbox}}` and the like, and the build fills each from the repository itself — the
catalog's counts and names, the designer's step kinds and their words, a harness's own name, the
hundred asks of the scenarios guide, the minimum macOS — so the site cannot drift from the
platform: a word the build cannot answer stops it, in a sentence.

## Screenshots

`screenshots/manifest.json` is the one list, and `screenshots/README.md` (built from it) says what
each one should show. Capture the app's whole window as macOS does it (⇧⌘4, then Space, then click
the window) — its own title bar, corners and shadow are part of the picture — save it as
`screenshots/<id>.png`, and run `just website`. The capture first becomes `screenshots/<id>.webp`,
made for the web by `cwebp`, Google's WebP encoder: at most 2000 px wide, its transparent shadow
kept, about a tenth of the weight; the original is moved aside under
`target/website-screenshot-originals/`, never deleted (`just website-shots` does that step alone).
It then appears on the site as it is, with no frame drawn around it, and in the repository's
README. Until then, each place shows a
labelled outline of the app's window instead — never a broken image. A file whose name the manifest
does not know is left out with a note naming the names it does (and `just website-check` fails on
it): rename it, or add its entry. A screenshot over 600 KB or wider than 2000 px fails
`just website-check` until `just website-shots` has made it for the web. Leave out what is yours alone: your npub, your home folder's path, an e-mail
address, a token, a usage number.

## Changing the site

1. Edit a fragment under `scripts/website/pages/`, a partial, `assets/site.css`, or the behaviours in
   `scripts/website/site.src.js` and their rules in `railModel.mjs`.
2. `just website` — rebuild and look.
3. `just website-check` — the guard holds every page to one heading, a title and a description of
   its own, its canonical address, the social tags, links and anchors that resolve from where each
   page stands, nothing from another host, images that are described and sized, the platform's
   palette (blues, cyans and the status hues — no purple) at a legible contrast, the budgets, and the
   project's vocabulary.
4. Commit the sources and the built pages together.

The opening's slideshow comes from one list, `slides` in the home page's front matter: each
screen's label and the screenshots that may show it — the first of them that is here; until one
is, the slide is the outlined window naming the file to add.
Each screen carries its label as a caption. It moves on by itself every few seconds, with no tabs
and no Pause — the owner's choice — and holds still where the visitor's system asks for reduced
motion, while the pointer rests on it, while something inside it has the focus, and while it is
scrolled out of sight; two arrows, shown while the pointer is over it (always on a touch screen),
go back and on, and the first press of one, or the first swipe, stops it for good.
The repository's README opens with the same screens as one animated image,
`assets/readme-slideshow.webp`, which `just website-shots` makes again with `img2webp` whenever a
screen changes — `assets/readme-slideshow.json` records which it holds, and `just website-check`
fails while it is behind. A screen with no screenshot is left out of it until it has one.

Four rules the owner set, each held by the guard:

- **The app is shown by its screenshots alone.** Never draw a screen of the app on the site — no
  mock-up of a canvas, a card or a window's contents. Where a screenshot is not there yet, its
  labelled outline stands in.
- **No command to type.** The site presents the app by its screens and its words; the commands are
  the guides' and the command line reference's, linked from the site.
- **No kinds named** — of a step, or of what a relay carries. Say what a thing does.
- **The catalog's agents and teams are each folded away** under *Every agent in the catalog* and
  *Every team in the catalog*, never listed on the page itself.

The site names no version: a link to the latest release is enough, and a number written here would
go stale.

## Putting it online

Nothing here publishes. Point `bisa.dev` at a static host serving this folder — a GitHub Pages
site deployed from a workflow that uploads `website/`, or any other — and, for search, verify the
domain with Google Search Console and Bing Webmaster Tools by DNS, so no tag is added to the pages.
The sitemap is `https://bisa.dev/sitemap.xml`.
