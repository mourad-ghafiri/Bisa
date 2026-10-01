# bisa-addon

The library an addon runs against inside Bisa's desktop — `window.bisa`, promises over the one
door an addon has: `postMessage` to the window that hosts it. The platform serves this file at every
addon's root, so your page loads it as

```html
<script src="bisa-addon.js"></script>
```

and never ships a copy (a bundle that carries a file of that name is refused at install). The types
are `bisa-addon.d.ts`; a starter addon is `../template/`. What each method needs the person to have
granted, what it answers, and what it never does, is `docs/reference/addon-api.md`; the walls an
addon runs behind are `docs/architecture/18-addons.md`; the developer's guide is `docs/guide/addons.md`.

This package is never published: the copy that runs is the platform's, always.
