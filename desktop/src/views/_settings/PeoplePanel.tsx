/**
 * Settings › People (14-collaboration): the people this workspace hosts on
 * other nodes, each with a role; the invitations that bring them, as a
 * link, a code and a QR to scan; the claims waiting on you when the
 * workspace admits by hand; joining another workspace from a code; and the
 * workspaces you are a guest of.
 *
 * Membership is the host's local table: the node keeps who is in and at
 * what role, and the pump tells each person what their role reaches. The
 * matrix is the node's too (`GET /workspace/roles`) — drawn here, never
 * restated. Every word around it is `peopleModel.mjs`'s.
 */

import { useEffect, useMemo, useRef, useState, type Ref } from "react";
import QRCode from "qrcode";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { useSearchValue } from "../../router";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { hostName, stateWords } from "../../shell/hostedModel.mjs";
import type { Hosted, Invite, MemberRole, PersonRow } from "../../types";
import {
  Avatar,
  Button,
  Card,
  Chip,
  ConfirmDialog,
  CopyText,
  Dialog,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Labelled,
  Pending,
  Section,
  Select,
  TextInput,
  useToast,
} from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, phase } from "./loadModel.mjs";
import { parseInviteCode } from "./inviteCodeModel.mjs";
import { wireMoved } from "./relayHealthModel.mjs";
import { SettingsLink } from "./SettingsTabLink";
import {
  HOSTED_ROLES,
  REMOVE_WORDS,
  ROLE_HINT,
  expiryWords,
  inviteOffer,
  inviteState,
  orderInvites,
  personName,
  personWords,
  roleChangeWords,
  roleLabel,
  admitKey,
} from "./peopleModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

function RoleSelect({ value, onChange, disabled }: { value: MemberRole; onChange: (r: MemberRole) => void; disabled?: boolean }) {
  return (
    <Select value={value} disabled={disabled} onChange={(e) => onChange(e.target.value as MemberRole)}>
      {HOSTED_ROLES.map((r) => (
        <option key={r} value={r}>
          {roleLabel(r)}
        </option>
      ))}
    </Select>
  );
}

function Person({ p, onRole, onRemove }: { p: PersonRow; onRole: (r: MemberRole) => void; onRemove: () => void }) {
  const words = personWords(p);
  return (
    <li className="flex min-h-row-lg items-center gap-2 px-3 py-1.5">
      <Avatar id={p.pubkey} name={personName(p)} photo={p.photo} size={20} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs">{personName(p)}</span>
        {words && <span className="block truncate text-2xs text-text-dim">{words}</span>}
      </span>
      <CopyText value={p.pubkey} label={`${p.pubkey.slice(0, 10)}…`} />
      <span className="w-28">
        <RoleSelect value={p.role} onChange={onRole} />
      </span>
      <Button size="sm" variant="danger" onClick={onRemove}>{tr("settings-git-profiles-panel-remove-2")}</Button>
    </li>
  );
}

/** The invitation just made: the link, the code, the QR — and nothing else ever shows the secret again. */
function InviteMade({ link, code, invite, onClose }: { link: string; code: string; invite: Invite; onClose: () => void }) {
  const [qr, setQr] = useState<string | null>(null);
  useEffect(() => {
    let gone = false;
    void QRCode.toDataURL(link, { margin: 1, width: 192 })
      .then((url) => {
        if (!gone) setQr(url);
      })
      .catch(() => setQr(null));
    return () => {
      gone = true;
    };
  }, [link]);
  return (
    <Dialog
      open
      onClose={onClose}
      title={tr("settings-people-panel-invitation-made")}
      description={tr("settings-people-panel-share-one-these-out-band-secret", { invite: inviteOffer(invite) })}
      footer={
        <Button variant="primary" onClick={onClose}>{tr("settings-people-panel-done")}</Button>
      }
    >
      <div className="flex flex-col gap-3">
        {/* A copy button is not a field: inside a <label>, a click on the caption would copy. */}
        <Labelled label={tr("settings-people-panel-link")} hint={tr("settings-people-panel-opens-app-their-machine-panel-code")}>
          <CopyText value={link} label={link} />
        </Labelled>
        <Labelled label={tr("settings-people-panel-code")} hint={tr("settings-people-panel-terminal-bisa-workspace-join-code")}>
          <CopyText value={code} label={code} />
        </Labelled>
        {qr && (
          <div className="flex items-center gap-3">
            <img src={qr} alt={tr("settings-people-panel-invitation-link-qr-code")} width={192} height={192} className="rounded-control border border-border bg-white" />
            <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{tr("settings-people-panel-scan-phone-person-will-use-link")}</p>
          </div>
        )}
      </div>
    </Dialog>
  );
}

function InviteDialog({ open, onClose, onMade }: { open: boolean; onClose: () => void; onMade: (r: { invite: Invite; link: string; code: string }) => void }) {
  const toast = useToast();
  const ws = useWorkspace();
  const [role, setRole] = useState<MemberRole>("guest");
  const [channels, setChannels] = useState<string[]>([]);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const ttl = useAsync(async (s) => (await api.settingsResolved(null, s)).settings.find((r) => r.key === "collab.invite_ttl_hours")?.value, []);
  const standing = ws.channels.map((c) => c.channel).filter((c) => c.id !== "general");
  useEffect(() => {
    if (open) {
      setRole("guest");
      setChannels([]);
      setLabel("");
    }
  }, [open]);
  const make = async () => {
    setBusy(true);
    await attempt(
      () => api.createInvite({ role, channels: role === "guest" ? channels : [], ...(label.trim() ? { label: label.trim() } : {}) }),
      toast.error,
      (r) => {
        onClose();
        onMade(r);
      },
    );
    setBusy(false);
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={tr("settings-people-panel-invite-someone")}
      description={tr("settings-people-panel-single-use-code-good-once-admits")}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{tr("settings-connectors-panel-cancel")}</Button>
          <Button variant="primary" disabled={busy} onClick={() => void make()}>
            {busy ? tr("settings-people-panel-making") : tr("settings-people-panel-make-invitation")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={tr("settings-people-panel-role")} hint={ROLE_HINT[role]}>
          <RoleSelect value={role} onChange={setRole} />
        </Field>
        {role === "guest" && (
          <Labelled label={tr("settings-people-panel-channels")} hint={standing.length ? tr("settings-people-panel-guest-reaches-these-nothing-else-add") : tr("settings-people-panel-channel-besides-general-yet-make-one")}>
            <div className="flex flex-wrap gap-1">
              {standing.map((c) => {
                const on = channels.includes(c.id);
                return (
                  <button
                    key={c.id}
                    type="button"
                    aria-pressed={on}
                    onClick={() => setChannels((cs) => (on ? cs.filter((x) => x !== c.id) : [...cs, c.id]))}
                    className={`anim inline-flex h-6 items-center gap-1 rounded-full border px-2 text-2xs ${on ? "border-text/35 bg-selected text-text" : "border-border text-text-dim hover:bg-surface-2 hover:text-text"}`}
                  >
                    <ICON.channel size={10} aria-hidden />
                    {c.name}
                  </button>
                );
              })}
            </div>
          </Labelled>
        )}
        <Field label={tr("settings-people-panel-what-call-them")} hint={tr("settings-people-panel-until-they-say-optional")}>
          <TextInput value={label} onChange={(e) => setLabel(e.target.value)} />
        </Field>
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-people-panel-invite-expiry-blurb", { code: (inner) => <span className="font-mono">{inner}</span> }, { expires: expiryWords(Number(ttl.data ?? 24)) })}</p>
      </div>
    </Dialog>
  );
}

function JoinCard({ initial, onJoined, codeRef }: { initial: string | null; onJoined: () => void; codeRef?: Ref<HTMLInputElement> }) {
  const toast = useToast();
  const [code, setCode] = useState(initial ?? "");
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [answer, setAnswer] = useState<Hosted | null>(null);
  useEffect(() => {
    if (initial) setCode(initial);
  }, [initial]);
  const parsed = parseInviteCode(code);
  const join = async () => {
    setBusy(true);
    await attempt(
      () => api.joinHost({ code: code.trim(), ...(label.trim() ? { label: label.trim() } : {}) }),
      toast.error,
      (r) => {
        setAnswer(r.host);
        setCode("");
        onJoined();
      },
    );
    setBusy(false);
  };
  return (
    <Card>
      <div className="flex flex-col gap-2">
        <Field label={tr("settings-people-panel-invitation-link-code")} hint={parsed.kind === "none" ? parsed.reason || tr("settings-people-panel-paste-what-host-sent") : tr("settings-people-panel-looks-like-invitation")}>
          <TextInput ref={codeRef} value={code} spellCheck={false} className="font-mono" placeholder={tr("settings-people-panel-bisa-join-nprofile1")} onChange={(e) => setCode(e.target.value)} />
        </Field>
        <div className="grid grid-cols-[1fr_auto] items-end gap-2">
          <Field label={tr("settings-people-panel-what-called-there")} hint={tr("settings-people-panel-optional")}>
            <TextInput value={label} onChange={(e) => setLabel(e.target.value)} />
          </Field>
          <Button variant="primary" disabled={parsed.kind === "none" || busy} onClick={() => void join()}>
            {busy ? tr("settings-people-panel-joining") : tr("settings-people-panel-join")}
          </Button>
        </div>
        {answer && (
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
            {answer.state.state === "member"
              ? tr("settings-people-panel-welcomed-channels-sidebar-under-hosted", { answer: hostName(answer), role: roleLabel(answer.role).toLowerCase() })
              : answer.state.state === "requested"
                ? tr("settings-people-panel-has-answered-yet-admits-hand-will", { answer: hostName(answer) })
                : stateWords(answer)}
          </p>
        )}
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{tr("settings-people-panel-join-yourself-only-agents-stay-node")}</p>
      </div>
    </Card>
  );
}

export function PeoplePanel() {
  const toast = useToast();
  const ws = useWorkspace();
  const [joinParam, setJoinParam] = useSearchValue("join");
  const people = useAsync(async (s) => (await api.people(s)).people, []);
  const invites = useAsync(async (s) => (await api.invites(s)).invites, []);
  const hosts = useAsync(async (s) => (await api.hosts(s)).hosts, []);
  const wire = useAsync((s) => api.sync(s), []);
  const [inviting, setInviting] = useState(false);
  const [made, setMade] = useState<{ invite: Invite; link: string; code: string } | null>(null);
  const [removing, setRemoving] = useState<PersonRow | null>(null);
  const [changing, setChanging] = useState<{ p: PersonRow; to: MemberRole } | null>(null);
  const [leaving, setLeaving] = useState<Hosted | null>(null);
  const [addKey, setAddKey] = useState("");
  const [addRole, setAddRole] = useState<MemberRole>("guest");
  const [addLabel, setAddLabel] = useState("");
  const [adding, setAdding] = useState(false);
  const joinCode = useRef<HTMLInputElement>(null);

  useEngineEvents((e) => {
    const t = e.payload.type;
    if (t === "people_changed" || t === "invite_changed") {
      people.reload();
      invites.reload();
    }
    if (t === "hosted_changed") hosts.reload();
    if (wireMoved(e.payload)) wire.reload();
  });
  const wireOff = wire.data ? !wire.data.enabled : false;

  const reloadAll = () => {
    people.reload();
    invites.reload();
    ws.refresh();
  };
  const { key, problem: keyProblem } = admitKey(addKey, people.data ?? [], ws.me);
  const ordered = useMemo(() => orderInvites(invites.data ?? []), [invites.data]);
  const waiting = ordered.filter((i) => i.state.state === "requested");

  const add = async () => {
    setAdding(true);
    await attempt(() => api.addPerson({ pubkey: key, role: addRole, ...(addLabel.trim() ? { label: addLabel.trim() } : {}) }), toast.error, () => {
      setAddKey("");
      setAddLabel("");
      toast.ok(tr("settings-people-panel-admitted"));
      reloadAll();
    });
    setAdding(false);
  };

  return (
    <div className="flex flex-col gap-6">
      {waiting.length > 0 && (
        <Section title={tr("settings-people-panel-waiting", { waiting: waiting.length })}>
          <ul className="flex flex-col gap-1.5">
            {waiting.map((i) => {
              const by = i.state.state === "requested" ? i.state.by : "";
              const label = i.state.state === "requested" ? i.state.label : null;
              return (
                // Somebody asks to join and the answer is yours: a summons, in the accent — not a warning.
                <li key={i.id} className="flex items-center gap-2 rounded-control border border-accent/40 bg-accent-soft px-2 py-1.5">
                  <Avatar id={by} name={label ?? by} size={20} />
                  <span className="min-w-0 flex-1 truncate text-xs">
                    {tr("settings-people-panel-asks-to-join-as", { who: label ?? `${by.slice(0, 8)}…`, role: inviteOffer(i).toLowerCase() })}
                  </span>
                  <Button size="sm" variant="primary" onClick={() => void attempt(() => api.admitInvite(i.id), toast.error, reloadAll)}>{tr("settings-people-panel-admit")}</Button>
                  <Button size="sm" onClick={() => void attempt(() => api.refuseInvite(i.id), toast.error, reloadAll)}>{tr("settings-people-panel-refuse")}</Button>
                </li>
              );
            })}
          </ul>
        </Section>
      )}

      <Section
        title={tr("settings-people-panel-people-2", { people: people.data?.length ?? 0 })}
        action={
          // The empty list carries this door itself; one Invite on screen.
          (people.data?.length ?? 0) > 0 && (
            <Button size="sm" variant="primary" onClick={() => setInviting(true)}>
              <ICON.add size={12} aria-hidden />{tr("settings-people-panel-invite-someone")}</Button>
          )
        }
      >
        {wireOff && (
          <p className="mb-2 flex items-center gap-1.5 rounded-control border border-warn/40 bg-warn-soft px-2 py-1 text-2xs text-warn">
            <ICON.sync size={11} aria-hidden className="shrink-0" />
            <span className="min-w-0 flex-1">{tr("settings-people-panel-relays-off-nobody-can-claim-invitation")}</span>
            <SettingsLink tab="sync" className="shrink-0">
              {tr("screens-settings-relays-sync")}
            </SettingsLink>
          </p>
        )}
        {phase(people) === "failed" && <ErrorNote error={people.error ?? tr("settings-people-panel-people-read-refused")} retry={people.reload} />}
        {phase(people) === "pending" && <Pending what={tr("settings-people-panel-people")} rows={pendingRows(tr("settings-people-panel-people"))} />}
        {phase(people) === "ready" && (people.data?.length ?? 0) === 0 && (
          <EmptyState
            icon={ICON.members}
            title={tr("settings-people-panel-nobody-else-yet")}
            hint={tr("settings-people-panel-invite-someone-they-join-from-their")}
            action={
              <Button variant="primary" onClick={() => setInviting(true)}>
                <ICON.add size={12} aria-hidden />{tr("settings-people-panel-invite-someone")}</Button>
            }
          />
        )}
        {phase(people) === "ready" && (people.data?.length ?? 0) > 0 && (
          <ul className="flex flex-col divide-y divide-hairline rounded-card border border-border bg-surface shadow-sm">
            {(people.data ?? []).map((p) => (
              <Person key={p.pubkey} p={p} onRole={(to) => setChanging({ p, to })} onRemove={() => setRemoving(p)} />
            ))}
          </ul>
        )}
      </Section>

      <Section title={tr("settings-people-panel-invitations")}>
        {phase(invites) === "failed" && invites.error ? (
          // A read that refused is said: *None made yet* would be a claim nobody checked.
          <ErrorNote error={invites.error} retry={invites.reload} />
        ) : phase(invites) === "pending" ? null : ordered.length === 0 ? (
          <EmptyState
            className="py-6"
            icon={ICON.link}
            title={tr("settings-people-panel-none-made-yet")}
            hint={tr("settings-people-panel-single-use-code-good-once-admits")}
            // While nobody is here, the People section above already offers the one door.
            action={phase(people) === "ready" && (people.data?.length ?? 0) === 0 ? null : <Button onClick={() => setInviting(true)}>{tr("settings-people-panel-invite-someone")}</Button>}
          />
        ) : (
          <ul className="flex flex-col divide-y divide-hairline rounded-card border border-border bg-surface shadow-sm">
            {ordered.map((i) => {
              const st = inviteState(i);
              return (
                <li key={i.id} className="flex min-h-row items-center gap-2 px-3 py-1 text-2xs">
                  <Chip tone={st.tone}>{st.word}</Chip>
                  <span className="min-w-0 flex-1 truncate">
                    {inviteOffer(i)}
                    {i.label ? ` · ${i.label}` : ""}
                  </span>
                  {i.state.state === "pending" && (
                    <Button size="sm" variant="ghost" onClick={() => void attempt(() => api.revokeInvite(i.id), toast.error, reloadAll)}>{tr("settings-people-panel-withdraw")}</Button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </Section>

      <Section title={tr("settings-people-panel-admit-key")}>
        <Card>
          <div className="flex flex-col gap-2">
            <Field label={tr("settings-people-panel-public-key-64-hex-characters")} error={keyProblem || undefined} hint={tr("settings-people-panel-somebody-who-already-knows-relays-invitation")}>
              <TextInput value={addKey} spellCheck={false} className="font-mono" /* for the machine */ placeholder="a1b2c3…" onChange={(e) => setAddKey(e.target.value)} />
            </Field>
            <div className="grid grid-cols-[1fr_10rem] gap-2">
              <Field label={tr("settings-connectors-panel-label")}>
                <TextInput value={addLabel} onChange={(e) => setAddLabel(e.target.value)} />
              </Field>
              <Field label={tr("settings-people-panel-role")}>
                <RoleSelect value={addRole} onChange={setAddRole} />
              </Field>
            </div>
            <div>
              <Button disabled={keyProblem !== null || adding} onClick={() => void add()}>
                {adding ? tr("settings-people-panel-admitting") : tr("settings-people-panel-admit")}
              </Button>
            </div>
          </div>
        </Card>
      </Section>

      <Section title={tr("settings-people-panel-join-workspace")}>
        <JoinCard
          codeRef={joinCode}
          initial={joinParam}
          onJoined={() => {
            setJoinParam(null);
            hosts.reload();
            ws.refresh();
          }}
        />
      </Section>

      <Section title={tr("settings-people-panel-workspaces", { hosts: hosts.data?.length ?? 0 })}>
        {phase(hosts) === "failed" && hosts.error ? (
          <ErrorNote error={hosts.error} retry={hosts.reload} />
        ) : phase(hosts) === "pending" ? null : (hosts.data?.length ?? 0) === 0 ? (
          <EmptyState
            className="py-6"
            icon={ICON.members}
            title={tr("settings-people-panel-none-yet-paste-invitation-above")}
            hint={tr("settings-people-panel-paste-invitation-to-join")}
            action={<Button onClick={() => joinCode.current?.focus()}>{tr("settings-people-panel-join-workspace")}</Button>}
          />
        ) : (
          <ul className="flex flex-col divide-y divide-hairline rounded-card border border-border bg-surface shadow-sm">
            {(hosts.data ?? []).map((h) => {
              const state = stateWords(h);
              return (
                <li key={h.host.pubkey} className="flex min-h-row-lg items-center gap-2 px-3 py-1.5">
                  <Avatar id={h.host.pubkey} name={hostName(h)} size={20} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-xs">{hostName(h)}</span>
                    <span className="block truncate text-2xs text-text-dim">{state ?? tr("settings-people-panel-words", { role: roleLabel(h.role).toLowerCase() })}</span>
                  </span>
                  <CopyText value={h.host.pubkey} label={`${h.host.pubkey.slice(0, 10)}…`} />
                  <Button size="sm" variant="danger" onClick={() => setLeaving(h)}>{tr("settings-people-panel-leave")}</Button>
                </li>
              );
            })}
          </ul>
        )}
      </Section>

      <InviteDialog open={inviting} onClose={() => setInviting(false)} onMade={(r) => { setMade(r); reloadAll(); }} />
      {made && <InviteMade link={made.link} code={made.code} invite={made.invite} onClose={() => setMade(null)} />}

      <ConfirmDialog
        open={changing !== null}
        onClose={() => setChanging(null)}
        title={changing ? tr("settings-people-panel-make", { p: personName(changing.p), to: roleLabel(changing.to).toLowerCase() }) : ""}
        confirmLabel={tr("settings-people-panel-change-role")}
        body={changing ? (roleChangeWords(changing.p.role, changing.to) ?? "") : ""}
        onConfirm={() => {
          if (!changing) return;
          const { p, to } = changing;
          void attempt(() => api.setPersonRole(p.pubkey, to), toast.error, () => {
            toast.ok(tr("settings-people-panel-now", { p: personName(p), to: roleLabel(to).toLowerCase() }));
            reloadAll();
          });
        }}
      />
      <ConfirmDialog
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={removing ? tr("settings-people-panel-remove", { removing: personName(removing) }) : ""}
        danger
        confirmLabel={tr("settings-git-profiles-panel-remove-2")}
        body={REMOVE_WORDS}
        onConfirm={() => {
          if (!removing) return;
          const target = removing.pubkey;
          void attempt(() => api.removePerson(target), toast.error, () => {
            toast.ok(tr("settings-people-panel-removed"));
            reloadAll();
          });
        }}
      />
      <ConfirmDialog
        open={leaving !== null}
        onClose={() => setLeaving(null)}
        title={leaving ? tr("settings-people-panel-leave-2", { leaving: hostName(leaving) }) : ""}
        danger
        confirmLabel={tr("settings-people-panel-leave")}
        body={tr("settings-people-panel-host-told-section-leaves-sidebar-what")}
        onConfirm={() => {
          if (!leaving) return;
          const target = leaving.host.pubkey;
          void attempt(() => api.leaveHost(target), toast.error, () => {
            hosts.reload();
            ws.refresh();
          });
        }}
      />
    </div>
  );
}
