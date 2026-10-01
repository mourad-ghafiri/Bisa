/**
 * Git config as a form reasons about it. The node serves the
 * platform's schema — the keys it will read and write, each with a kind, a
 * label and the layers it lives at — beside every key's `local`, `global` and
 * `effective` value. This module turns that into rows for one layer, turns a
 * person's edits back into the `{set, unset}` write the node takes, and
 * validates a value the way the vcs crate's schema will, so the form refuses
 * before the round trip. The key list and kinds are checked against the Rust
 * schema in the test; the schema itself is never mirrored — it is served.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The rows a form shows for one layer. `value` is that layer's own value;
 * `inherited` is what the global layer would give a local row (`null` in the
 * global form); `effective` is what git resolves; `editable` is whether the
 * key lives at this layer at all (`init.defaultBranch` is global only).
 * @param {readonly {key: string, kind: {type: string, options?: string[]}, label: string, hint: string, scopes: string[]}[]} schema
 * @param {readonly {key: string, local?: string | null, global?: string | null, effective?: string | null}[]} entries
 * @param {"global" | "local"} scope
 */
export function configRows(schema, entries, scope) {
  const byKey = new Map(entries.map((e) => [e.key, e]));
  return schema.map((def) => {
    const e = byKey.get(def.key) ?? {};
    const value = (scope === "global" ? e.global : e.local) ?? null;
    return {
      key: def.key,
      label: def.label,
      hint: def.hint,
      kind: def.kind ?? { type: "text" },
      value,
      inherited: scope === "local" ? (e.global ?? null) : null,
      effective: e.effective ?? (scope === "global" ? value : null),
      // A key whose schema names no scopes is shown and never written.
      editable: (def.scopes ?? []).includes(scope),
    };
  });
}

/**
 * The write a set of edits amounts to. `edits` maps a key to the text the
 * person left in its control; an emptied value on a row that had one is an
 * unset (the key falls through to the layer beneath), a changed non-empty
 * value is a set, everything else is nothing. Keys the layer cannot hold are
 * never written.
 */
export function diffWrites(rows, edits) {
  const set = {};
  const unset = [];
  for (const row of rows) {
    if (!row.editable || !(row.key in edits)) continue;
    const next = String(edits[row.key] ?? "").trim();
    const current = row.value ?? "";
    if (next === "" && current !== "") unset.push(row.key);
    else if (next !== "" && next !== current) set[row.key] = next;
  }
  return { set, unset };
}

/** Whether a write changes anything at all. */
export function isEmptyWrite(write) {
  return Object.keys(write.set).length === 0 && write.unset.length === 0;
}

/**
 * The schema's rule, ahead of the round trip: a bool is true/false, a choice
 * is one of its words, a text is one line and not option-shaped, and the two
 * identity keys pass the identity rule. An empty value is never a problem —
 * it means "inherit". Returns the reason or `null`.
 */
export function validateValue(kind, key, value) {
  const v = String(value ?? "").trim();
  if (v === "") return null;
  switch (kind.type) {
    case "bool":
      return v === "true" || v === "false" ? null : t("work-git-config-true-false");
    case "choice":
      return (kind.options ?? []).includes(v) ? null : t("work-git-config-one", { options: (kind.options ?? []).join(", ") });
    default:
      if (v.startsWith("-")) return t("work-git-config-cannot-start-dash");
      if (/[\r\n]/.test(v)) return t("work-git-config-one-line");
      if (key === "user.email") {
        if (/\s/.test(v)) return t("work-git-config-email-has-no-spaces");
        const at = v.split("@").length - 1;
        if (at !== 1 || v.startsWith("@") || v.endsWith("@")) return t("work-git-config-email-has-one-something-both-sides");
      }
      return null;
  }
}

/** Every problem in a set of edits, by key. */
export function validateEdits(rows, edits) {
  const problems = {};
  for (const row of rows) {
    if (!(row.key in edits)) continue;
    const p = validateValue(row.kind, row.key, edits[row.key]);
    if (p) problems[row.key] = p;
  }
  return problems;
}

/** The identity the global layer holds, or `null`. */
export function globalIdentityOf(entries) {
  const g = (key) => entries.find((e) => e.key === key)?.global ?? null;
  const name = g("user.name");
  const email = g("user.email");
  return name && email ? { name, email } : null;
}

/**
 * Git needs both keys of the identity pair. A write that sets one while the
 * other is unset everywhere — not in this repository, not inherited — is
 * refused before the round trip with the sentence the form shows; a pair
 * left whole is fine, and so is a key the layer beneath still answers.
 * @param {readonly {key: string, value?: string | null, inherited?: string | null, editable?: boolean}[]} rows the local layer's rows
 * @param {{set: Record<string, string>, unset: string[]}} write
 * @returns {string | null}
 */
export function identityPairProblem(rows, write) {
  const after = (key) => {
    if (write.unset.includes(key)) {
      const row = rows.find((r) => r.key === key);
      return row?.inherited ? String(row.inherited) : "";
    }
    if (key in write.set) return write.set[key];
    const row = rows.find((r) => r.key === key);
    return row?.value ? String(row.value) : row?.inherited ? String(row.inherited) : "";
  };
  const touched = ["user.name", "user.email"].some((k) => k in write.set || write.unset.includes(k));
  if (!touched) return null;
  const name = after("user.name");
  const email = after("user.email");
  if (name && !email) return t("work-git-config-git-needs-email-beside-name-set");
  if (email && !name) return t("work-git-config-git-needs-name-beside-email-set");
  return null;
}

/** `Name <email>`, the one way a pair is printed. */
export function formatIdent(ident) {
  return `${ident.name} <${ident.email}>`;
}
