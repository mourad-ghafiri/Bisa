/**
 * The MCP registry (06 § MCP servers): the servers an agent may be given,
 * named once and referenced by id — and, here, **checked**: every row wears
 * the health its last probe answered, *Check* dials it again, *Check all*
 * dials every row a few at a time, and the editor's *Test connection* dials
 * a draft before it is saved. The probe is the node's (`POST /mcp/probe`,
 * `POST /mcp/{id}/probe`, `bisa-mcp-probe`): it negotiates either protocol
 * era and speaks all three transports — a process over stdio, Streamable
 * HTTP, the older HTTP+SSE — and never calls a tool.
 *
 * This lives in Settings because an MCP server is infrastructure, not a
 * definition. A stdio transport is a command line plus an environment — it
 * names a binary and a set of variables on *one disk* — so the registry is
 * local to this machine and never syncs. What travels between nodes is the id
 * an agent carries.
 *
 * **Secrets are write-only.** The node answers every `env` and `headers`
 * value as `••••••` and keeps the stored one when the mask comes back
 * (`mcpFormModel.MASK`); the editor shows the mask and offers no reveal —
 * type over it to replace a value, leave it to keep it.
 *
 * Three refusals are the node's and are surfaced as its own words: the name
 * `bisa` is reserved, the id is a slug that cannot change afterwards, and a
 * server something still uses cannot be deleted (`GET /usage/mcp/{id}` is
 * asked before the button is offered). Nothing ships in this registry: a
 * bundled entry that shells out to a third-party package is a supply-chain
 * decision, and it belongs to whoever owns the machine.
 */

import { useEffect, useRef, useState } from "react";
import { ApiError, api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { McpServerView, McpProbeReport } from "../../types";
import {
  Button,
  Card,
  Chip,
  Dialog,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Labelled,
  MoreMenu,
  NO_TAG_FILTER,
  Section,
  SegmentedControl,
  Pending,
  SecretInput,
  Switch,
  TAG_VOCABULARY,
  TagChips,
  TagFilterBar,
  TagInput,
  TextArea,
  TextInput,
  Tooltip,
  failureText,
  passesTagFilter,
  useToast,
  type TagFilterState,
} from "../../ui";
import { DeleteDialog, useUsage } from "../_work/LibraryRefs";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows } from "./loadModel.mjs";
import {
  KINDS,
  MASK,
  RESERVED,
  blank,
  fieldForRefusal,
  probeStillAbout,
  fromDef,
  isMasked,
  kindWords,
  secretCount,
  toTransport,
  transportLine,
  validate,
} from "./mcpFormModel.mjs";
import type { McpDraft, McpFormErrors, McpKind, Pair } from "./mcpFormModel.mjs";
import { CHECK_ALL_AT_ONCE, capabilityWords, checkedWords, healthTone, healthWords, reportLines } from "./mcpHealthModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

// ---------------------------------------------------------------------------
// Pairs — the environment of a process, the headers of a remote
// ---------------------------------------------------------------------------

/**
 * Key/value rows for a process's environment or a remote's headers. Every
 * value is the kit's secret field: hidden as typed, the eye to show what was
 * typed here; a value the node masked reads as `••••••`, its eye out of work
 * — the value never came back — and typing over it replaces it.
 */
function PairEditor({
  value,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
  addLabel,
  keyLabel,
}: {
  value: Pair[];
  onChange: (next: Pair[]) => void;
  keyPlaceholder: string;
  valuePlaceholder: string;
  addLabel: string;
  keyLabel: string;
}) {
  const patch = (i: number, next: Pair) => onChange(value.map((p, j) => (j === i ? next : p)));
  return (
    <div className="flex flex-col gap-1">
      {value.map((p, i) => (
        <div key={i} className="flex items-center gap-1">
          <TextInput value={p.k} placeholder={keyPlaceholder} aria-label={keyLabel} onChange={(e) => patch(i, { ...p, k: e.target.value })} className="font-mono" />
          <SecretInput
            what={tr("ui-secret-input-what-value")}
            value={p.v}
            stored={isMasked(p.v)}
            placeholder={valuePlaceholder}
            aria-label={tr("settings-mcp-panel-value")}
            title={isMasked(p.v) ? tr("settings-mcp-panel-kept-machine-type-over-replace") : undefined}
            onChange={(v) => patch(i, { ...p, v })}
            className="w-full"
          />
          <Tooltip label={tr("settings-git-profiles-panel-remove-2")}>
            <Button size="icon" variant="ghost" aria-label={tr("settings-git-profiles-panel-remove-2")} onClick={() => onChange(value.filter((_, j) => j !== i))}>
              <ICON.close size={12} aria-hidden />
            </Button>
          </Tooltip>
        </div>
      ))}
      <div>
        <Button size="sm" variant="ghost" onClick={() => onChange([...value, { k: "", v: "" }])}>
          {addLabel}
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// A probe's report, drawn
// ---------------------------------------------------------------------------

function ReportView({ report }: { report: McpProbeReport }) {
  const lines = reportLines(report);
  const [open, setOpen] = useState(false);
  return (
    <div className={`rounded-card border p-2 text-2xs ${lines.ok ? "border-ok/40 bg-ok-soft/40" : "border-warn/40 bg-warn-soft/40"}`} data-mcp-report>
      <div className="flex items-center gap-2">
        {lines.ok ? <ICON.check size={12} aria-hidden className="text-ok" /> : <ICON.warn size={12} aria-hidden className="text-warn" />}
        <span className="font-medium text-text">{lines.headline}</span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5 text-text-dim">
        {lines.detail.map((d, i) => (
          <li key={i}>{d}</li>
        ))}
      </ul>
      {lines.ok && capabilityWords(report.capabilities).length > 0 && (
        <div className="mt-1 flex flex-wrap gap-1">
          {capabilityWords(report.capabilities).map((c) => (
            <Chip key={c} tone="neutral">
              {c}
            </Chip>
          ))}
        </div>
      )}
      {lines.tools.length > 0 && (
        <div className="mt-1.5">
          <button type="button" aria-expanded={open} onClick={() => setOpen((o) => !o)} className="anim flex items-center gap-1 rounded-control text-text-dim hover:text-text">
            <ICON.collapsed size={11} aria-hidden className={open ? "rotate-90" : ""} />
            {tr("settings-mcp-health-tools", { n: lines.tools.length })}
            {lines.more > 0 && ` ${tr("settings-mcp-panel-more", { more: lines.more })}`}
          </button>
          {open && (
            <ul className="mt-1 flex flex-col gap-0.5 pl-4 font-mono">
              {lines.tools.map((t) => (
                <li key={t.name} className="text-text">
                  {t.name}
                  {t.description && <span className="ml-2 font-sans text-text-dim">{t.description}</span>}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The editor
// ---------------------------------------------------------------------------

function McpEditor({ open, server, onClose, onSaved }: { open: boolean; server: McpServerView | null; onClose: () => void; onSaved: () => void }) {
  const toast = useToast();
  const editing = server !== null;
  const [d, setD] = useState<McpDraft>(blank);
  const [errors, setErrors] = useState<McpFormErrors>({});
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [report, setReport] = useState<McpProbeReport | null>(null);
  /** Which draft is on screen, counted: a test's answer is drawn only over the draft it dialled. */
  const version = useRef(0);

  useEffect(() => {
    if (!open) return;
    version.current += 1;
    setD(server ? fromDef(server) : blank());
    setErrors({});
    setReport(null);
  }, [open, server]);

  const set = (p: Partial<McpDraft>) => {
    version.current += 1;
    setD((prev) => ({ ...prev, ...p }));
    setReport(null);
  };

  const save = async () => {
    const found = validate(d, editing);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setBusy(true);
    try {
      if (server) {
        await api.patchMcp(server.id, { description: d.description.trim(), tags: d.tags, transport: toTransport(d) });
        toast.ok(tr("settings-mcp-panel-server-updated"));
      } else {
        await api.createMcp({ id: d.id.trim(), description: d.description.trim(), tags: d.tags, transport: toTransport(d) });
        toast.ok(tr("settings-mcp-panel-server-registered-machine"));
      }
      setBusy(false);
      onSaved();
      onClose();
    } catch (e) {
      setBusy(false);
      const message = failureText("settings", "mcp-panel-failed", e);
      // A 4xx is an answer about an input — the reserved name, a taken id, a
      // URL that is not one — and belongs beside the field rather than in a
      // toast that vanishes. Which field is the refusal's id, never its words.
      if (e instanceof ApiError && e.status >= 400 && e.status < 500) setErrors({ [fieldForRefusal(e.refusal)]: message });
      else setErrors({ form: message });
    }
  };

  /** Dial the draft as it stands — nothing is saved by it. */
  const test = async () => {
    const found = validate({ ...d, id: d.id || "draft" }, true);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setTesting(true);
    setReport(null);
    // The draft this test dials: an answer that lands after the draft was edited says nothing about what is now on screen, and is not drawn.
    const dialled = version.current;
    try {
      const r = await api.probeMcp({ transport: toTransport(d) });
      if (probeStillAbout(dialled, version.current)) setReport(r.report);
    } catch (e) {
      if (!probeStillAbout(dialled, version.current)) return;
      const message = failureText("settings", "mcp-panel-failed", e);
      if (e instanceof ApiError && e.status >= 400 && e.status < 500) setErrors({ [fieldForRefusal(e.refusal)]: message });
      else setErrors({ form: message });
    } finally {
      setTesting(false);
    }
  };

  const err = (key: keyof McpFormErrors) => (errors[key] ? <span className="text-danger">{errors[key]}</span> : undefined);
  const remote = d.kind === "http" || d.kind === "sse";
  const hasMaskedDraft = editing && (d.env.some((p) => isMasked(p.v)) || d.headers.some((p) => isMasked(p.v)));

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={editing ? tr("settings-mcp-panel-edit", { server: server.name }) : tr("settings-mcp-panel-register-mcp-server")}
      description={tr("settings-mcp-panel-registered-here-referenced-id-from-agent")}
      width="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" disabled={busy || testing} onClick={onClose}>{tr("settings-connectors-panel-cancel")}</Button>
          {/* Disabled over a masked value, it says why on the button; enabled, the tooltip says what a test does. */}
          <Tooltip label={hasMaskedDraft ? undefined : tr("settings-mcp-panel-dial-server-configured-here-before-saving")}>
            <Button
              variant="default"
              disabled={busy || testing || hasMaskedDraft}
              disabledReason={hasMaskedDraft ? tr("settings-mcp-panel-test-dials-draft-typed-value-still") : undefined}
              onClick={() => void test()}
            >
              <ICON.inspect size={12} aria-hidden />
              {testing ? tr("settings-mcp-panel-testing") : tr("settings-mcp-panel-test-connection")}
            </Button>
          </Tooltip>
          <Button variant="primary" disabled={busy || testing} onClick={() => void save()}>
            {busy ? tr("settings-connectors-panel-saving") : editing ? tr("settings-decisions-panel-save") : tr("settings-mcp-panel-register")}
          </Button>
        </>
      }
    >
      {/* The dialog's body is the one scrollport, and an @container: the form reads the same in any width. */}
      <div className="flex flex-col gap-3">
        {errors.form && <div className="rounded-control border border-transparent bg-danger-soft px-3 py-2 text-2xs text-danger">{errors.form}</div>}

        <div className="grid gap-3 @lg:grid-cols-2">
          <Field label={tr("settings-mcp-panel-id")} error={errors.id} hint={editing ? tr("settings-mcp-panel-fixed-agents-reference-id") : tr("settings-mcp-panel-lowercase-letters-digits-agents-reference-so")}>
            <TextInput autoFocus={!editing} disabled={editing} value={d.id} /* for the machine */ placeholder="github" onChange={(e) => set({ id: e.target.value })} className="font-mono" />
          </Field>
          <Field label={tr("settings-git-profiles-panel-name")} error={errors.name} hint={tr("settings-mcp-panel-what-harness-calls-refused", { RESERVED })}>
            <TextInput autoFocus={editing} value={d.name} /* for the machine */ placeholder="github" onChange={(e) => set({ name: e.target.value })} />
          </Field>
        </div>

        <Field label={tr("settings-mcp-panel-description")} hint={tr("settings-mcp-panel-what-agent-gets-being-given-server")}>
          <TextArea rows={2} value={d.description} placeholder={tr("settings-mcp-panel-issues-pull-requests-code-search-github")} onChange={(e) => set({ description: e.target.value })} />
        </Field>

        <Labelled label={tr("settings-mcp-panel-tags")}>
          <TagInput value={d.tags} onChange={(tags) => set({ tags })} suggestions={[...TAG_VOCABULARY]} />
        </Labelled>

        {/* Three buttons, not one control: a caption over them, never a <label> around them. */}
        <Labelled label={tr("settings-mcp-panel-transport")} hint={kindWords(d.kind).hint}>
          <SegmentedControl
            label={tr("settings-mcp-panel-transport")}
            size="sm"
            value={d.kind}
            onChange={(v) => set({ kind: v as McpKind })}
            options={KINDS.map((k) => ({ id: k, label: kindWords(k).label, hint: kindWords(k).hint }))}
          />
        </Labelled>

        {!remote ? (
          <>
            <Field label={tr("settings-keymap-panel-command")} error={errors.command} hint={tr("settings-mcp-panel-executable-without-arguments")}>
              <TextInput value={d.command} /* for the machine */ placeholder="npx" onChange={(e) => set({ command: e.target.value })} className="font-mono" />
            </Field>
            <Field label={tr("settings-mcp-panel-arguments")} hint={tr("settings-mcp-panel-one-per-line-so-quoted-path")}>
              <TextArea rows={3} value={d.args} /* for the machine */ placeholder={"-y\n@modelcontextprotocol/server-github"} onChange={(e) => set({ args: e.target.value })} className="font-mono" />
            </Field>
            <Field label={tr("settings-mcp-panel-working-directory")} error={errors.cwd} hint={tr("settings-mcp-panel-optional-absolute-left-blank-process-starts")}>
              <TextInput value={d.cwd} /* for the machine */ placeholder="/Users/me/servers/github" onChange={(e) => set({ cwd: e.target.value })} className="font-mono" />
            </Field>
            <Labelled label={tr("settings-mcp-panel-environment")} hint={err("env") ?? tr("settings-mcp-panel-kept-machine-never-read-back-saved", { MASK })}>
              <PairEditor value={d.env} onChange={(env) => set({ env })} /* for the machine */ keyPlaceholder="GITHUB_TOKEN" valuePlaceholder={tr("settings-mcp-panel-value-placeholder")} addLabel={tr("settings-mcp-panel-add-variable")} keyLabel={tr("settings-mcp-panel-variable")} />
            </Labelled>
          </>
        ) : (
          <>
            <Field label={tr("settings-mcp-panel-url")} error={errors.url} hint={d.kind === "sse" ? tr("settings-mcp-panel-url-opens-event-stream") : tr("settings-mcp-panel-one-endpoint-harness-calls")}>
              <TextInput value={d.url} /* for the machine */ placeholder={d.kind === "sse" ? "https://mcp.example.com/sse" : "https://mcp.example.com/mcp"} onChange={(e) => set({ url: e.target.value })} className="font-mono" />
            </Field>
            <Labelled label={tr("settings-mcp-panel-headers")} hint={err("headers") ?? tr("settings-mcp-panel-sent-every-request-bearer-token-api", { MASK })}>
              <PairEditor value={d.headers} onChange={(headers) => set({ headers })} keyPlaceholder={tr("settings-mcp-panel-authorization")} valuePlaceholder={tr("settings-mcp-panel-bearer")} addLabel={tr("settings-mcp-panel-add-header")} keyLabel={tr("settings-mcp-panel-header")} />
            </Labelled>
          </>
        )}

        {report && <ReportView report={report} />}
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// One registered server
// ---------------------------------------------------------------------------

function McpCard({ server, onEdit, reload, checking, onCheck }: { server: McpServerView; onEdit: () => void; reload: () => void; checking: boolean; onCheck: () => void }) {
  const toast = useToast();
  // The record's `enabled` reads as optional because it defaults when written; the node always says it.
  const enabled = server.enabled ?? true;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  // Asked per row rather than on open, because Delete sits in the row itself
  // and there is nothing to expand first. The registry is small by
  // construction — nothing ships in it — so this is a handful of local
  // requests, and what it buys is a row that never offers a button whose only
  // outcome is the node's refusal.
  const usage = useUsage("mcp", server.id);

  const setEnabled = async (next: boolean) => {
    setBusy(true);
    setError(null);
    await attempt(
      () => api.patchMcp(server.id, { enabled: next }),
      setError,
      () => {
        toast.ok(next ? tr("settings-mcp-panel-switched") : tr("settings-mcp-panel-switched-off"));
        reload();
      },
    );
    setBusy(false);
  };

  const health = server.health;
  const secrets = secretCount(server.transport);

  return (
    <Card className={enabled ? "" : "opacity-70"} data-mcp-row={server.id}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="truncate text-xs font-medium">{server.name}</span>
        <code className="font-mono text-2xs text-text-dim">{server.id}</code>
        <Chip tone="neutral">{kindWords(server.transport.transport).label}</Chip>
        {enabled && (
          <Tooltip label={[healthWords(health), checkedWords(health.checked_at)].filter(Boolean).join(" · ")}>
            <span className="inline-flex">
              <Chip tone={checking ? "quiet" : healthTone(health)}>{checking ? tr("settings-system-permissions-checking") : healthWords(health)}</Chip>
            </span>
          </Tooltip>
        )}
        {secrets && (
          <Chip tone="neutral" title={tr("settings-mcp-panel-kept-machine-never-read-back")}>
            {secrets}
          </Chip>
        )}
        <TagChips tags={server.tags ?? []} max={4} />
        {/* The switch is the state, Check the row's one verb; Edit and Delete wait in the menu. */}
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <Switch checked={enabled} disabled={busy} label={tr("settings-mcp-panel-server-enabled")} onChange={(next) => void setEnabled(next)} />
          <Tooltip label={enabled ? tr("settings-mcp-panel-dial-server-read-what-answers") : undefined}>
            <Button
              size="sm"
              variant="ghost"
              disabled={checking || !enabled}
              disabledReason={enabled ? undefined : tr("settings-mcp-panel-enable-server-check")}
              onClick={onCheck}
            >
              <ICON.inspect size={12} aria-hidden />
              {checking ? tr("settings-mcp-panel-checking-2") : tr("settings-mcp-panel-check")}
            </Button>
          </Tooltip>
          <MoreMenu
            label={tr("settings-mcp-panel-more-server", { server: server.name })}
            items={[
              { label: tr("settings-connectors-panel-edit"), onSelect: onEdit },
              // What the usage check found is the label: still checking, held by N things (the dialog lists them), or free to delete.
              usage.checking
                ? { label: tr("work-library-refs-checking-usage"), disabled: true, separatorBefore: true, onSelect: () => undefined }
                : (usage.refs?.length ?? 0) > 0
                  ? { label: tr("work-library-refs-in-use-by-things", { n: usage.refs?.length ?? 0 }), separatorBefore: true, onSelect: () => setConfirming(true) }
                  : { label: tr("work-library-refs-delete"), danger: true, separatorBefore: true, onSelect: () => setConfirming(true) },
            ]}
          />
        </div>
      </div>

      {server.description && <p className="mt-1 max-w-measure text-2xs leading-relaxed text-text-dim">{server.description}</p>}

      <div className="mt-1.5 min-w-0 overflow-x-auto">
        <code className="font-mono text-2xs whitespace-nowrap text-text-dim" title={transportLine(server.transport)}>
          {transportLine(server.transport)}
        </code>
      </div>

      {health.state === "failing" && health.error && <p className="mt-1 text-2xs text-warn">{health.error}</p>}

      {error && (
        <div className="mt-1.5">
          <ErrorNote error={error} />
        </div>
      )}

      <DeleteDialog
        open={confirming}
        title={tr("settings-mcp-panel-delete", { server: server.name })}
        usage={usage}
        consequence={tr("settings-mcp-panel-registry-entry-goes-nothing-uninstalled-from")}
        note={tr("settings-mcp-panel-detaching-server-from-agent-costs-agent")}
        remove={() => api.deleteMcp(server.id)}
        onClose={() => setConfirming(false)}
        onDeleted={() => {
          toast.ok(tr("settings-mcp-panel-server-removed-from-registry"));
          reload();
        }}
      />
    </Card>
  );
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

export function McpPanel() {
  const toast = useToast();
  const { data, error, loading, reload } = useAsync((s) => api.mcps(undefined, s), []);
  const [filter, setFilter] = useState<TagFilterState>(NO_TAG_FILTER);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<McpServerView | null>(null);
  const [checking, setChecking] = useState<ReadonlySet<string>>(new Set());

  // A probe finished anywhere — the CLI, another window — the rows read it.
  useEngineEvents((e) => {
    if (e.payload.type === "mcp_probed") reload();
  });

  const rows = data?.mcp ?? [];
  const shown = rows.filter((m) => passesTagFilter(m.tags ?? [], filter));
  const on = shown.filter((m) => m.enabled ?? true);
  const off = shown.filter((m) => !(m.enabled ?? true));

  /** Dial one server; the row says *checking…* meanwhile and the bus frame refreshes it. */
  const check = async (id: string) => {
    setChecking((c) => new Set([...c, id]));
    try {
      await api.probeMcpById(id);
    } catch (e) {
      toast.error(failureText("settings", "mcp-panel-failed", e));
    } finally {
      setChecking((c) => {
        const next = new Set(c);
        next.delete(id);
        return next;
      });
      reload();
    }
  };

  /** Every enabled row, a few at a time — the engine joins a second ask to a running probe, so more buys nothing. */
  const checkAll = async () => {
    const queue = on.map((m) => m.id);
    const workers = Array.from({ length: Math.min(CHECK_ALL_AT_ONCE, queue.length) }, async () => {
      while (queue.length > 0) {
        const id = queue.shift();
        if (id) await check(id);
      }
    });
    await Promise.all(workers);
  };

  const editors = (
    <McpEditor
      open={creating || editing !== null}
      server={editing}
      onClose={() => {
        setCreating(false);
        setEditing(null);
      }}
      onSaved={reload}
    />
  );

  if (loading && !data) return <Pending what={tr("settings-mcp-panel-registry")} rows={pendingRows(tr("settings-mcp-panel-registry"))} />;
  if (error && !data) return <ErrorNote error={error} retry={reload} />;

  if (rows.length === 0) {
    return (
      <>
        <EmptyState
          icon={ICON.mcpServer}
          title={tr("settings-mcp-panel-mcp-servers-registered")}
          hint={tr("settings-mcp-panel-nothing-ships-registry-purpose-bundled-entry")}
          action={
            <Button variant="primary" onClick={() => setCreating(true)}>{tr("settings-mcp-panel-register-server")}</Button>
          }
        />
        {editors}
      </>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center gap-2">
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-mcp-panel-registry-local-to-machine-never-syncs", { name: <code className="font-mono">{RESERVED}</code> })}</p>
        <Button size="sm" className="ml-auto shrink-0" variant="ghost" disabled={on.length === 0 || checking.size > 0} onClick={() => void checkAll()}>
          <ICON.inspect size={12} aria-hidden />
          {checking.size > 0 ? tr("settings-mcp-panel-checking", { checking: checking.size }) : tr("settings-mcp-panel-check-all")}
        </Button>
        <Button size="sm" className="shrink-0" variant="primary" onClick={() => setCreating(true)}>{tr("settings-mcp-panel-register-server")}</Button>
      </div>

      {error && <ErrorNote error={error} retry={reload} />}

      <TagFilterBar items={rows} tagsOf={(m) => m.tags ?? []} value={filter} onChange={setFilter} />

      <Section title={tr("settings-mcp-panel-enabled", { on: on.length })}>
        {on.length === 0 ? (
          <p className="text-2xs text-text-dim">{rows.length === shown.length ? tr("settings-mcp-panel-every-registered-server-switched-off") : tr("settings-mcp-panel-nothing-enabled-carries-those-tags")}</p>
        ) : (
          <div className="flex flex-col gap-2">
            {on.map((m) => (
              <McpCard key={m.id} server={m} onEdit={() => setEditing(m)} reload={reload} checking={checking.has(m.id)} onCheck={() => void check(m.id)} />
            ))}
          </div>
        )}
      </Section>

      {off.length > 0 && (
        <Section title={tr("settings-mcp-panel-disabled", { off: off.length })}>
          <p className="mb-2 max-w-measure text-2xs leading-relaxed text-text-dim">{tr("settings-mcp-panel-still-registered-skipped-launch-never-dialed")}</p>
          <div className="flex flex-col gap-2">
            {off.map((m) => (
              <McpCard key={m.id} server={m} onEdit={() => setEditing(m)} reload={reload} checking={false} onCheck={() => undefined} />
            ))}
          </div>
        </Section>
      )}

      {editors}
    </div>
  );
}
