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
  assert.ok(panel.includes("healthTone(a.health)") && panel.includes("healthWords(a.health, row.name)") && panel.includes("checkedWords(a.health.checked_at)"), "the chip reads the row's health and says when");
  assert.ok(panel.includes("api.checkConnectorAccount(cid, aid)"), "a row checks itself through the node");
  assert.ok(panel.includes('t("settings-connectors-panel-check-all")') && panel.includes("CHECK_ALL_AT_ONCE"), "the panel checks every account, a few at a time");
  assert.ok(panel.includes('t("settings-system-permissions-checking")'), "a row says *checking…* while its check runs");
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
  assert.ok(panel.includes("flowLanded(pending, accounts.find((x) => x.id === pending.account))"), "the landing is a change to the row");
  assert.ok(panel.includes("mayPaste({ flow, busy: busy !== null, now })") && panel.includes("mayPaste({ flow: pending, busy: busy !== null, now })"), "the paste sends once, by the model");
  assert.ok(panel.includes("flowWords(pending, now)"), "the box says how long a code is taken");
  assert.ok(panel.includes("connectUriWords(pending.redirectUri)"), "the redirect URI is the flow's while one is open");
  assert.ok(panel.includes("savedWords(body, account)"), "an edit says what moved");
  assert.ok(panel.includes("validationFor(seen, text)") && !panel.includes("setValidation("), "Valid. is about the text the node read");
  assert.ok(panel.includes("renamedProblem(initial?.id ?? null, def)"), "a renamed id is refused before the round trip");
  assert.ok(panel.includes("mayEditDefinition({ busy: busy !== null, definitionRead: detail.data !== null })"), "Edit waits for the definition");
  assert.equal(panel.split("useConnectorDetail(").length - 1, 1, "one detail read per card: the dialog takes the card's parameters");
  assert.ok(panel.includes("params={detail.data?.connector.params ?? null}"));
  assert.ok(panel.includes("SECRET_FIELD_WHAT[f]") && !panel.includes(".toLowerCase()"), "the secret input's noun is the field's own message");
  const checkAll = panel.slice(panel.indexOf("const checkAll = async"), panel.indexOf("const checkable ="));
  assert.ok(checkAll.indexOf("setCheckingAll(true)") < checkAll.indexOf("api.connector(row.id)"), "held from the first read");
  assert.ok(checkAll.includes("else unread += 1;") && checkAll.includes("checkAllSummary({ queued, unread })"), "a connector that could not be read is counted and said");
  assert.ok(panel.includes("disabled={!checkable || checkingAll || checking.size > 0}"), "no second pass");
  assert.ok(panel.includes("connectors.error && !empty && <ErrorNote"), "a failed refresh is said over the rows");
  assert.ok(panel.includes("disabled={connectors.loading || connectors.refreshing}"), "Read again is held while a read is on its way");
  assert.ok(src("../views/_workflow/useConnectors.ts").includes("refreshing: list.refreshing"));
});

test("the designer's connector pickers say what a step holds and the machine no longer has, a read that failed as such, commit a poll's parameters on blur, follow the connectors without an edit, and guess no connector for an account input", () => {
  const step = src("../views/_workflow/forms/ConnectorStepForm.tsx");
  const start = src("../views/_workflow/forms/StartStepForm.tsx");
  for (const form of [step, start]) {
    assert.ok(form.includes('import { TemplateField } from "./TemplateField";'), "one template field, committed on blur or Enter");
    assert.ok(form.includes("strayAccount(") && form.includes("{notAnAccount !== null && <option value={accountValue("), "a lost account is named in the picker");
    assert.ok(form.includes("connectorPickerHint({ count: connectors.rows.length, error: connectors.error })"), "a list that could not be read says so");
    assert.ok(form.includes("definitionHint(detail.error,"), "a definition that could not be read says so under the operation");
    assert.ok(form.includes("accountsNote({ loading: detail.loading, error: detail.error,"), "the account note knows a read on its way");
    assert.ok(form.includes("const stray = def ? strayParams(params, declared) : [];"), "a stray is judged against a definition read");
  }
  assert.ok(!step.includes("function TemplateField("), "the field lives in its own file");
  assert.ok(start.includes('<TemplateField value={params[p.name] ?? ""}') && !start.includes("withParam(params, p.name, e.target.value)"), "a poll's parameter is no longer rewritten on every keystroke");
  assert.ok(start.includes("(declared.length > 0 || stray.length > 0)"), "a poll's strays are shown to be removed");
  const field = src("../views/_workflow/forms/TemplateField.tsx");
  assert.ok(field.includes("useCommitOnUnmount(") && field.includes('if (e.key === "Enter")'), "committed on blur, Enter, and on the way out");
  const inputs = src("../views/_workflow/InputsForm.tsx");
  assert.ok(inputs.includes("accountFieldNote({ connector, loading: detail.loading, error: detail.error, count: accounts.length })") && !inputs.includes("!detail.loading && accounts.length === 0"), "a read that failed is never *no account*");
  assert.ok(src("../views/_workflow/forms/InputDefsEditor.tsx").includes("{ ...base, kind, connector: null }"), "an account input guesses no connector");
  const live = src("../views/_workflow/useLiveValidation.ts");
  assert.ok(live.includes("if (listReloads(e.payload)) setConnectorsMoved((n) => n + 1);") && live.includes("}, [body, delay, connectorsMoved]);"), "the problems list follows the connectors without an edit");
});
