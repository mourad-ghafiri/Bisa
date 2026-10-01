/**
 * Who may sign which gate.
 *
 * A listed policy accepts pubkeys *and* `team:<id>` references, which are
 * expanded to that team's humans whenever a decision is checked — so adding
 * someone to a team grants them approval without touching this screen.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import type { Gate, GatePolicy, Governance, TeamDef } from "../../types";
import {
  Button,
  Card,
  ErrorNote,
  Field,
  GATE_ICON,
  ICON,
  Pending,
  ReadLine,
  Select,
  TextInput,
  useToast,
} from "../../ui";
import { pendingRows, phase, readWords } from "../_settings/loadModel.mjs";
import { matrixRows, roleLabel } from "../_settings/peopleModel.mjs";
import { GATES, governanceBody } from "./governanceModel.mjs";
import { attempt, useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";

const EXPLAIN: Record<Gate, string> = {
  approval:
    tr("work-governance-panel-adopting-proposed-workflow-signing-approval-step"),
  escalation: tr("work-governance-panel-answering-human-step-question-agent-raised"),
  publish: tr("work-governance-panel-pushing-branch-opening-pull-request-work"),
};

type PolicyKind = "owner" | "admins" | "members" | "listed";

function policyKind(p: GatePolicy | undefined): PolicyKind {
  return p?.policy ?? "owner";
}

/** The matrix, from the node: one row per permission, a cell per role. */
function RoleMatrix() {
  const read = useAsync((s) => api.roles(s), []);
  if (phase(read) === "failed") return <ErrorNote error={read.error ?? tr("work-governance-panel-roles-read-refused")} retry={read.reload} />;
  if (phase(read) === "pending") return <Pending what={tr("work-governance-panel-roles-2")} rows={pendingRows(tr("work-governance-panel-roles-2"))} />;
  const { roles, rows } = matrixRows(read.data);
  return (
    <Card>
      <p className="mb-2 text-2xs text-text-dim">{tr("work-governance-panel-what-each-role-may-do")}</p>
      <div className="overflow-x-auto">
        <table className="w-full text-2xs">
          <thead>
            <tr className="text-left text-text-dim">
              <th className="py-1 pr-2 font-medium">{tr("work-governance-panel-permission")}</th>
              {roles.map((r) => (
                <th key={r} className="px-2 py-1 text-center font-medium">
                  {roleLabel(r)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.permission} className="border-t border-border">
                <td className="py-1 pr-2" title={row.words}>
                  {row.label}
                </td>
                {row.holds.map((held, i) => (
                  <td key={roles[i]} className="px-2 py-1 text-center">
                    {held ? <ICON.ok size={12} aria-label={tr("work-governance-panel-yes")} className="inline text-ok" /> : <span className="text-text-dim">—</span>}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

/** A gate's glyph, from the one map that owns the three of them. */
function GateIcon({ kind }: { kind: Gate }) {
  const Icon = GATE_ICON[kind];
  return <Icon size={13} aria-hidden className="shrink-0 text-text-dim" />;
}

export function GovernancePanel() {
  const toast = useToast();
  const governance = useAsync((s) => api.governance(s), []);
  const { data, error, reload } = governance;
  // Teams are only needed to offer `team:<id>` chips on a listed gate — their
  // own read, so the chips say *reading the teams…* rather than nothing.
  const teamRead = useAsync((s) => api.teams(s), []);
  const teams: TeamDef[] = teamRead.data?.teams ?? [];
  const [draft, setDraft] = useState<Governance | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (data) setDraft(data.governance);
  }, [data]);

  // `phase` decides: a refused first read is a note with *Retry*, never a
  // skeleton that pulses forever (ide/13 §Every panel reads the same way).
  const state = phase(governance);
  if (state === "failed") return <ErrorNote error={error ?? tr("work-governance-panel-governance-read-refused")} retry={reload} />;
  if (state === "pending" || !draft) return <Pending what={tr("work-governance-panel-governance")} rows={pendingRows(tr("work-governance-panel-governance"))} />;

  const set = (gate: Gate, policy: GatePolicy) =>
    setDraft((d) => (d ? { ...d, [gate]: policy } : d));

  const save = async () => {
    setBusy(true);
    const ok = await attempt(() => api.setGovernance(governanceBody(draft)), toast.error);
    setBusy(false);
    if (ok) {
      toast.ok(tr("work-governance-panel-governance-updated"));
      reload();
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="text-2xs text-text-dim">{tr("work-governance-panel-only-you-default")}</p>

      {GATES.map((gate) => {
        const p = draft[gate];
        const kind = policyKind(p);
        const entries = p?.policy === "listed" ? p.pubkeys : [];
        return (
          <Card key={gate}>
            <div className="mb-2 flex items-center gap-2">
              {/* The same glyph this gate wears everywhere else — in the
                  inbox, on a gate bar, on a project's publishing policy. */}
              <GateIcon kind={gate} />
              <h3 className="text-xs font-medium capitalize">{gate}</h3>
              <span className="min-w-0 text-2xs text-text-dim">{EXPLAIN[gate]}</span>
            </div>
            <div className="grid gap-2 md:grid-cols-2">
              <Field label={tr("work-governance-panel-who-decides")}>
                <Select
                  value={kind}
                  onChange={(e) => {
                    const v = e.target.value as PolicyKind;
                    set(
                      gate,
                      v === "listed" ? { policy: "listed", pubkeys: entries } : { policy: v },
                    );
                  }}
                >
                  <option value="owner">{tr("work-agent-editor-only-me")}</option>
                  <option value="admins">{tr("work-governance-panel-me-admins")}</option>
                  <option value="members">{tr("work-governance-panel-me-admins-members")}</option>
                  <option value="listed">{tr("work-governance-panel-specific-list")}</option>
                </Select>
              </Field>
              {kind === "listed" && (
                <Field
                  label={tr("work-governance-panel-entries")}
                  hint={tr("work-governance-panel-comma-separated-pubkeys-team-id-references")}
                >
                  <TextInput
                    value={entries.join(", ")}
                    placeholder={teams[0] ? `team:${teams[0].id}` : tr("work-governance-panel-team-id-pubkey")} // for the machine: a team reference
                    onChange={(e) =>
                      set(gate, {
                        policy: "listed",
                        pubkeys: e.target.value
                          .split(",")
                          .map((x) => x.trim())
                          .filter(Boolean),
                      })
                    }
                  />
                </Field>
              )}
            </div>
            {kind === "listed" && phase(teamRead) === "pending" && <Pending what={tr("work-governance-panel-teams")} rows={0} className="mt-1.5" />}
            {kind === "listed" && phase(teamRead) === "failed" && <ReadLine words={readWords({ what: tr("work-governance-panel-teams"), error: teamRead.error, data: null })} onReload={teamRead.reload} />}
            {kind === "listed" && teams.length > 0 && (
              <p className="mt-1.5 flex flex-wrap items-center gap-1 text-2xs text-text-dim">
                {tr("work-governance-panel-teams-list")}
                {teams.map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    className="anim inline-flex items-center gap-1 rounded-full border border-border px-1.5 hover:border-accent/50 hover:text-text"
                    onClick={() =>
                      set(gate, {
                        policy: "listed",
                        pubkeys: entries.includes(`team:${t.id}`)
                          ? entries
                          : [...entries, `team:${t.id}`],
                      })
                    }
                  >
                    <ICON.add size={10} aria-hidden />
                    <ICON.team size={10} aria-hidden />
                    {t.name}
                  </button>
                ))}
              </p>
            )}
          </Card>
        );
      })}

      <div>
        <Button variant="primary" onClick={() => void save()} disabled={busy}>
          {busy ? tr("work-agent-editor-saving") : tr("work-governance-panel-save-governance")}
        </Button>
      </div>

      <h3 className="mt-2 text-xs font-medium">{tr("work-governance-panel-roles")}</h3>
      <RoleMatrix />
    </div>
  );
}
