# Third-party notices

This file is the hand-written part of the platform's third-party notices: the marks it draws, the
fonts it bundles, and the material embedded in packages that their own metadata does not name.
`THIRD-PARTY-NOTICES.md` — `just gen-notices` — carries this file verbatim and then every Rust crate
and npm package the shipped binaries carry, with their licence texts and notices.

## The Bisa mark — ours

The platform's own mark — `logo/logo.svg`, a blue glass squircle holding the split B, shown in the app by `desktop/src/ui/PlatformMark.tsx` and rasterised into the OS icon set under `desktop/src-tauri/icons/` by `just app-icon` — is original work of this project, from no set and no other mark. It carries the repository's licence (MIT, `LICENSE`).

The desktop draws each coding harness's own mark (`desktop/src/ui/harnessMarks.tsx`) from two free icon sets. The marks are the harnesses' trademarks, used only to identify the harness they name.

## LobeHub icons — MIT

The Claude Code, Codex, GitHub Copilot, Grok, Goose and pi marks are the SVG paths of `@lobehub/icons-static-svg` (https://github.com/lobehub/lobe-icons).

```
MIT License

Copyright (c) 2023 LobeHub

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The pi mark is pi's own logo (https://pi.dev, `badlogic/pi-mono`, MIT), in the same geometry.

## Simple Icons — CC0 1.0

The OpenCode and Cursor marks are the SVG paths of Simple Icons (https://simpleicons.org), released under CC0 1.0 Universal. Simple Icons asks that its icons be used to identify the brand they depict, and nothing else.

## Fonts — SIL Open Font License 1.1, and one MIT

Fonts are not software the platform licenses: each stays under its own licence, which permits
embedding and redistribution and asks that the font keep its name and its notice. The OFL 1.1 text
is in `THIRD-PARTY-NOTICES.md` §Licence texts (as shipped by the `@fontsource` packages).

**The interface and code faces** — `@fontsource` packages, OFL-1.1, each with its LICENSE beside it:

- Inter — Copyright 2016 The Inter Project Authors (https://github.com/rsms/inter)
- Atkinson Hyperlegible — Copyright 2020 Braille Institute of America, Inc.
- JetBrains Mono — Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono)
- IBM Plex Mono — Copyright © 2017 IBM Corp. (https://github.com/IBM/plex)

**The drawing faces** — copied from `@excalidraw/excalidraw`'s `dist/prod/fonts` by
`desktop/scripts/sync-excalidraw-assets.mjs` into the bundle. The package carries the fonts without
their licence files, so each is named here with its own project as its holder, under the licence
that project publishes:

- Excalifont — the Excalidraw project (https://github.com/excalidraw/excalidraw), OFL-1.1
- Virgil — Ellinor Rapp (https://github.com/excalidraw/virgil), OFL-1.1
- Cascadia Code — Microsoft Corporation (https://github.com/microsoft/cascadia-code), OFL-1.1
- Liberation Sans — Red Hat, Inc. (https://github.com/liberationfonts/liberation-fonts), OFL-1.1
- Lilita One — Juan Montoreano (https://fonts.google.com/specimen/Lilita+One), OFL-1.1
- Nunito — The Nunito Project Authors (https://github.com/googlefonts/nunito), OFL-1.1
- Assistant — Ben Nathan (https://github.com/hafontia-zz/Assistant), OFL-1.1
- Xiaolai — Xiaolai Li (https://github.com/lxgw/kose-font), OFL-1.1
- Comic Shanns — Shannon Miwa (https://github.com/shannpersand/comic-shanns), MIT

## Monaco Editor — MIT, with attributed material

`monaco-editor` is MIT (Microsoft Corporation); its `ThirdPartyNotices.txt` ships in
`THIRD-PARTY-NOTICES.md` §Notices shipped by packages. Two things inside it carry an attribution
licence the platform reproduces here:

- The TypeScript worker embeds the DOM standard's descriptions — Copyright © WHATWG (Apple, Google,
  Mozilla, Microsoft), licensed under the Creative Commons Attribution 4.0 International License
  (https://creativecommons.org/licenses/by/4.0/).
- The editor's icon font, `codicon.ttf`, is Microsoft's Codicons (https://github.com/microsoft/vscode-codicons):
  the icons under the Creative Commons Attribution 4.0 International License, the code under MIT.

Attribution is all these ask; the platform's own licence is unaffected.

## Mozilla Public License 2.0 files

Two Rust crates the binaries link are MPL-2.0, used unmodified from crates.io; MPL is file-level, so
those files stay MPL and their source is where the notices say (§3.2): `attohttpc`
(https://crates.io/crates/attohttpc) in the `bisa` binary, and `option-ext`
(https://crates.io/crates/option-ext) in both binaries. The MPL 2.0 text is in
`THIRD-PARTY-NOTICES.md` §Licence texts.
