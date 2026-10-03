/**
 * The one control a photo is picked with (ide/14 §Photos): the preview as the
 * `Avatar` every list draws, *Choose a photo* / *Change photo* / *Remove*, and
 * the hidden file input. A picked picture is scaled to the profile's square
 * before it is uploaded — the camera's file never leaves the machine — and
 * the caller is handed the `AttachmentRef` to attach, or `null` when the
 * photo is removed. A project's, an agent's and a team's **picture**, a
 * person's **face**: one field, two profiles (`PHOTO_PROFILES`).
 */

import { useRef, useState } from "react";
import type { AttachmentRef } from "../types";
import { api } from "../api";
import { Avatar } from "./Avatar";
import { Button } from "./Button";
import { PHOTO_PROFILES, PHOTO_TYPES } from "./photoModel.mjs";
import type { PhotoProfile } from "./photoModel.mjs";
import { scalePhoto } from "./photoScale";
import { sayFailure } from "./failure";
import { useToast } from "./Toast";
import { t } from "../i18n/l10n.mjs";

export function PhotoField({
  id,
  name,
  photo,
  profile = PHOTO_PROFILES.picture,
  onChange,
  disabled = false,
  size = 32,
}: {
  /** The identicon's seed when there is no photo — the record's stable id. */
  id: string;
  /** The name the identicon takes its letters from. */
  name?: string;
  photo: AttachmentRef | null;
  profile?: PhotoProfile;
  onChange: (photo: AttachmentRef | null) => void;
  disabled?: boolean;
  size?: number;
}) {
  const toast = useToast();
  const input = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  return (
    <div className="flex items-center gap-2">
      <Avatar id={id} name={name} photo={photo} size={size} />
      <input
        ref={input}
        type="file"
        accept={PHOTO_TYPES.join(",")}
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = "";
          if (!file) return;
          setUploading(true);
          // Scaled to the profile's square here, before it is uploaded (ide/14 §Photos).
          scalePhoto(file, profile)
            .then((scaled) => api.uploadAttachment(scaled))
            .then((ref) => onChange(ref))
            .catch((err: unknown) => toast.error(sayFailure("photo", t("ui-photo-field-could-not-add"), err)))
            .finally(() => setUploading(false));
        }}
      />
      <Button size="sm" disabled={disabled || uploading} onClick={() => input.current?.click()}>
        {uploading ? t("ui-photo-field-uploading") : photo ? t("ui-photo-field-change-photo") : t("ui-photo-field-choose-photo")}
      </Button>
      {photo && (
        <Button size="sm" variant="ghost" disabled={disabled || uploading} onClick={() => onChange(null)}>{t("ui-photo-field-remove")}</Button>
      )}
    </div>
  );
}
