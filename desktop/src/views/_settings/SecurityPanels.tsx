/**
 * Settings › Security: the Redactor, the Tool & Commands Guard and the
 * Classifier, each its own panel over the node's policy.
 *
 * A rule is a setting — `security.redactor.rules`, `security.guard.rules` —
 * and a rule list **merges** across scopes: the workspace's rules (shared with
 * the team) and this machine's both apply, in that order. So the editor reads
 * and writes one scope's raw layer (`GET`/`PUT /settings/{scope}`), never the
 * resolved value, and says which scope it is editing. The built-ins ship with
 * the node and can be switched off, never deleted: a switch here writes the
 * rule's id into `builtins_off` at the chosen scope.
 *
 * What the node makes of the whole — every rule in order, the ones it could
 * not read, the classifier's readiness, the last decisions with their redacted
 * subjects — is `GET /security/status`, and it is what the summary cards show.
 * The words are `securityRules.mjs`'s. Nothing here ever holds a secret: a
 * preview runs on a scratch vault on the node, and a decision's subject is
 * redacted before it is remembered.
 */

import { useMemo, useState, type ReactNode } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { AgentDef, GuardDecision, GuardRule, RedactRule, SecurityStatus, SettingScope } from "../../types";
import {
  Button,
  Card,
  Chip,
  ErrorNote,
  Field,
  ICON,
  Pending,
  Section,
  Select,
  Switch,
  TextArea,
  TextInput,
  Tooltip,
  useToast,
} from "../../ui";
import { href } from "../../router";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, phase, phaseOf } from "./loadModel.mjs";
import { RegistryPanel } from "./RegistryPanel";
import { settingsPath, settingsSearch } from "./settingsLink.mjs";
import {
  ACTIONS,
  actionWords,
  blankGuardRule,
  blankRedactRule,
  classifierFieldsFor,
  detectorWords,
  draftOf,
  envDetectorWords,
  guardRuleProblem,
  harnessGuardWords,
  judgeWords,
  KEYS,
  MATCHERS,
  matcherWords,
  moveRule,
  offWords,
  problemLines,
  readinessLine,
  redactRuleProblem,
  slugOf,
  toggleBuiltin,
  verdictWords,
  withDraft,
  withoutDraft,
} from "./securityRules.mjs";
import { t as tr } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/** The two scopes a rule list lives at; a project never holds one. */
const RULE_SCOPES: readonly SettingScope[] = ["workspace", "machine"];

/** What every rule row has, whatever else it carries. `enabled` is optional on the wire and means true. */
interface RuleLike {
  id: string;
  label: string;
  enabled?: boolean;
}

/** A call whose answer the panel shows; the failure goes to the toast and the answer is `null`. */
async function fetchOr<T>(fn: () => Promise<T>, onError: (message: string) => void): Promise<T | null> {
  try {
    return await fn();
  } catch (e) {
    onError(e instanceof Error ? e.message : String(e));
    return null;
  }
}

function scopeWords(scope: SettingScope): string {
  return scope === "machine" ? tr("settings-security-panels-machine-only") : tr("settings-security-panels-workspace-shared-team");
}

/** The status, re-read whenever a `security.*` key changes anywhere. */
function useSecurityStatus() {
  const status = useAsync((s) => api.securityStatus(s), []);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && e.payload.keys.some((k) => k.startsWith("security."))) status.reload();
    if (e.payload.type === "guard_decided") status.reload();
  });
  return status;
}

/**
 * One scope's raw list under `key`, and the built-ins switched off there.
 * Reads the layer; writes go back to the same layer.
 */
function useLayerList<T>(scope: SettingScope, key: string, offKey: string) {
  const layer = useAsync((s) => api.settingsLayer(scope, null, s), [scope]);
  const rules = useMemo(() => (Array.isArray(layer.data?.values[key]) ? (layer.data?.values[key] as T[]) : []), [layer.data, key]);
  const off = useMemo(
    () => (Array.isArray(layer.data?.values[offKey]) ? (layer.data?.values[offKey] as string[]) : []),
    [layer.data, offKey],
  );
  return { layer, rules, off };
}

function ScopePicker({ scope, onChange }: { scope: SettingScope; onChange: (s: SettingScope) => void }) {
  return (
    <Field label={tr("settings-security-panels-rules-kept")} hint={tr("settings-security-panels-both-scopes-apply-workspace-s-first")}>
      <Select value={scope} onChange={(e) => onChange(e.target.value as SettingScope)}>
        {RULE_SCOPES.map((s) => (
          <option key={s} value={s}>
            {scopeWords(s)}
          </option>
        ))}
      </Select>
    </Field>
  );
}

/** The feature is switched off on this node: said above its rules, which draw idle. */
function OffBanner({ feature }: { feature: "redactor" | "guard" }) {
  const w = offWords(feature);
  return (
    <div role="status" className="rounded-control border border-warn/40 bg-warn-soft/40 px-3 py-2 text-2xs text-text">
      <Chip tone={w.tone}>{tr("settings-security-panels-off")}</Chip>
      <span className="ml-2">{w.text}</span>
    </div>
  );
}

function Problems({ lines }: { lines: string[] }) {
  if (lines.length === 0) return null;
  return (
    <div className="rounded-control border border-danger/40 bg-danger/5 px-2 py-1.5 text-2xs text-danger" role="alert">
      <p className="font-medium">{tr("settings-security-panels-node-could-not-read-rules", { rules: lines.length })}</p>
      <ul className="mt-0.5 list-disc pl-4">
        {lines.map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
    </div>
  );
}

/** The shipped rules, each with a switch; off is written into `builtins_off`. */
function Builtins<T extends RuleLike>({
  rules,
  words,
  scope,
  off,
  offKey,
  onChanged,
}: {
  rules: T[];
  words: (r: T) => string;
  scope: SettingScope;
  off: string[];
  offKey: string;
  onChanged: () => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const flip = async (id: string, enabled: boolean) => {
    setBusy(true);
    // Switching a built-in **on** must clear it from every scope's list, since
    // the lists are a union; switching it off is written where the picker says.
    const ok = await attempt(async () => {
      if (enabled) {
        for (const s of RULE_SCOPES) {
          const layer = await api.settingsLayer(s);
          const list = Array.isArray(layer.values[offKey]) ? (layer.values[offKey] as string[]) : [];
          if (list.includes(id)) await api.setSettings(s, { [offKey]: toggleBuiltin(list, id, true) });
        }
      } else {
        await api.setSettings(scope, { [offKey]: toggleBuiltin(off, id, false) });
      }
    }, toast.error);
    setBusy(false);
    if (ok) onChanged();
  };
  return (
    <div className="flex flex-col gap-1">
      {rules.map((r) => (
        <div key={r.id} className="flex items-center gap-2 rounded-control border border-border px-2 py-1.5">
          <Switch checked={r.enabled !== false} disabled={busy} onChange={(v) => void flip(r.id, v)} label={r.label} />
          <span className="min-w-0 flex-1 truncate font-mono text-2xs text-text-dim" title={words(r)}>
            {words(r)}
          </span>
          <Chip tone="quiet">{tr("settings-security-panels-built")}</Chip>
        </div>
      ))}
    </div>
  );
}

/** A draft list of a person's own rules: rows, order, save. */
function UserRules<T extends RuleLike>({
  title,
  scope,
  rules,
  blank,
  problem,
  settingKey,
  row,
  onChanged,
}: {
  title: string;
  scope: SettingScope;
  rules: T[];
  blank: () => T;
  problem: (rule: T, taken: string[]) => string | null;
  /** The `security.*.rules` key the list is written under. */
  settingKey: string;
  row: (rule: T, patch: (next: T) => void) => ReactNode;
  onChanged: () => void;
}) {
  const toast = useToast();
  // One draft a scope (`securityRules.draftOf`): a list is written whole to
  // the scope it was read from, so what was edited at the workspace is never
  // saved over this machine's rules after the picker moved.
  const [drafts, setDrafts] = useState<Partial<Record<string, T[]>>>({});
  const [busy, setBusy] = useState(false);
  const { rules: draft, dirty } = draftOf(drafts, scope, rules);
  const edit = (next: T[]) => setDrafts((all) => withDraft(all, scope, next));
  const taken = draft.map((r) => r.id);
  const problems = draft.map((r) => problem(r, taken));
  const blocked = problems.some(Boolean);
  const save = async () => {
    // The scope and the list on screen as the button was pressed: they are written together.
    const at = scope;
    const list = draft;
    setBusy(true);
    const ok = await attempt(() => api.setSettings(at, { [settingKey]: list }), toast.error);
    setBusy(false);
    if (ok) {
      setDrafts((all) => withoutDraft(all, at));
      toast.ok(tr("settings-security-panels-one-rule-rules-saved-machine-workspace", { draft: list.length, flag: (at === "machine") ? "yes" : "no" }));
      onChanged();
    }
  };
  return (
    <Section
      title={title}
      action={
        <div className="flex items-center gap-1">
          {dirty && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setDrafts((all) => withoutDraft(all, scope))}>{tr("settings-global-git-panel-discard")}</Button>
          )}
          <Button size="sm" disabled={!dirty || blocked || busy} onClick={() => void save()}>{tr("settings-decisions-panel-save")}</Button>
        </div>
      }
    >
      <div className="flex flex-col gap-2">
        {draft.length === 0 && <p className="text-2xs text-text-dim">{tr("settings-security-panels-no-rules-of-your-own-at", { scope: scopeWords(scope) })}</p>}
        {draft.map((r, i) => (
          <div key={i} className="rounded-control border border-border p-2">
            <div className="flex items-center gap-1">
              <Switch checked={r.enabled !== false} onChange={(v) => edit(draft.map((x, j) => (j === i ? { ...x, enabled: v } : x)))} label="" />
              <TextInput
                value={r.label}
                placeholder={tr("settings-git-profiles-panel-name")}
                aria-label={tr("settings-security-panels-rule-name")}
                className="w-48"
                onChange={(e) => {
                  const label = e.target.value;
                  edit(draft.map((x, j) => (j === i ? { ...x, label, id: x.id === slugOf(x.label) || !x.id ? slugOf(label) : x.id } : x)));
                }}
              />
              <span className="font-mono text-2xs text-text-dim">{r.id || "—"}</span>
              <span className="flex-1" />
              <Tooltip label={tr("settings-security-panels-earlier-rules-tried-first")}>
                <Button size="sm" variant="ghost" aria-label={tr("settings-security-panels-move-up")} disabled={i === 0} onClick={() => edit(moveRule(draft, i, -1))}>
                  ▲
                </Button>
              </Tooltip>
              <Button size="sm" variant="ghost" aria-label={tr("settings-security-panels-move-down")} disabled={i === draft.length - 1} onClick={() => edit(moveRule(draft, i, 1))}>
                ▼
              </Button>
              <Button size="sm" variant="ghost" aria-label={tr("settings-git-profiles-panel-remove-2")} onClick={() => edit(draft.filter((_, j) => j !== i))}>
                <ICON.close size={11} aria-hidden />
              </Button>
            </div>
            <div className="mt-1.5 flex flex-wrap items-center gap-1">{row(r, (next) => edit(draft.map((x, j) => (j === i ? next : x))))}</div>
            {problems[i] && (
              <p className="mt-1 text-2xs text-danger" role="alert">
                {problems[i]}
              </p>
            )}
          </div>
        ))}
        <div>
          <Button size="sm" onClick={() => edit([...draft, blank()])}>{tr("settings-security-panels-add-rule")}</Button>
        </div>
      </div>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// The Redactor
// ---------------------------------------------------------------------------

export function RedactorPanel() {
  const status = useSecurityStatus();
  const [scope, setScope] = useState<SettingScope>("workspace");
  const { layer, rules, off } = useLayerList<RedactRule>(scope, KEYS.redactor.rules, KEYS.redactor.builtinsOff);
  const reload = () => {
    status.reload();
    layer.reload();
  };
  // The shipped patterns, then the detectors armed from this node's own
  // environment — listed by name so one can be switched off; never a value.
  const builtins = (status.data?.redact_rules ?? []).filter((r) => r.origin === "builtin" && !r.id.startsWith("env:"));
  const envRules = (status.data?.redact_rules ?? []).filter((r) => r.origin === "builtin" && r.id.startsWith("env:"));
  const isOff = status.data ? !status.data.redactor_enabled : false;
  const env = envDetectorWords(status.data);

  return (
    <div className="mb-4 flex max-w-3xl flex-col gap-4">
      {status.error && <ErrorNote error={status.error} retry={status.reload} />}
      {isOff && <OffBanner feature="redactor" />}
      <Problems lines={problemLines(status.data?.problems, "redactor")} />
      <Section title={tr("settings-security-panels-what-redactor-recognises")}>
        {phase(status) !== "ready" ? (
          <Pending what={tr("settings-security-panels-the-built-in-rules")} rows={pendingRows(tr("settings-security-panels-the-built-in-rules"))} />
        ) : (
          <div className={isOff ? "opacity-60" : undefined}>
            <Builtins
              rules={builtins}
              words={(r) => detectorWords(r.detector)}
              scope={scope}
              off={off}
              offKey={KEYS.redactor.builtinsOff}
              onChanged={reload}
            />
          </div>
        )}
      </Section>
      <Section title={tr("settings-security-panels-node-s-environment")}>
        <p className={`mb-1.5 text-2xs ${env.tone === "ok" ? "text-text" : "text-text-dim"}`}>{env.text}</p>
        {envRules.length > 0 && (
          <div className={isOff ? "opacity-60" : undefined}>
            <Builtins
              rules={envRules}
              words={(r) => detectorWords(r.detector)}
              scope={scope}
              off={off}
              offKey={KEYS.redactor.builtinsOff}
              onChanged={reload}
            />
          </div>
        )}
        <p className="mt-1.5 text-2xs text-text-dim">{tr("settings-security-panels-variable-whose-name-says-is-detector")}</p>
      </Section>
      <ScopePicker scope={scope} onChange={setScope} />
      {phase(layer) === "failed" ? (
        <ErrorNote error={layer.error ?? tr("settings-security-panels-layer-read-refused")} retry={layer.reload} />
      ) : phase(layer) === "pending" ? (
        <Pending what={tr("settings-security-panels-rules-2")} rows={pendingRows(tr("settings-security-panels-rules-2"))} />
      ) : (
        <UserRules
          title={tr("settings-security-panels-rules")}
          scope={scope}
          rules={rules}
          blank={blankRedactRule}
          problem={redactRuleProblem}
          settingKey={KEYS.redactor.rules}
          onChanged={reload}
          row={(r, patch) => (
            <>
              <Select
                value={r.detector.kind}
                aria-label={tr("settings-security-panels-recognised")}
                onChange={(e) =>
                  patch({
                    ...r,
                    detector: e.target.value === "env_value" ? { kind: "env_value", name: "" } : { kind: "pattern", regex: "" },
                  })
                }
              >
                <option value="pattern">{tr("settings-security-panels-pattern")}</option>
                <option value="env_value">{tr("settings-security-panels-value-environment-variable")}</option>
              </Select>
              {r.detector.kind === "pattern" ? (
                <TextInput
                  value={r.detector.regex}
                  placeholder={tr("settings-security-panels-teamkey-0-9-6-group-named")}
                  aria-label={tr("settings-security-panels-pattern-2")}
                  className="min-w-0 flex-1 font-mono"
                  onChange={(e) => patch({ ...r, detector: { kind: "pattern", regex: e.target.value } })}
                />
              ) : (
                <TextInput
                  value={r.detector.name}
                  placeholder="MY_API_KEY" // for the machine
                  aria-label={tr("settings-security-panels-variable")}
                  className="min-w-0 flex-1 font-mono"
                  onChange={(e) => patch({ ...r, detector: { kind: "env_value", name: e.target.value.toUpperCase() } })}
                />
              )}
            </>
          )}
        />
      )}
      <RedactTryIt />
    </div>
  );
}

/** Paste a text, see what the rules would do to it — on the node, on a scratch vault. */
function RedactTryIt() {
  const toast = useToast();
  const [text, setText] = useState("");
  const [result, setResult] = useState<{ text: string; count: number; kinds: string[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    const out = await fetchOr(() => api.redactPreview(text), toast.error);
    setBusy(false);
    if (out) setResult(out);
  };
  return (
    <Section title={tr("settings-decisions-panel-try")}>
      <p className="mb-1 text-2xs text-text-dim">{tr("settings-security-panels-paste-something-made-up-key-text")}</p>
      <TextArea value={text} rows={3} className="font-mono text-2xs" placeholder={tr("settings-security-panels-deploy-akiaexampleexample00-tonight")} onChange={(e) => setText(e.target.value)} />
      <div className="mt-1 flex items-center gap-2">
        <Button size="sm" disabled={!text.trim() || busy} onClick={() => void run()}>{tr("settings-security-panels-redact")}</Button>
        {result && (
          <span className="text-2xs text-text-dim">
            {result.count === 0 ? tr("settings-security-panels-nothing-recognised") : tr("settings-security-panels-redacted", { result: result.count, kinds: result.kinds.join(", ") })}
          </span>
        )}
      </div>
      {result && result.count > 0 && <pre className="mt-1 whitespace-pre-wrap rounded-control bg-surface-2 p-2 font-mono text-2xs">{result.text}</pre>}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// The Guard
// ---------------------------------------------------------------------------

export function GuardPanel() {
  const status = useSecurityStatus();
  const [scope, setScope] = useState<SettingScope>("workspace");
  const { layer, rules, off } = useLayerList<GuardRule>(scope, KEYS.guard.rules, KEYS.guard.builtinsOff);
  const reload = () => {
    status.reload();
    layer.reload();
  };
  const builtins = (status.data?.guard_rules ?? []).filter((r) => r.origin === "builtin");
  const isOff = status.data ? !status.data.guard_enabled : false;

  return (
    <div className="mb-4 flex max-w-3xl flex-col gap-4">
      {status.error && <ErrorNote error={status.error} retry={status.reload} />}
      {isOff && <OffBanner feature="guard" />}
      <Problems lines={problemLines(status.data?.problems, "guard")} />
      <Section title={tr("settings-security-panels-what-guard-refuses-asks-about")}>
        {phase(status) !== "ready" ? (
          <Pending what={tr("settings-security-panels-the-built-in-rules")} rows={pendingRows(tr("settings-security-panels-the-built-in-rules"))} />
        ) : (
          <div className={isOff ? "opacity-60" : undefined}>
            <Builtins
              rules={builtins}
              words={(r) => `${actionWords(r.action)} · ${matcherWords(r.matcher)}`}
              scope={scope}
              off={off}
              offKey={KEYS.guard.builtinsOff}
              onChanged={reload}
            />
          </div>
        )}
      </Section>
      <ScopePicker scope={scope} onChange={setScope} />
      {phase(layer) === "failed" ? (
        <ErrorNote error={layer.error ?? tr("settings-security-panels-layer-read-refused")} retry={layer.reload} />
      ) : phase(layer) === "pending" ? (
        <Pending what={tr("settings-security-panels-rules-2")} rows={pendingRows(tr("settings-security-panels-rules-2"))} />
      ) : (
        <UserRules
          title={tr("settings-security-panels-rules-tried-order-after-built-ins")}
          scope={scope}
          rules={rules}
          blank={blankGuardRule}
          problem={guardRuleProblem}
          settingKey={KEYS.guard.rules}
          onChanged={reload}
          row={(r, patch) => (
            <>
              <Select value={r.action} aria-label={tr("settings-security-panels-action")} onChange={(e) => patch({ ...r, action: e.target.value as GuardRule["action"] })}>
                {ACTIONS.map((a) => (
                  <option key={a} value={a}>
                    {a} — {actionWords(a)}
                  </option>
                ))}
              </Select>
              <Select
                value={r.matcher.kind}
                aria-label={tr("settings-security-panels-looks")}
                onChange={(e) => {
                  const kind = e.target.value;
                  patch({
                    ...r,
                    matcher:
                      kind === "path" ? { kind: "path", glob: "" } : kind === "tool" ? { kind: "tool", name: "" } : kind === "any" ? { kind: "any" } : { kind: "command", regex: "" },
                  });
                }}
              >
                {MATCHERS.map((m) => (
                  <option key={m} value={m}>
                    {m === "command" ? tr("settings-security-panels-commands-matching") : m === "path" ? tr("settings-security-panels-paths-matching") : m === "tool" ? tr("settings-security-panels-tool-named") : tr("settings-security-panels-every-call")}
                  </option>
                ))}
              </Select>
              {r.matcher.kind === "command" && (
                <TextInput
                  value={r.matcher.regex}
                  placeholder={tr("settings-security-panels-git-push-b")}
                  aria-label={tr("settings-security-panels-pattern-2")}
                  className="min-w-0 flex-1 font-mono"
                  onChange={(e) => patch({ ...r, matcher: { kind: "command", regex: e.target.value } })}
                />
              )}
              {r.matcher.kind === "path" && (
                <TextInput
                  value={r.matcher.glob}
                  placeholder="~/.config/**" // for the machine
                  aria-label={tr("settings-security-panels-glob")}
                  className="min-w-0 flex-1 font-mono"
                  onChange={(e) => patch({ ...r, matcher: { kind: "path", glob: e.target.value } })}
                />
              )}
              {r.matcher.kind === "tool" && (
                <TextInput
                  value={r.matcher.name}
                  placeholder={tr("settings-security-panels-webfetch")}
                  aria-label={tr("settings-security-panels-tool")}
                  className="min-w-0 flex-1 font-mono"
                  onChange={(e) => patch({ ...r, matcher: { kind: "tool", name: e.target.value } })}
                />
              )}
            </>
          )}
        />
      )}
      <GuardTryIt />
      <Harnesses status={status.data} />
      <Recent decisions={status.data?.recent ?? []} />
    </div>
  );
}

const TRY_TOOLS = ["Bash", "Read", "Write", "Edit", "WebFetch"] as const;

/** A tool and an input, judged by the rules alone. */
function GuardTryIt() {
  const toast = useToast();
  const [tool, setTool] = useState<string>("Bash");
  const [text, setText] = useState("");
  const [result, setResult] = useState<{ verdict: string; label?: string | null; reason?: string | null; paths: string[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const isShell = tool === "Bash";
  const run = async () => {
    setBusy(true);
    const input = isShell ? { command: text } : { file_path: text };
    const out = await fetchOr(() => api.guardPreview(tool, input), toast.error);
    setBusy(false);
    if (out) setResult(out);
  };
  const words = result ? verdictWords(result.verdict) : null;
  return (
    <Section title={tr("settings-decisions-panel-try")}>
      <p className="mb-1 text-2xs text-text-dim">{tr("settings-security-panels-rules-alone-nothing-runs-nothing-asked")}</p>
      <div className="flex items-center gap-1">
        <Select value={tool} aria-label={tr("settings-security-panels-tool")} onChange={(e) => setTool(e.target.value)}>
          {TRY_TOOLS.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </Select>
        <TextInput
          value={text}
          className="min-w-0 flex-1 font-mono"
          placeholder={isShell ? tr("settings-security-panels-git-push-origin-main") : "/path/to/a/file"} // for the machine
          aria-label={isShell ? tr("settings-keymap-panel-command") : tr("settings-security-panels-path")}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && text.trim()) void run();
          }}
        />
        <Button size="sm" disabled={!text.trim() || busy} onClick={() => void run()}>{tr("settings-security-panels-judge")}</Button>
      </div>
      {result && words && (
        <div className="mt-1.5 flex flex-wrap items-center gap-2 text-2xs">
          <Chip tone={words.tone}>{words.text}</Chip>
          {result.label && <span className="text-text-dim">{tr("settings-security-panels-rule", { result: result.label })}</span>}
          {result.reason && <span className="text-text-dim">{tr("settings-security-panels-dash-reason", { reason: result.reason })}</span>}
          {result.paths.length > 0 && <span className="font-mono text-text-dim">{tr("settings-security-panels-paths", { paths: result.paths.join(", ") })}</span>}
        </div>
      )}
    </Section>
  );
}

function Harnesses({ status }: { status: SecurityStatus | null | undefined }) {
  if (!status) return null;
  return (
    <Section title={tr("settings-security-panels-which-harnesses-guard-can-stop")}>
      <div className="flex flex-col gap-1">
        {status.harnesses.map((h) => {
          const w = harnessGuardWords(h);
          return (
            <div key={h.id} className="flex items-center gap-2 text-2xs">
              <span className="w-32 shrink-0 font-mono">{h.id}</span>
              <Chip tone={w.tone} icon={h.tool_guard ? ICON.guard : undefined}>
                {w.text}
              </Chip>
            </div>
          );
        })}
        {status.harnesses.length === 0 && <p className="text-2xs text-text-dim">{tr("settings-security-panels-harness-registered-node")}</p>}
      </div>
      <p className="mt-1.5 text-2xs text-text-dim">{tr("settings-security-panels-harness-guard-judges-stopped-before-refused")}</p>
    </Section>
  );
}

function Recent({ decisions }: { decisions: GuardDecision[] }) {
  return (
    <Section title={tr("settings-security-panels-recent-decisions")}>
      {decisions.length === 0 ? (
        <p className="text-2xs text-text-dim">{tr("settings-decisions-panel-nothing-judged-since-node-started")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {decisions.slice(0, 20).map((d, i) => {
            const w = verdictWords(d.verdict);
            return (
              <li key={`${d.at}-${i}`} className="flex items-center gap-2 text-2xs">
                <Chip tone={w.tone}>{w.text}</Chip>
                <span className="shrink-0 font-medium">{d.tool}</span>
                <span className="min-w-0 flex-1 truncate font-mono text-text-dim" title={d.subject}>
                  {d.subject}
                </span>
                <span className="shrink-0 text-text-dim" title={d.reason ?? undefined}>
                  {judgeWords(d)}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------------
// The Classifier
// ---------------------------------------------------------------------------

export function ClassifierPanel() {
  const toast = useToast();
  const status = useSecurityStatus();
  const agents = useAsync(async (s) => (await api.agents(s)).agents, []);
  const [busy, setBusy] = useState(false);
  const line = readinessLine(status.data);
  const pick = async (agent: string) => {
    setBusy(true);
    const ok = await attempt(() => api.setSettings("workspace", { [KEYS.classifier.agent]: agent }), toast.error);
    setBusy(false);
    if (ok) status.reload();
  };
  const enabled: AgentDef[] = (agents.data ?? []).filter((a) => a.enabled !== false);
  const recent = (status.data?.recent ?? []).filter((d) => d.by === "classifier");
  const provider = status.data?.classifier.provider ?? "agent";
  const fields = classifierFieldsFor(provider);
  return (
    <div className="mb-4 flex max-w-3xl flex-col gap-4">
      {status.error && <ErrorNote error={status.error} retry={status.reload} />}
      <Card>
        <div className="flex items-center gap-2">
          <ICON.guard size={14} aria-hidden className="shrink-0 text-text-dim" />
          <Chip tone={line.tone}>{line.text}</Chip>
        </div>
        <p className="mt-1.5 text-2xs text-text-dim">{rich("settings-security-panels-classifier-blurb", { code: (inner) => <span className="font-mono">{inner}</span> })}</p>
      </Card>
      <RegistryPanel group="security" only={[KEYS.classifier.provider]} />
      {provider === "agent" && (
        <Field label={tr("settings-security-panels-classifier-agent")} hint={tr("settings-security-panels-any-enabled-agent-harness-models-what")}>
          {phaseOf([status, agents]) === "pending" && <Pending what={tr("settings-security-panels-agents")} rows={pendingRows(tr("settings-security-panels-agents"))} />}
          {phase(agents) === "failed" && <ErrorNote error={agents.error ?? tr("settings-security-panels-agents-read-refused")} retry={agents.reload} />}
          {phaseOf([status, agents]) === "ready" && (
          <Select value={status.data?.classifier.agent ?? ""} disabled={busy || !status.data} onChange={(e) => void pick(e.target.value)}>
            {status.data && !enabled.some((a) => a.id === status.data?.classifier.agent) && (
              <option value={status.data.classifier.agent}>{tr("settings-security-panels-classifier-agent-not-enabled", { agent: status.data.classifier.agent })}</option>
            )}
            {enabled.map((a) => (
              <option key={a.id} value={a.id}>
                {tr("settings-security-panels-name-id", { name: a.name, id: a.id })}
              </option>
            ))}
          </Select>
          )}
        </Field>
      )}
      {fields.length > 0 && <RegistryPanel group="security" only={fields} />}
      {provider === "decision_making_agent" && (
        <p className="rounded-control border border-border bg-surface-2 px-2 py-1.5 text-2xs text-text-dim">
          {rich("settings-security-panels-decision-making-agent-reads-redacted-call", { door: <a href={href({ name: "settings" }, settingsSearch("decision-making"))} className="text-accent-ink underline underline-offset-2">{settingsPath("decision-making")}</a> })}
        </p>
      )}
      <Section title={tr("settings-security-panels-what-classifier-decided")}>
        {recent.length === 0 ? (
          <p className="text-2xs text-text-dim">{tr("settings-security-panels-nothing-yet-since-node-started")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {recent.slice(0, 20).map((d, i) => {
              const w = verdictWords(d.verdict);
              return (
                <li key={`${d.at}-${i}`} className="flex items-center gap-2 text-2xs">
                  <Chip tone={w.tone}>{w.text}</Chip>
                  <span className="shrink-0 font-medium">{d.tool}</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-text-dim" title={d.subject}>
                    {d.subject}
                  </span>
                  {d.reason && <span className="shrink-0 truncate text-text-dim">{d.reason}</span>}
                </li>
              );
            })}
          </ul>
        )}
      </Section>
    </div>
  );
}
