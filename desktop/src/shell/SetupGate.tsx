/**
 * The setup gate (16 — The setup gate): what the platform needs before it
 * can work — git, a coding harness and its three core agents: the
 * Decision-Making Agent, the General Agent and the Workflow Agent, in the
 * order the node checks them — read from `GET /readiness` and put in front of
 * everything until every check is ready. Each missing check says what is
 * wrong, shows this platform's official install lines to copy and the page
 * they come from, opens the Settings tab or the Agents page where it is
 * fixed by hand, and offers the one-click fixes the node vouches for. The
 * gate installs nothing and runs no command.
 *
 * Three modes (`setupModel.gateMode`): nothing while ready or unknown; a
 * banner over Settings and Agents, which stay usable so the fixes can be
 * made; the modal everywhere else — an `alertdialog` nobody can dismiss
 * (no close, Escape held), since there is nothing to do in the app without
 * what it asks for. It reads again on `settings_changed`, after a fix, on
 * *Check again*, when the node comes back, and every twenty seconds while
 * something is missing or the checks could not be read (`useReadiness.ts`).
 */

import * as A from "@radix-ui/react-alert-dialog";
import { useState } from "react";
import { api, openExternal } from "../api";
import { navigate } from "../router";
import type { InstallHint, Readiness, ReadinessCheck, ReadinessFix } from "../types";
import { Button, Card, Chip, ICON, copyText, useToast } from "../ui";
import { BODY, HEADER, OVERLAY, PANEL } from "../ui/dialogLayout.mjs";
import { useSurface } from "../ui/openSurfaces";
import { bannerWords, checkWords, commandsFor, doorLabel, doorTarget, fixBody, gateMode, offeredFixes, platformOf, progressWords } from "./setupModel.mjs";
import { useReadiness } from "./useReadiness";
import { t } from "../i18n/l10n.mjs";
import { rich } from "../i18n/rich";

/** One official install line, with a Copy button. */
function CommandLine({ command }: { command: string }) {
  const toast = useToast();
  return (
    <div className="flex items-center gap-2 rounded-control border border-border bg-surface-2 px-2 py-1">
      <code className="min-w-0 flex-1 truncate font-mono text-2xs text-text" title={command}>
        {command}
      </code>
      <Button size="sm" variant="ghost" onClick={() => void copyText(command).then((ok) => (ok ? toast.ok(t("shell-setup-gate-copied")) : toast.error(t("shell-setup-gate-clipboard-refused"))))}>
        <ICON.copy size={11} aria-hidden />{t("shell-setup-gate-copy")}</Button>
    </div>
  );
}

/** How to install one thing: this platform's lines, the check, the sign-in, the official page. */
function Hint({ hint, platform }: { hint: InstallHint; platform: string }) {
  const toast = useToast();
  return (
    <div className="flex flex-col gap-1.5">
      {commandsFor(hint, platform).map((c) => (
        <CommandLine key={c} command={c} />
      ))}
      {hint.verify && <p className="text-2xs text-text-dim">{rich("shell-setup-gate-then-check-landed", { verify: <code className="font-mono">{hint.verify}</code> })}</p>}
      {hint.sign_in && <p className="text-2xs text-text-dim">{t("shell-setup-gate-sign", { sign_in: hint.sign_in })}</p>}
      <div>
        <Button size="sm" variant="ghost" onClick={() => void openExternal(hint.url).catch(() => toast.error(t("shell-setup-gate-browser-did-open")))}>
          <ICON.open size={11} aria-hidden />{t("shell-setup-gate-open-official-docs")}</Button>
      </div>
    </div>
  );
}

function CheckCard({ check, platform, onFixed }: { check: ReadinessCheck; platform: string; onFixed: () => void }) {
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const words = checkWords(check);
  const Glyph = ICON[words.icon];
  const door = doorTarget(check.door);
  const fixes = offeredFixes(check);
  const fix = async (f: ReadinessFix) => {
    const body = fixBody(f);
    if (!body) return;
    setBusy(f.label);
    try {
      if (body.kind === "settings") await api.setSettings("workspace", body.set);
      else await api.patchAgent(body.id, body.body);
      toast.ok(t("shell-setup-gate-done", { f: f.label }));
      onFixed();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : t("shell-setup-gate-could", { f: f.label.toLowerCase() }));
    } finally {
      setBusy(null);
    }
  };
  return (
    <Card className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Glyph size={14} aria-hidden className="shrink-0 text-text-dim" />
        <span className="text-xs font-semibold text-text">{check.title}</span>
        <Chip tone={words.tone}>{words.word}</Chip>
      </div>
      <p className="text-2xs text-text-dim">{check.detail}</p>
      {check.state !== "ready" && (
        <>
          {check.hint && <Hint hint={check.hint} platform={platform} />}
          {fixes.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {fixes.map((f) => (
                <Button key={f.label} size="sm" variant="primary" disabled={busy !== null} onClick={() => void fix(f)}>
                  {busy === f.label ? "…" : f.label}
                </Button>
              ))}
            </div>
          )}
          {door && (
            <div>
              <Button size="sm" variant="default" onClick={() => navigate(door.route, door.search)}>
                {doorLabel(check.door)}
              </Button>
            </div>
          )}
        </>
      )}
    </Card>
  );
}

/** The docked line over Settings and Agents while something is missing. */
function Banner({ readiness, checking, onCheck }: { readiness: Readiness; checking: boolean; onCheck: () => void }) {
  return (
    <div role="status" className="flex items-center gap-2 border-b border-warn/40 bg-warn-soft/30 px-3 py-1.5 text-2xs text-text">
      <ICON.warn size={12} aria-hidden className="shrink-0 text-warn" />
      <span className="font-semibold">{t("shell-setup-gate-setup-incomplete")}</span>
      <span className="text-text-dim">{bannerWords(readiness)}</span>
      <span className="ml-auto flex items-center gap-1">
        <Button size="sm" variant="ghost" disabled={checking} onClick={onCheck}>
          {checking ? t("shell-node-overlay-checking") : t("shell-setup-gate-check-again")}
        </Button>
        <Button size="sm" variant="default" onClick={() => navigate({ name: "pulse" })}>{t("shell-setup-gate-back-setup")}</Button>
      </span>
    </div>
  );
}

export function SetupGate({ screen }: { screen: string }) {
  const { readiness, error, checking, check } = useReadiness();
  const mode = gateMode({ ready: readiness?.ready, screen });
  useSurface(mode === "modal");
  const platform = platformOf(typeof navigator === "undefined" ? null : navigator);
  if (mode === "none" || !readiness) return null;
  if (mode === "banner") return <Banner readiness={readiness} checking={checking} onCheck={check} />;
  return (
    <A.Root open>
      <A.Portal>
        <A.Overlay className={OVERLAY} />
        <A.Content data-pane className={`${PANEL} max-w-2xl`} aria-modal onEscapeKeyDown={(e) => e.preventDefault()}>
          <header className={`${HEADER} flex items-center gap-2`}>
            <A.Title className="text-base font-semibold">{t("shell-setup-gate-before-start")}</A.Title>
            <Chip tone="warn">{progressWords(readiness)}</Chip>
            <Button size="sm" variant="ghost" className="ml-auto" disabled={checking} onClick={check}>
              {checking ? t("shell-node-overlay-checking") : t("shell-setup-gate-check-again")}
            </Button>
          </header>
          <A.Description asChild>
            <p className="px-4 pt-3 text-2xs text-text-dim">{t("shell-setup-gate-platform-needs-git-one-coding-harness")}</p>
          </A.Description>
          <div className={`${BODY} flex flex-col gap-2`}>
            {error && <p className="text-2xs text-danger">{error}</p>}
            {readiness.checks.map((c) => (
              <CheckCard key={c.id} check={c} platform={platform} onFixed={check} />
            ))}
          </div>
        </A.Content>
      </A.Portal>
    </A.Root>
  );
}
