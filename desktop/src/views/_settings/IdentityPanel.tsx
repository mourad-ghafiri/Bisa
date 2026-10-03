/**
 * Who you are on this node.
 *
 * The keypair *is* the account — there is no server that knows you, and no
 * reset link. That makes "where does this live and how do I keep it" the only
 * thing this panel really has to answer, honestly — and, beside it, what the
 * people who see you see: **your name and your face** (14-collaboration). Both
 * are your own row of this workspace's members, set through `PUT /workspace/me`,
 * and both travel with your profile to every workspace this node is a member
 * of, where they draw beside your messages the way they draw here.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { useWorkspace } from "../../shell/useWorkspaceData";
import type { AttachmentRef, WorkspaceInfo } from "../../types";
import { Card, CopyText, Field, Labelled, PHOTO_PROFILES, PhotoField, Section, TextInput, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { ownerRow, profileDraft, profileWords } from "./identityModel.mjs";
import { SaveFooter } from "./SaveFooter";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function IdentityPanel({ ws }: { ws: WorkspaceInfo }) {
  const toast = useToast();
  const workspace = useWorkspace();
  const me = ownerRow(ws);
  const [label, setLabel] = useState("");
  const [photo, setPhoto] = useState<AttachmentRef | null>(null);
  const [busy, setBusy] = useState(false);
  // The draft follows the row: a save elsewhere, or a reload, re-seeds it.
  useEffect(() => {
    const d = profileDraft(me);
    setLabel(d.label);
    setPhoto(d.photo);
  }, [me]);
  const dirty = profileDraft(me).label !== label.trim() || (profileDraft(me).photo?.sha256 ?? null) !== (photo?.sha256 ?? null);
  const discard = () => {
    const d = profileDraft(me);
    setLabel(d.label);
    setPhoto(d.photo);
  };

  const save = async () => {
    setBusy(true);
    const ok = await attempt(async () => {
      await api.setMe({ label: label.trim() ? label.trim() : null, photo });
    }, toast.error);
    setBusy(false);
    if (ok) {
      toast.ok(t("settings-identity-panel-saved-every-workspace-member-told"));
      workspace.refresh();
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <Section title={t("settings-identity-panel-profile")}>
        <Card>
          <div className="flex flex-col gap-3">
            <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{profileWords()}</p>
            <Field label={t("settings-identity-panel-name")} hint={t("settings-identity-panel-what-people-list-every-message-write")}>
              <TextInput value={label} placeholder={t("settings-identity-panel-ada")} maxLength={64} onChange={(e) => setLabel(e.target.value)} />
            </Field>
            {/* A picker is several buttons, not one control: a caption over it, never a <label> around it. */}
            <Labelled label={t("settings-identity-panel-photo")} hint={t("settings-identity-panel-small-square-scaled-here-before-kept")}>
              <PhotoField id={ws.pubkey} name={label || t("settings-identity-panel-you")} photo={photo} profile={PHOTO_PROFILES.face} onChange={setPhoto} disabled={busy} />
            </Labelled>
            <SaveFooter form="identity-profile" dirty={dirty} saving={busy} onSave={() => void save()} onDiscard={discard} />
          </div>
        </Card>
      </Section>

      <Section title={t("settings-identity-panel-identity")}>
        <Card>
          <div className="flex flex-col gap-3">
            {/* A copy button is not a field: inside a <label>, a click on the caption would copy. */}
            <Labelled /* for the machine */ label="npub" hint={t("settings-identity-panel-share-added-someone-else-s-workspace")}>
              <CopyText value={ws.npub} />
            </Labelled>
            <Labelled label={t("settings-identity-panel-public-key-hex")}>
              <CopyText value={ws.pubkey} />
            </Labelled>
          </div>
        </Card>
      </Section>

      <Section title={t("settings-identity-panel-where-things-live")}>
        <Card>
          <div className="flex flex-col gap-3">
            <Labelled label={t("settings-identity-panel-workspace-directory")} hint={t("settings-identity-panel-goals-journals-agents-their-memory")}>
              <CopyText value={ws.data_dir} />
            </Labelled>
            <div>
              <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-identity-panel-secret-key")}</span>
              {/* for the machine: the identity directory's path */}
              <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-identity-panel-secret-key-held", { dir: <code>{ws.data_dir}/identity/</code>, code: (inner) => <code>{inner}</code> })}</p>
              <p className="mt-1 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-identity-panel-no-export-command-yet-back-up")}</p>
            </div>
          </div>
        </Card>
      </Section>
    </div>
  );
}
