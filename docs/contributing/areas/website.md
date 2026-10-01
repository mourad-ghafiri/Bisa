# Website

The public site, bisa.dev: static HTML, one stylesheet and one small script, no framework, no web
font and nothing loaded from another host. `website/` **is** the site — any static host serves it as
it is — and every page in it is built, never edited by hand. The sources are in `scripts/website/`:
a fragment per page, the shared partials, and `build.mjs`, which fills each page from the
repository's own facts — the catalog's counts and names, the step kinds' words, a harness's name, the
minimum macOS — so the site cannot drift from the platform: a word the build cannot answer stops it.

## Where it lives

- `scripts/website/pages/` · `scripts/website/partials/` · `scripts/website/examples/` — a fragment per page (its title, description and address at its head), the shared head, header and footer, the worked example.
- `scripts/website/build.mjs` — assembles the site; `scripts/website/websiteModel.mjs` and `scripts/website/railModel.mjs` — its rules, pure and tested.
- `scripts/website/site.src.js` — the few behaviours, built into `website/assets/site.js`.
- `scripts/website/web-screenshots.mjs` · `scripts/website/card.mjs` — screenshots made for the web; the social card.
- `website/` — the built pages; `website/assets/site.css` — the look; `website/screenshots/` — the screenshots, with `manifest.json` the one list of them.

## Read first

- [website/README.md](../../../website/README.md) — where things are, [screenshots](../../../website/README.md#screenshots), [changing the site](../../../website/README.md#changing-the-site), the owner's four rules, putting it online.
- [Release § The website](../release.md#the-website) — the site names no version, so a release changes nothing there.
- [Terminology](../terminology.md) — the site keeps the project's vocabulary, and its guard checks it.

## Rules a change must keep

- Never edit a page under `website/` by hand: edit its source, run `just website`, and commit the sources and the built pages together ([Changing the site](../../../website/README.md#changing-the-site)).
- The app is shown by its screenshots alone — never a drawn screen of the app; where a screenshot is missing, its labelled outline stands in ([website/README.md](../../../website/README.md)).
- No command to type: the commands are the guides' and the command line reference's, linked from the site.
- No kinds named — of a step, or of what a relay carries; say what a thing does.
- The catalog's agents and teams are each folded away, never listed on the page itself.
- The site names no version: a link to the latest release is enough.
- One heading, a title and a description of its own per page; links and anchors that resolve from where each page stands; nothing from another host; images described and sized; the platform's palette — blues, cyans and the status hues, no purple — at a legible contrast; the budgets.
- A screenshot is the app's whole window as macOS captures it, made WebP for the web — at most 2000 px wide, under 600 KB — and shows nothing that is yours alone: an npub, a home folder's path, an e-mail address, a token, a usage number. An original is moved aside under `target/`, never deleted.
- Nothing here publishes the site.

## Testing a change

- `just website-check` — `node --test scripts/website/websiteModel.test.mjs scripts/website/railModel.test.mjs scripts/website/site.test.mjs`: the rules, the rail, and the committed pages held to what the sources build. It is in `just verify` and in CI's `website` job.
- `just website` — build the site and serve `website/` on `http://localhost:4321` to look at it.
- `scripts/lint-terminology website scripts/website` — the vocabulary, as the lint runs it on the whole tree.
- No journey covers the site; nothing in it reaches the node.

## Common changes

- A page's words, the look or a behaviour: [Changing the site](../../../website/README.md#changing-the-site).
- A screenshot added or replaced: [Screenshots](../../../website/README.md#screenshots), then `just website-shots`.
- A fact of the platform the site states — a count, a step kind's word, a harness's name — is rebuilt by `just website`; a number written by hand into a fragment is a bug.

## Compatibility

- Nothing on the site is part of the public contract [Compatibility](../../reference/compatibility.md) describes. Declare a site-only change as no contract change in the pull request.

## Review focus

- The sources and the built pages changed together; no hand edit under `website/` ([Code review](../review/code.md)).
- The owner's four rules, no version and no command on a page ([Docs and language](../review/docs-and-language.md)).
- A screenshot made for the web and free of anything personal ([Security review](../review/security.md)).
- The page's budgets, its images sized ([Performance review](../review/performance.md)); any new image, icon or font carries its licence ([Licences review](../review/licences.md)).
