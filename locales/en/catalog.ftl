### Bisa — the catalog's display fields (Namespace::Content).
###
### What ships in `library/` speaks for itself: an entry's name, its description and a pet's
### tagline are the file's own words, and English ships no message here. Another language may
### translate a display field by its id; a field with no message keeps the file's text.
###
###   catalog-<kind>-<slug> = <the name>          kind: agent · skill · team · channel · connector · workflow
###       .description = <the description>
###   catalog-pet-<id> = <the display name>
###       .description = <the description>
###       .tagline = <the tagline>
###
### Never here: a template's body, a skill's steps, a prompt — what is for a model stays as written.
