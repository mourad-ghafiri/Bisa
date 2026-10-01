/**
 * A device's document in the Project IDE (ide/19): a simulator, an emulator
 * or a phone mirrored beside the code — the screen read from the node a few
 * times a second — with a bar over it that runs the checkout's app on it
 * (in a terminal the shell opens; the node composes the run line),
 * hot reloads, hot restarts and stops it (`r`, `R`, `q` to that terminal),
 * brings the Simulator window forward, captures the screen (copy, save,
 * attach) and turns the wand on; under it the note box a drawn rectangle
 * waits in and the capture tray. The mirror is watch-only: a touch is made
 * in the Simulator or emulator window, one click away.
 *
 * The device's facts are the devices store's; the run terminal is the
 * terminals store's, found by workstream and device; the captures are
 * `useDeviceCapture`'s. The body carries `data-device-doc`.
 */

import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { refreshDevices, useDevices } from "../../shell/devicesStore";
import { attachDeviceShot, copyDeviceShot, saveDeviceShot, takeDeviceShot } from "../../shell/deviceShots";
import { mobileDevelopmentRunSession } from "../../shell/terminalsModel.mjs";
import { openTerminalIn, useTerminals, writeToTerminalTab } from "../../shell/useTerminals";
import { useVisible } from "../../shell/visibility";
import { Button, EmptyState, ICON, Menu, Pending, Tooltip, cn, useToast } from "../../ui";
import type { MenuItem } from "../../ui";
import { CaptureNoteBox, CaptureTray } from "./CaptureTray";
import { ERROR_MS, bootDoorWords, markCss, markFromDrag, mirrorCadence } from "./deviceMirrorModel.mjs";
import type { MirrorBox } from "./deviceMirrorModel.mjs";
import { deviceById, deviceTitle, isUp } from "./devicesModel.mjs";
import type { Mark } from "./captureModel.mjs";
import { useDeviceCapture } from "./useDeviceCapture";
import { t } from "../../i18n/l10n.mjs";

export function DeviceDoc({ wid, pid, deviceId }: { wid: string; pid: string | null; deviceId: string }) {
  const toast = useToast();
  const { devices, read } = useDevices();
  const device = deviceById(devices, deviceId);
  const { sessions } = useTerminals();
  const run = mobileDevelopmentRunSession(sessions, wid, deviceId);
  const up = device ? isUp(device) : false;
  const label = device?.name ?? deviceId;
  const capture = useDeviceCapture({ wid, deviceId, label, up });
  const [busy, setBusy] = useState(false);
  useEngineEvents((e) => {
    if (e.payload.type === "mobile_development_changed") void refreshDevices();
  });

  const say = (e: unknown) => toast.error(e instanceof Error ? e.message : String(e));
  const write = (data: string) => {
    if (!run) return;
    void writeToTerminalTab(run.key, data).catch(say);
  };
  const runApp = () => {
    if (!device) return;
    openTerminalIn({ scope: "workstream", id: wid, label: device.name, mobileDevelopment: { device: device.id } });
  };
  const boot = async () => {
    if (!device || busy) return;
    setBusy(true);
    try {
      const after = await api.mobileDevelopmentDeviceBoot(device.id);
      toast.ok(t("workbench-device-doc-up", { after: after.name }));
      await refreshDevices();
    } catch (e) {
      say(e);
    } finally {
      setBusy(false);
    }
  };
  const show = () => {
    if (!device) return;
    void api.mobileDevelopmentDeviceShow(device.id).catch(say);
  };
  const shot = (what: "copy" | "save" | "attach") => {
    if (!device || busy) return;
    setBusy(true);
    void takeDeviceShot(device.id)
      .then(async (taken) => {
        if (what === "copy") {
          await copyDeviceShot(taken);
          toast.ok(t("workbench-device-doc-copied-screen"));
        } else if (what === "save") {
          const path = await saveDeviceShot(taken);
          if (path) toast.ok(t("workbench-device-doc-saved", { path }));
        } else {
          attachDeviceShot(taken, device.id, device.name, `workstream:${wid}`);
          toast.ok(t("workbench-device-doc-attached-screen-write-message-agent-panel"));
        }
      })
      .catch(say)
      .finally(() => setBusy(false));
  };
  const camera: MenuItem[] = [
    { label: t("workbench-device-doc-copy-screen"), icon: ICON.copy, onSelect: () => shot("copy"), disabled: !up || busy },
    { label: t("workbench-device-doc-save-screen"), icon: ICON.save, onSelect: () => shot("save"), disabled: !up || busy },
    { label: t("workbench-device-doc-attach-screen-agent-panel"), icon: ICON.attach, onSelect: () => shot("attach"), disabled: !up || busy },
  ];
  const held = !run ? t("workbench-device-doc-run-device-first") : null;
  const traying = up && (capture.inspecting || capture.draft.captures.length > 0);

  return (
    <div data-device-doc tabIndex={-1} className="flex h-full min-h-0 flex-col outline-none">
      <div className="flex shrink-0 flex-wrap items-center gap-1 border-b border-border px-2 py-1 text-2xs" role="toolbar" aria-label={t("workbench-device-doc-device")}>
        <span className="mr-1 inline-flex min-w-0 items-center gap-1.5 text-text" title={device ? deviceTitle(device) : deviceId}>
          <ICON.simulator size={12} aria-hidden className="text-text-dim" />
          <span className="truncate">{label}</span>
          {device && <span className="text-text-dim">· {device.os ? `${device.os} · ` : ""}{up ? (run ? t("workbench-device-doc-running-app") : t("workbench-device-doc-state-up")) : device.state === "shutdown" ? t("workbench-devices-shut-down") : device.state}</span>}
        </span>
        <span className="flex-1" />
        <Tooltip label={run ? t("workbench-device-doc-app-running-device-terminal-strip") : up ? t("workbench-device-doc-run-app-device-terminal") : t("workbench-device-doc-boot-device-first")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" disabled={!up || !!run} onClick={runApp} aria-label={t("workbench-device-doc-run-app-device")}>
              <ICON.play size={12} aria-hidden />{t("workbench-device-doc-run")}</Button>
          </span>
        </Tooltip>
        <Tooltip label={held ?? t("workbench-device-doc-hot-reload-r-run-terminal")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" disabled={!run} onClick={() => write("r")} aria-label={t("workbench-device-doc-hot-reload")}>
              <ICON.hotReload size={12} aria-hidden />{t("workbench-device-doc-reload")}</Button>
          </span>
        </Tooltip>
        <Tooltip label={held ?? t("workbench-device-doc-hot-restart-r-run-terminal")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" disabled={!run} onClick={() => write("R")} aria-label={t("workbench-device-doc-hot-restart")}>
              <ICON.hotRestart size={12} aria-hidden />{t("workbench-device-doc-restart")}</Button>
          </span>
        </Tooltip>
        <Tooltip label={held ?? t("workbench-device-doc-stop-app-q-run-terminal")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" disabled={!run} onClick={() => write("q")} aria-label={t("workbench-device-doc-stop-app")}>
              <ICON.terminate size={12} aria-hidden />{t("workbench-device-doc-stop")}</Button>
          </span>
        </Tooltip>
        {device?.platform === "ios" && (
          <Tooltip label={t("workbench-device-doc-bring-simulator-window-forward-touch-app")}>
            <span className="inline-flex">
              <Button size="sm" variant="ghost" disabled={!up} onClick={show} aria-label={t("workbench-device-doc-show-simulator-window")}>
                <ICON.expand size={12} aria-hidden />{t("workbench-device-doc-window")}</Button>
            </span>
          </Tooltip>
        )}
        <Menu
          label={t("workbench-device-doc-capture-screen")}
          items={camera}
          trigger={
            <span className={cn("anim inline-flex h-6 items-center gap-1 rounded-control px-1.5 text-text-dim hover:bg-surface-2 hover:text-text", !up && "opacity-60")}>
              <ICON.camera size={12} aria-hidden />
            </span>
          }
        />
        <Tooltip label={up ? (capture.inspecting ? t("workbench-device-doc-stop-marking-screen") : t("workbench-device-doc-mark-spot-screen-agent")) : t("workbench-device-doc-boot-device-mark-screen")}>
          <span className="inline-flex">
            <Button size="sm" variant={capture.inspecting ? "primary" : "ghost"} disabled={!up} onClick={() => capture.setInspecting(!capture.inspecting)} aria-label={t("workbench-device-doc-mark-screen-agent")} aria-pressed={capture.inspecting}>
              <ICON.annotate size={12} aria-hidden />
              {capture.draft.captures.length > 0 && <span className="tnum">{capture.draft.captures.length}</span>}
            </Button>
          </span>
        </Tooltip>
      </div>
      <div className="relative min-h-0 flex-1 bg-surface-2">
        {device && up && <DeviceMirror deviceId={deviceId} label={label} inspecting={capture.inspecting} taking={capture.taking} marks={capture.draft.captures.map((c) => c.mark)} onMark={capture.begin} />}
        {device && !up && (
          <div className="flex h-full items-center justify-center p-6">
            <BootDoor device={device} busy={busy} onBoot={() => void boot()} />
          </div>
        )}
        {!device && (
          <div className="flex h-full items-center justify-center p-6">
            {read ? (
              <EmptyState icon={ICON.device} title={t("workbench-device-doc-device-not-here")} hint={t("workbench-device-doc-unplugged-platform-off-settings-capabilities-mobile-development")} action={<Button size="sm" onClick={() => void refreshDevices()}>{t("workbench-device-doc-look-again")}</Button>} />
            ) : (
              <Pending what={t("workbench-device-doc-devices")} rows={2} />
            )}
          </div>
        )}
      </div>
      {capture.pending && <CaptureNoteBox mark={capture.pending.mark} label={label} onAdd={capture.add} onClose={capture.cancel} />}
      {traying && <CaptureTray wid={wid} pid={pid} device={deviceId} label={label} draft={capture.draft} onDraft={capture.setDraft} onDone={capture.done} />}
    </div>
  );
}

/** The door drawn where the mirror would be while the device is not up. */
function BootDoor({ device, busy, onBoot }: { device: { name: string; kind: "simulator" | "emulator" | "physical"; state: string }; busy: boolean; onBoot: () => void }) {
  const words = bootDoorWords(device);
  return (
    <EmptyState
      icon={ICON.simulator}
      title={words.title}
      hint={words.hint}
      action={
        words.verb ? (
          <Button size="sm" variant="primary" disabled={busy} onClick={onBoot}>
            <ICON.boot size={12} aria-hidden />
            {words.verb}
          </Button>
        ) : null
      }
    />
  );
}

/**
 * The screen, read again a few times a second while the device is up and
 * the window is awake and in front (`mirrorCadence`); the last frame stays
 * while it pauses. With the wand on, a drag over the picture is a rectangle
 * in device pixels and a click the whole screen; the marks made so far are
 * numbered badges over it.
 */
function DeviceMirror({ deviceId, label, inspecting, taking, marks, onMark }: { deviceId: string; label: string; inspecting: boolean; taking: boolean; marks: readonly (Mark | null)[]; onMark: (mark: Mark | null) => void }) {
  const awake = useVisible();
  const [focused, setFocused] = useState(() => (typeof document === "undefined" ? true : document.hasFocus()));
  const [errors, setErrors] = useState(0);
  const [seq, setSeq] = useState(1);
  const [shown, setShown] = useState<string | null>(null);
  const [box, setBox] = useState<MirrorBox | null>(null);
  const [drag, setDrag] = useState<{ start: { x: number; y: number }; end: { x: number; y: number } } | null>(null);
  const timer = useRef<number | null>(null);
  const img = useRef<HTMLImageElement | null>(null);
  const cadence = mirrorCadence({ up: true, awake, focused, errors });
  const src = cadence.poll ? api.mobileDevelopmentFrameUrl(deviceId, seq) : shown;

  useEffect(() => {
    const on = () => setFocused(true);
    const off = () => setFocused(false);
    window.addEventListener("focus", on);
    window.addEventListener("blur", off);
    return () => {
      window.removeEventListener("focus", on);
      window.removeEventListener("blur", off);
    };
  }, []);
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );
  // The picture's box, kept as the pane resizes: what a drag is mapped through.
  useEffect(() => {
    const el = img.current;
    if (!el) return;
    const measure = () => setBox({ clientWidth: el.clientWidth, clientHeight: el.clientHeight, naturalWidth: el.naturalWidth, naturalHeight: el.naturalHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [shown]);

  const schedule = (ms: number) => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setSeq((s) => s + 1), ms);
  };
  const loaded = () => {
    setShown(src);
    setErrors(0);
    const el = img.current;
    if (el) setBox({ clientWidth: el.clientWidth, clientHeight: el.clientHeight, naturalWidth: el.naturalWidth, naturalHeight: el.naturalHeight });
    if (cadence.poll) schedule(cadence.every);
  };
  const failed = () => {
    setErrors((n) => n + 1);
    if (cadence.poll) schedule(ERROR_MS);
  };

  const at = (e: ReactPointerEvent<HTMLElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const down = (e: ReactPointerEvent<HTMLElement>) => {
    if (!inspecting || taking) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = at(e);
    setDrag({ start: p, end: p });
  };
  const move = (e: ReactPointerEvent<HTMLElement>) => {
    if (!drag) return;
    setDrag({ start: drag.start, end: at(e) });
  };
  const upAt = (e: ReactPointerEvent<HTMLElement>) => {
    if (!drag) return;
    const end = at(e);
    setDrag(null);
    if (!box) return;
    // A rectangle, or — a click — the whole screen.
    onMark(markFromDrag(drag.start, end, box));
  };
  const rect = drag && box ? markCss(markFromDrag(drag.start, drag.end, box) ?? { x: 0, y: 0, width: 0, height: 0 }, box) : null;

  return (
    <div className="relative h-full w-full">
      {src ? (
        <img ref={img} src={src} alt={t("workbench-device-doc-screen", { label })} draggable={false} onLoad={loaded} onError={failed} className="h-full w-full select-none object-contain" />
      ) : (
        <div className="flex h-full items-center justify-center">
          <Pending what={t("workbench-device-doc-screen-2")} rows={2} />
        </div>
      )}
      {marks.map((m, i) => {
        const css = m && box ? markCss(m, box) : null;
        return css ? (
          <span key={i} className="pointer-events-none absolute rounded border-2 border-accent" style={{ left: css.left, top: css.top, width: css.width, height: css.height }}>
            <span className="absolute -left-2 -top-2 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-accent px-1 text-2xs font-semibold text-accent-contrast">{i + 1}</span>
          </span>
        ) : m === null ? (
          <span key={i} className="pointer-events-none absolute left-1 top-1 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-accent px-1 text-2xs font-semibold text-accent-contrast">{i + 1}</span>
        ) : null;
      })}
      {inspecting && (
        <div
          role="presentation"
          className={cn("absolute inset-0 touch-none", taking ? "cursor-progress" : "cursor-crosshair")}
          onPointerDown={down}
          onPointerMove={move}
          onPointerUp={upAt}
          onPointerCancel={() => setDrag(null)}
        >
          {rect && rect.width > 0 && <span className="pointer-events-none absolute rounded border-2 border-dashed border-accent bg-accent/10" style={{ left: rect.left, top: rect.top, width: rect.width, height: rect.height }} />}
          <span className="pointer-events-none absolute bottom-2 left-2 rounded bg-surface/90 px-1.5 py-0.5 text-2xs text-text-dim">{taking ? t("workbench-device-doc-capturing-screen") : t("workbench-device-doc-drag-rectangle-click-whole-screen")}</span>
        </div>
      )}
      {cadence.stopped && (
        <div className="absolute inset-x-0 bottom-0 flex items-center justify-between gap-2 bg-surface/90 px-3 py-1.5 text-2xs text-warn">
          <span>{t("workbench-device-doc-device-stopped-answering")}</span>
          <Button size="sm" variant="ghost" onClick={() => setErrors(0)}>{t("workbench-device-doc-try-again")}</Button>
        </div>
      )}
    </div>
  );
}
