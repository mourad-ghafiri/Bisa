/**
 * A read kept fresh by frames alone is stale across a node restart: what the
 * node forgot (an open ask, a server it served, a session) and what changed
 * while it was away reach the page by no frame. So **every source** that
 * listens to the bus — a store, a hook, a screen, a panel — reads again when
 * the bus comes back (`workspaceLoadModel.reloadOnReconnect`,
 * `ui/useReloadOnReconnect`), reads through `useAsync`, which does so for
 * every read made through it, or is named here with the reason it need not.
 * A source guard, as `logDoor.test.mjs` is: a file that reads through
 * `useAsync` is taken at its word for the reads it makes by hand.
 *
 * Run with `node --test desktop/src/reconnectReads.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));
// The bus's `subscribe`, under either name a store imports it as — a store fed by the bus under another alias was a store the guard never saw.
const LISTENS = /\b(useEngineEvents|useConversationEvents|useBus|busSubscribe|subscribeBus)\(/;
const RELOADS = /\b(useReloadOnReconnect|reloadOnReconnect)\(|\buseAsync(<|\()/;

/** Sources fed by the bus that do not read again, each with why that holds. */
const EXEMPT = Object.freeze({
  "shell/pathIndexStore.ts": "holds no read of its own: an index is fetched when Quick Open opens and patched by frames; a patch it cannot place forgets the index",
  "shell/followedSessionStore.ts": "holds a person's choice, not a read: a followed session that is gone is dropped against `sessionsStore`, which reads again",
  "views/_work/useGitConfigDraft.ts": "a draft a person is typing in, as `useProjectSettingsDraft` is: a read underneath it is flagged stale, never swapped in",
  "shell/notifications.ts": "says a fact once, as its frame lands, and holds no read: the switches it asks are `notifySettings`', which reads again",
  "views/_work/SecurityToasts.tsx": "toasts a refusal once, as its frame lands, and holds no read",
  "views/_work/WorkstreamScriptToasts.tsx": "toasts a failed script once, as its frame lands, and holds no read",
  "views/_work/SyncBar.tsx": "says a publish that did not go out once, as its frame lands, and holds no read of its own: the status it draws is the Git tab's, which reads again, and the notice itself is the Inbox's, which does too",
  "shell/BrowserPanel.tsx": "reads the parked requests on its own cadence (`BROWSER_PRESENCE_MS`), bus or no bus; a frame only brings one sooner",
  "draw/DrawPanel.tsx": "reads the parked requests on its own cadence (`DRAW_PRESENCE_MS`), bus or no bus; a frame only brings one sooner",
  "shell/NetworkStat.tsx": "reads through `networkStore`, which reads on its own cadence; a frame only brings a read sooner",
  "shell/DeviceLauncher.tsx": "the devices are `devicesStore.useDevices`', which reads again; what it reads by hand goes through `useAsync`",
  "views/_settings/MobileDevelopmentPanel.tsx": "the devices are `devicesStore.useDevices`', which reads again",
  "views/_workbench/DeviceDoc.tsx": "the devices are `devicesStore.useDevices`', which reads again",
  "draw/DrawEditor.tsx": "a canvas a person draws on: a save states the hash it read, so what was drawn while the node was away is a conflict the editor settles — a read underneath would swap the canvas under the pen",
  "views/_goal/DesigningCard.tsx": "draws the goal page's own read (`guidance`), which reads again; a frame only moves the card ahead of that read",
});

const busFed = () => sourceFiles(SRC, (p) => /\.tsx?$/.test(p) && !p.endsWith(".d.ts")).filter((file) => LISTENS.test(readFileSync(file, "utf8")));

test("a source fed by the bus reads again when the bus comes back, or says why it need not", () => {
  const owed = [];
  for (const file of busFed()) {
    const rel = relative(SRC, file);
    if (rel === "bus.ts" || EXEMPT[rel]) continue;
    if (!RELOADS.test(readFileSync(file, "utf8"))) owed.push(rel);
  }
  assert.deepEqual(owed, []);
});

test("an exemption names a file that still listens and still does not read again", () => {
  const fed = new Set(busFed().map((f) => relative(SRC, f)));
  for (const [rel, why] of Object.entries(EXEMPT)) {
    assert.ok(fed.has(rel), `${rel} no longer listens to the bus: drop its exemption`);
    assert.ok(!/\b(useReloadOnReconnect|reloadOnReconnect)\(/.test(readFileSync(`${SRC}/${rel}`, "utf8")), `${rel} reads again now: drop its exemption`);
    assert.ok(why.length > 20, `${rel} says why`);
  }
});

test("every read made through the shared hook is read again, and the devices' one reader too", () => {
  const hook = readFileSync(`${SRC}/views/_work/useAsync.ts`, "utf8");
  assert.ok(hook.includes("useReloadOnReconnect(reload)"), "`useAsync` reads again when the bus comes back");
  const devices = readFileSync(`${SRC}/shell/devicesStore.ts`, "utf8");
  assert.ok(/useReloadOnReconnect\(\(\) => \{\s*if \(load\) void refreshDevices\(\);/.test(devices), "`useDevices` reads again for every surface that asked for the list");
});

test("the screens of the four features read again: the Inbox's list, the Pulse's head, a goal's page, the library", () => {
  const reads = (rel) => readFileSync(`${SRC}/${rel}`, "utf8");
  assert.ok(reads("views/Inbox.tsx").includes("useReloadOnReconnect(() => void load())"));
  assert.ok(reads("views/Pulse.tsx").includes("useReloadOnReconnect(nudgeHead)"));
  for (const rel of ["views/GoalDetail.tsx", "views/Workflows.tsx", "views/WorkflowRun.tsx", "views/_workflow/GoalWorkflowTab.tsx", "views/_workflow/WorkflowPicker.tsx"]) {
    assert.ok(/\buseAsync(<|\()/.test(reads(rel)), `${rel} reads through the shared hook`);
  }
});

test("the switches a person set are read once the node is there — a node away at boot never leaves the defaults standing", () => {
  for (const rel of ["shell/notifySettings.ts", "shell/trayPrefs.ts", "shell/closeGuardSettings.ts", "shell/logSettings.ts", "shell/artifactSettings.ts"]) {
    assert.ok(readFileSync(`${SRC}/${rel}`, "utf8").includes("useReloadOnReconnect(() => void load())"), rel);
  }
});
