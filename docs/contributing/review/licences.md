# Licence review

For any change that adds, removes or upgrades a dependency, or brings in material the project did not
write — code, a font, an icon, an image, a template, a translation. The rules are in
[Release — Licence](../release.md#licence) and [Add a dependency](../recipes.md#28-add-a-dependency).

## Contributions
- [ ] The contribution is the author's to give, under the project's MIT licence; nothing in it is copied
  from a source whose licence does not allow it.
- [ ] Code adapted from elsewhere names its source and licence beside it, and the licence is on the
  allow list.

## Dependencies
- [ ] The licence is on the allow list in `deny.toml` — added to **both** copies (the root and
  `desktop/src-tauri/deny.toml`) with a comment, if new; never a copyleft licence for shipped code.
- [ ] `just licence-gate` is green; `THIRD-PARTY-NOTICES.md` was regenerated (`just gen-notices`) and
  committed.
- [ ] The dependency is needed: nothing already in the tree does the job.
- [ ] After a Rust dependency change, the hakari table was regenerated (`just hakari`).

## Assets
- [ ] A font, icon, image or mark carries its licence in `NOTICES.md`; brand marks of other products
  follow their owners' terms and appear only as text or under their published licence.
- [ ] Screenshots and examples contain nothing that belongs to somebody else's account.

## The verdict
- [ ] The reviewer names each new licence the change brings in and confirms it is allowed.
