/**
 * Marking a device's screen for an agent (ide/19): the wand's state, the
 * capture taken the moment a rectangle is drawn — the screen changes under
 * a running app, so the picture is the one the person marked — the note
 * asked for it, and the draft of captures kept per device for the session.
 * The rules are `captureModel.mjs`'s; this hook only holds and asks.
 */

import { useEffect, useState } from "react";
import type { MobileDevelopmentShot } from "../../types";
import { failureText, useToast } from "../../ui";
import { takeDeviceShot } from "../../shell/deviceShots";
import { useSessionDraft } from "../_work/gitPanelStore";
import { EMPTY_CAPTURE_DRAFT, addCapture, capturesKey } from "./captureModel.mjs";
import type { CaptureDraft, Mark } from "./captureModel.mjs";

export interface PendingCapture {
  readonly mark: Mark | null;
  readonly shot: MobileDevelopmentShot;
}

export interface DeviceCapture {
  readonly inspecting: boolean;
  readonly setInspecting: (on: boolean) => void;
  /** A rectangle drawn and its picture taken, waiting for its note. */
  readonly pending: PendingCapture | null;
  /** The picture is being taken. */
  readonly taking: boolean;
  readonly draft: CaptureDraft;
  readonly setDraft: (next: CaptureDraft | ((prev: CaptureDraft) => CaptureDraft)) => void;
  /** A rectangle was drawn (`null` for the whole screen): capture the screen now, then ask for the note. */
  readonly begin: (mark: Mark | null) => void;
  readonly add: (note: string) => void;
  readonly cancel: () => void;
  /** The captures left the tray — sent, attached or cleared. */
  readonly done: () => void;
}

export function useDeviceCapture({ wid, deviceId, label, up }: { wid: string; deviceId: string; label: string; up: boolean }): DeviceCapture {
  const toast = useToast();
  const [inspecting, setInspecting] = useState(false);
  const [pending, setPending] = useState<PendingCapture | null>(null);
  const [taking, setTaking] = useState(false);
  const [draft, setDraft] = useSessionDraft<CaptureDraft>(capturesKey(`workstream:${wid}`, deviceId), EMPTY_CAPTURE_DRAFT);

  // A device that went down has no screen to mark.
  useEffect(() => {
    if (!up) {
      setInspecting(false);
      setPending(null);
    }
  }, [up]);

  const begin = (mark: Mark | null) => {
    if (taking || !up) return;
    setTaking(true);
    void takeDeviceShot(deviceId)
      .then((shot) => setPending({ mark, shot }))
      .catch((e: unknown) => toast.error(failureText("workbench", "use-device-capture-failed", e)))
      .finally(() => setTaking(false));
  };
  const add = (note: string) => {
    if (!pending) return;
    const taken = pending;
    setDraft((prev) => addCapture(prev, { shot: taken.shot.attachment, mark: taken.mark, label, width: taken.shot.width ?? null, height: taken.shot.height ?? null }, note));
    setPending(null);
  };
  const cancel = () => setPending(null);
  const done = () => {
    setDraft(EMPTY_CAPTURE_DRAFT);
    setPending(null);
    setInspecting(false);
  };

  return { inspecting, setInspecting, pending, taking, draft, setDraft, begin, add, cancel, done };
}
