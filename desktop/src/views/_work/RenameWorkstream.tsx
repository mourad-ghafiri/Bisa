/**
 * Renaming a workstream (ide/07): a name is a label over the branch, and an
 * empty one gives the branch its title back. One commit rule
 * (`useRenameWorkstream`) under the field that replaces the Workstreams
 * panel's title in place — the rail's rows rename inline through the same
 * API call, so the two never disagree about what an empty name means.
 */

import { useRef, useState } from "react";
import { api } from "../../api";
import { TextInput, failureText, useToast } from "../../ui";
import { renameBody, renamedWords, renames } from "./workstreamCardModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The one rule (`workstreamCardModel.renameBody`): trim, `null` for empty,
 * one toast either way — and one write at a time: Enter and the blur that
 * follows it are one rename, never two.
 */
function useRenameWorkstream(wid: string, onRenamed?: () => void) {
  const toast = useToast();
  const writing = useRef(false);
  return async (value: string) => {
    if (writing.current) return;
    writing.current = true;
    const body = renameBody(value);
    try {
      await api.patchWorkstream(wid, body);
      toast.ok(renamedWords(body));
      onRenamed?.();
    } catch (e) {
      toast.error(failureText("work", "rename-workstream-failed", e));
    } finally {
      writing.current = false;
    }
  };
}

/** The panel's title while it is being renamed: Enter commits, Escape lets go, leaving commits. */
export function RenameWorkstreamField({
  wid,
  name,
  placeholder,
  onDone,
}: {
  wid: string;
  /** The current name, or `null` when the branch titles it. */
  name: string | null;
  /** What an empty field means — the branch. */
  placeholder: string;
  onDone: () => void;
}) {
  const rename = useRenameWorkstream(wid, onDone);
  const [value, setValue] = useState(name ?? "");
  const commit = () => {
    if (!renames(name, value)) return onDone();
    void rename(value);
  };
  return (
    <TextInput
      autoFocus
      aria-label={t("work-rename-workstream-workstream-name")}
      value={value}
      placeholder={placeholder}
      className="h-7 min-w-0 flex-1 font-mono text-sm"
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") onDone();
      }}
      onBlur={commit}
    />
  );
}
