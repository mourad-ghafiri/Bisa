/**
 * A project group's adornment: the photo a person hangs on a group,
 * kept in the workspace-scoped `rail.groups` setting keyed by group name. A
 * group is still just the distinct `Project.group` values — this is display
 * data hung off that name, exactly as a project's photo is display data on the
 * project. Pure: no React, no I/O.
 *
 * Shape: `{ [groupName]: { photo?: AttachmentRef } }`.
 */

/** The photo set on a group, or `null`. */
export function groupPhoto(meta, name) {
  return meta?.[name]?.photo ?? null;
}

/**
 * The map with `name`'s photo set, or removed when `ref` is null. An entry that
 * ends up empty is dropped, so the map holds only groups a person adorned.
 */
export function withGroupPhoto(meta, name, ref) {
  const next = { ...(meta ?? {}) };
  const entry = { ...(next[name] ?? {}) };
  if (ref) entry.photo = ref;
  else delete entry.photo;
  if (Object.keys(entry).length === 0) delete next[name];
  else next[name] = entry;
  return next;
}

/**
 * The map after a group is renamed: `old`'s entry moves to `next` (merged onto
 * anything already there), and `old` is dropped. A no-op — a plain copy — when
 * the names match or `old` has no entry.
 */
export function renameGroupMeta(meta, old, next) {
  const m = meta ?? {};
  if (old === next || !m[old]) return { ...m };
  const out = { ...m };
  out[next] = { ...(out[next] ?? {}), ...m[old] };
  delete out[old];
  return out;
}
