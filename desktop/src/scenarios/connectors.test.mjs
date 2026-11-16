/**
 * Connectors as the desktop shows them (guide/connectors.md): a custom
 * definition is written only after the node has read it, the connector
 * step's spellings are its model's, a write is gated or owned by the switch,
 * and an unknown check state is said. Source assertions, as
 * `conversations.test.mjs` makes them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/connectors.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("a custom definition is validated on the node before it is written, and the rules are said", () => {
  const panel = src("../views/_settings/ConnectorsPanel.tsx");
  const save = panel.slice(panel.indexOf("const save = async"), panel.indexOf("const problems = problemLines(validation)"));
  assert.ok(save.indexOf("api.validateConnector(def)") < save.indexOf("api.createConnector(def)"), "Save validates first");
  assert.ok(save.includes("found.length > 0") && save.includes("return;"), "and writes nothing while a problem stands");
  assert.ok(save.includes("saveRefusedWords("), "saying so in the model's words");
  // The rules paragraph is one message of the catalog, said through `rich()`.
  assert.ok(panel.includes('rich("settings-connectors-panel-shape-blurb"'), "the panel says the rules paragraph");
  const blurb = readFileSync(new URL("../../../locales/en/desktop/settings.ftl", import.meta.url), "utf8");
  for (const rule of ["timeout_secs", "Idempotency-Key", "<code>page</code>", "Transfer-Encoding", "*.suffix", "64 KiB"]) {
    assert.ok(blurb.includes(rule), `the rules paragraph names ${rule}`);
  }
  assert.ok(panel.includes('aria-live="polite"'), "the paste-code box says when it goes away on its own");
});

test("the connector step's spellings and words are its model's, and a write is gated or owned", () => {
  const form = src("../views/_workflow/forms/ConnectorStepForm.tsx");
  assert.ok(form.includes('from "./connectorStepModel.mjs"'));
  assert.ok(!form.includes('const FIXED = "fixed:"'), "no spelling of its own");
  assert.ok(form.includes("<Switch") && form.includes("set({ unattended: on })"), "the person's word is a switch");
  assert.ok(form.includes("writeWords(unattended)"), "the warning reads by the switch");
  assert.ok(src("../views/_workflow/stepKinds.mjs").includes("ungated_write:"), "the validator's word has its sentence");
});

test("an unknown check state is said, never read as nothing", () => {
  const model = src("../views/_settings/connectorsModel.mjs");
  const tail = model.slice(model.indexOf("export function checkLine"));
  assert.ok(tail.includes("does not know"), "the default arm names the state");
  const health = src("../views/_settings/connectorHealthModel.mjs");
  assert.ok(health.slice(health.indexOf("export function healthWords")).includes('t("settings-connectors-check-answered-state-desktop-does-know"'), "and so does the row's health");
});

test("an account's health is the node's, on its row: a chip with when it was checked, a check, a check of every account, refreshed by the bus", () => {
  const panel = src("../views/_settings/ConnectorsPanel.tsx");
  assert.ok(panel.includes("healthTone(a.health)") && panel.includes("healthWords(a.health, row.name)") && panel.includes("checkedWords(a.health.checked_at, now)") && !panel.includes("<Tooltip label={[healthWords("), "the chip reads the row's health, and when is said beside it — never behind a hover");
  assert.ok(panel.includes("api.checkConnectorAccount(cid, aid)"), "a row checks itself through the node");
  assert.ok(panel.includes("checkAllLabel(pass)") && panel.includes("CHECK_ALL_AT_ONCE"), "the panel checks every account, a few at a time");
  assert.ok(panel.includes('t("settings-connectors-panel-checking-chip")'), "a row says *checking…* while its check runs, in a message of its own");
  assert.ok(!panel.includes("useState<Record<string, AccountCheck>>"), "no answer of the panel's own");
  const hooks = src("../views/_workflow/useConnectors.ts");
  assert.ok(hooks.includes("listReloads(e.payload)") && hooks.includes("detailReloads(e.payload, id ?? null)"), "a change refreshes the list and every detail; a check refreshes the detail of the connector it was about");
  const frames = src("../types.hand.ts");
  assert.ok(frames.includes('type: "connector_checked"; connector: string; account: string; ok: boolean'), "the frame is typed as the engine sends it");
});

test("the panel's doors are the model's: a flow remembered and landed by the row, the browser opened before the toast, one paste at a time, an edit's words, a validation of the text it read, Edit held until the definition, Check all once with its unread said, a failed refresh said over the rows", () => {
  const panel = src("../views/_settings/ConnectorsPanel.tsx");
  const connect = panel.slice(panel.indexOf("const connect = async"), panel.indexOf("const paste = async"));
  assert.ok(connect.includes("pendingFlow(start, a)"), "the flow remembers the row as it stood");
  assert.ok(connect.indexOf("await openExternal(flow.url)") < connect.indexOf("toast.ok(openedWords())"), "the toast says the browser is opening once it has");
  assert.ok(connect.includes("browserFailed: true"), "a browser that cannot open leaves the URL to copy");
  assert.ok(panel.includes('t("settings-connectors-panel-open-this-link-yourself")') && panel.includes("select-all"), "shown as a line to select");
  assert.ok(panel.includes("flowsSettled(flows, accounts, now)"), "the landing is a change to the row, an expiry and a lost account end the box too — by the model");
  assert.ok(panel.includes("useState<Record<string, PendingFlow>>({})") && !panel.includes("useState<PendingFlow | null>"), "one connection open per account, never one per card");
  assert.equal(panel.split("mayPaste({ flow, busy: busy !== null, now })").length - 1, 2, "the paste sends once, by the model — at the door and on the button");
  assert.ok(panel.includes("flowWords(flow, now)") && panel.includes("const now = useNow("), "the box says how long a code is taken, against a clock that moves");
  assert.ok(panel.includes("connectUriWords(Object.values(flows)[0].redirectUri)"), "the redirect URI is the flow's while one is open");
  const cancel = panel.slice(panel.indexOf("const cancel = "), panel.indexOf("const remove = async"));
  assert.ok(cancel.includes("api.cancelConnectorOauth(row.id, a.id)") && cancel.includes("toast.error"), "Cancel tells the node, and a node that could not be told is said");
  assert.ok(panel.includes("toast.info(t(\"settings-connectors-flow-ran-out\", { label }))"), "a connection that ran out of time is said");
  assert.ok(panel.includes("window.addEventListener(\"focus\", again)") && panel.includes("const again = () => reloadDetail();"), "back from the browser, the card's rows are read again");
  assert.ok(panel.includes("savedWords(body, account)"), "an edit says what moved");
  assert.ok(panel.includes("validationFor(seen, text)") && !panel.includes("setValidation("), "Valid. is about the text the node read");
  assert.ok(panel.includes("renamedProblem(initial?.id ?? null, def)"), "a renamed id is refused before the round trip");
  assert.ok(panel.includes("const definitionRead = detail.data !== null;") && panel.includes("mayEditDefinition({ busy: busy !== null, definitionRead })"), "Edit waits for the definition");
  assert.equal(panel.split("useConnectorDetail(").length - 1, 1, "one detail read per card: the dialog takes the card's parameters");
  assert.ok(panel.includes("useConnectorDetail(open ? row.id : null)") && panel.includes("useState(() => startsOpen(row))"), "and none for a card nobody opened");
  assert.ok(panel.includes("params={detail.data?.connector.params ?? null}"));
  assert.ok(panel.includes("{adding && detail.data && <AccountDialog") && panel.includes("disabled={busy !== null || !definitionRead}"), "an account is added only once the definition is read");
  assert.ok(panel.includes("accountBody({ label, values, secrets }, account, params)") && panel.includes("strayParams(account, params)") && panel.includes("missingRequired(values, params)"), "the form sends what is declared, says what it lets go of, and names what is required");
  assert.ok(panel.includes("t(\"settings-connectors-panel-account-gone\", { label: editing.label })"), "an account removed elsewhere closes its form with a word");
  assert.ok(panel.includes("accountVerbs(a, oauth, hasCheck)") && panel.includes("const hasCheck = Boolean(row.check);"), "Check is offered where the connector names one");
  assert.ok(panel.includes("readOnly={busy !== null}") && panel.includes("setParseError(null);") && panel.includes("if (text !== opened) setLeaving(true);") && panel.includes('role="status" aria-live="polite"'), "the editor holds its text while the node reads it, drops a stale parse error, asks before losing what was typed, and announces what was found");
  assert.ok(panel.includes("filterConnectors(connectors.rows, query)") && panel.includes("connectorFacts(row).map("), "the list is searched, and a card's facts are set apart");
  assert.ok(panel.includes("secretWhat(auth, f)") && panel.includes("secretLabel(auth, f)") && !panel.includes(".toLowerCase()"), "the secret input's noun is the field's own message — a further key's, the definition's own name for it");
  const checkAll = panel.slice(panel.indexOf("const checkAll = async"), panel.indexOf("const checkable ="));
  assert.ok(checkAll.indexOf('setPass({ phase: "reading", done: 0, total: 0 })') < checkAll.indexOf("api.connector(row.id)"), "held from the first read");
  assert.ok(checkAll.includes("} else unread += 1;") && checkAll.includes("checkAllReport({ queued, unread, failed })"), "what did not answer and what could not be read are one sentence");
  assert.ok(checkAll.includes("if (!here.current) return;") && checkAll.includes("next && here.current"), "a pass stops when the panel closes");
  assert.ok(panel.includes('disabled={!checkable || pass.phase !== "idle"}') && panel.includes("{checkAllLabel(pass)}"), "no second pass, and a row's own check never relabels the button");
  assert.ok(panel.includes("connectors.error && !empty && <ErrorNote"), "a failed refresh is said over the rows");
  assert.ok(panel.includes("disabled={connectors.loading || connectors.refreshing}"), "Read again is held while a read is on its way");
  assert.ok(src("../views/_workflow/useConnectors.ts").includes("refreshing: list.refreshing"));
});

test("the designer's connector pickers say what a step holds and the machine no longer has, a read that failed as such, commit a poll's parameters on blur, follow the connectors without an edit, and guess no connector for an account input", () => {
  const step = src("../views/_workflow/forms/ConnectorStepForm.tsx");
  const start = src("../views/_workflow/forms/StartStepForm.tsx");
  for (const form of [step, start]) {
    assert.ok(form.includes('import { ConnectorOptions, ParamRows } from "./ConnectorParts";') && form.includes("<ParamRows declared={declared} stray={stray} params={params} disabled={disabled} onParam={setParam} />") && form.includes("<ConnectorOptions rows={connectors.rows} />"), "one set of parameter rows and one connector picker, shared by the step and the poll");
    assert.ok(form.includes("strayAccount(") && form.includes("{notAnAccount !== null && <option value={accountValue("), "a lost account is named in the picker");
    assert.ok(form.includes("connectorPickerHint({ count: connectors.rows.length, error: connectors.error })"), "a list that could not be read says so");
    assert.ok(form.includes("definitionHint(detail.error,"), "a definition that could not be read says so under the operation");
    assert.ok(form.includes("accountsNote({ loading: detail.loading, error: detail.error,"), "the account note knows a read on its way");
    assert.ok(form.includes("const stray = def ? strayParams(params, declared) : [];"), "a stray is judged against a definition read");
  }
  const parts = src("../views/_workflow/forms/ConnectorParts.tsx");
  assert.ok(!step.includes("function TemplateField(") && parts.includes('import { TemplateField } from "./TemplateField";'), "the field lives in its own file, and the shared rows draw it");
  assert.ok(parts.includes("onCommit={(v) => onParam(p.name, v)}") && !start.includes("withParam(params, p.name, e.target.value)"), "a parameter is committed on blur or Enter — a poll's is no longer rewritten on every keystroke");
  assert.ok(parts.includes("isRequired(p) && <Chip") && parts.includes('p.kind === "path"'), "*required* is what a person must fill, and a path says what it is");
  assert.ok(start.includes("filter(pollable)") && start.includes("error={keyProblem(on.key) ?? undefined}") && !start.includes('placeholder={t("workflow-start-step-form-id")}'), "a poll offers what may be polled, and its key is required in words");
  assert.ok(step.includes("operationGroups(def?.operations)") && step.includes("operation && changes(operation) &&"), "a step's operations are grouped, and a delete is flagged as the write it is");
  assert.ok(step.includes("const promised = step.output_schema != null;") && step.includes('"workflow-connector-step-form-output-selected-whole"'), "fields of the output are promised only under a schema");
  assert.ok(start.includes("(declared.length > 0 || stray.length > 0)"), "a poll's strays are shown to be removed");
  const field = src("../views/_workflow/forms/TemplateField.tsx");
  assert.ok(field.includes("useCommitOnUnmount(") && field.includes('if (e.key === "Enter")'), "committed on blur, Enter, and on the way out");
  const inputs = src("../views/_workflow/InputsForm.tsx");
  assert.ok(inputs.includes("accountFieldNote({ connector, loading: detail.loading, error: detail.error, count: accounts.length })") && !inputs.includes("!detail.loading && accounts.length === 0"), "a read that failed is never *no account*");
  assert.ok(src("../views/_workflow/forms/InputDefsEditor.tsx").includes("{ ...base, kind, connector: null }"), "an account input guesses no connector");
  const live = src("../views/_workflow/useLiveValidation.ts");
  assert.ok(live.includes("if (listReloads(e.payload)) setConnectorsMoved((n) => n + 1);") && live.includes("}, [body, delay, connectorsMoved]);"), "the problems list follows the connectors without an edit");
});
