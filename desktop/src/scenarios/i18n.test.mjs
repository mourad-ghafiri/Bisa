/**
 * The translation layer as the sources show it (17 §Guards): every catalog
 * file parses for the desktop's runtime; every id the desktop says exists
 * in the English catalog and every desktop message is said somewhere; the
 * bare sentences per file equal the committed baseline; the door, the boot,
 * the header and the test preload are wired. Source assertions, as
 * `browser.test.mjs` makes them — no DOM. Run with
 * `node --test desktop/src/scenarios/i18n.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { FluentBundle, FluentResource } from "@fluent/bundle";

import { has } from "../i18n/l10n.mjs";
import { baseline, compare, withoutComments } from "../i18n/ratchetModel.mjs";
import { englishFiles } from "../i18n/testing.mjs";
import { sourceFiles } from "../testWalk.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(src, "..", "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");
const readRoot = (rel) => readFileSync(join(root, rel), "utf8");

test("every catalog file parses for the desktop's runtime, message by message", () => {
  const files = englishFiles(new URL("../../../locales/", import.meta.url));
  assert.ok(files.length > 0, "locales/ holds the catalog");
  const faults = [];
  for (const [path, text] of files) {
    const bundle = new FluentBundle("en", { useIsolating: false });
    for (const e of bundle.addResource(new FluentResource(text))) faults.push(`${relative(root, path)}: ${e.message}`);
  }
  assert.deepEqual(faults, []);
});

/** The English desktop catalog: id → the attribute names it carries. */
function desktopMessages() {
  const bundle = new FluentBundle("en", { useIsolating: false });
  const ids = new Map();
  for (const [path, text] of englishFiles()) {
    if (!/[\\/]desktop[\\/]/.test(path)) continue;
    const resource = new FluentResource(text);
    bundle.addResource(resource);
    for (const entry of resource.body) if (entry.id && !entry.id.startsWith("-")) ids.set(entry.id, new Set(Object.keys(entry.attributes ?? {})));
  }
  return { bundle, ids };
}

const isSource = (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !/\.test\.mjs$/.test(p) && !/[\\/]scenarios[\\/]/.test(p);

test("every id the desktop says is a message of the English catalog, and every desktop message is said", () => {
  const { ids } = desktopMessages();
  const said = new Map();
  const faults = [];
  for (const file of sourceFiles(src, isSource)) {
    const text = readFileSync(file, "utf8");
    const at = relative(src, file);
    // `t("id")`, `tr("id")` where a file has a `t` of its own, `rich("id", …)` for a sentence with a slot.
    for (const m of text.matchAll(/\b(?:t|tr|rich)\(\s*"([a-z0-9-]+)"/g)) {
      said.set(m[1], at);
      if (!ids.has(m[1])) faults.push(`${at}: t("${m[1]}") — no such message`);
    }
    for (const m of text.matchAll(/\battr\(\s*"([a-z0-9-]+)",\s*"([a-z0-9-]+)"/g)) {
      said.set(m[1], at);
      const attrs = ids.get(m[1]);
      if (!attrs) faults.push(`${at}: attr("${m[1]}", "${m[2]}") — no such message`);
      else if (!attrs.has(m[2])) faults.push(`${at}: attr("${m[1]}", "${m[2]}") — the message has no such attribute`);
    }
  }
  for (const id of ids.keys()) if (!said.has(id)) faults.push(`locales/en/desktop: \`${id}\` is a message nothing says`);
  assert.deepEqual(faults, []);
});

test("the bare sentences per file equal the committed baseline — a rise is a sentence for the catalog, a fall a baseline to lower", () => {
  let was;
  try {
    was = JSON.parse(read("i18n/ratchet.baseline.json"));
  } catch {
    assert.fail("desktop/src/i18n/ratchet.baseline.json: no baseline yet — run `just i18n-baseline`");
  }
  const { up, down } = compare(was, baseline(src));
  assert.deepEqual(up, [], "bare sentences appeared — say them through t() instead");
  assert.deepEqual(down, [], "bare sentences went — lower the baseline with `just i18n-baseline`");
});

test("a sentence moved by hand before the scanner could see its shape — a text node with a `;`, a `,` or a bracket — is held by name, and stays moved", () => {
  // Each: the screen, the message it says, and how the sentence began when the screen spelled it.
  const MOVED = [
    ["views/_work/GovernancePanel.tsx", "work-governance-panel-only-you-default", "Only you is the default."],
    ["views/_work/GovernancePanel.tsx", "work-governance-panel-what-each-role-may-do", "What each role may do."],
    ["views/_settings/ConnectorsPanel.tsx", "settings-connectors-panel-field-left-blank-keeps-stored", "A field left blank keeps what is stored"],
    ["views/_settings/ConnectorsPanel.tsx", "settings-connectors-panel-install-more-from-library", "Install more from Library"],
    ["views/_settings/GitProfilesPanel.tsx", "settings-git-profiles-panel-profile-who-you-are", "A profile is who you are for one organization"],
    ["views/Channels.tsx", "screens-channels-guest-reaches-only-when-listed", "A guest reaches this channel only when listed here"],
    ["views/Inbox.tsx", "screens-inbox-answered-at-harness-prompt", "Answered at the harness's own prompt"],
    ["views/Inbox.tsx", "screens-inbox-admitted", "Admitted."],
    ["views/GoalDetail.tsx", "screens-goal-detail-next-run-uses-workflow", "The workflow this goal's next run uses."],
    ["views/GoalDetail.tsx", "screens-goal-detail-close-replaced-by-hint", "The goal that takes this one's place, when there is one."],
    ["views/_work/GoalInspector.tsx", "work-goal-inspector-names-agent-every-step", "The Workflow Agent names an agent on every step it designs."],
    ["views/_work/GoalInspector.tsx", "work-goal-inspector-none-yet-see-projects", "None yet — a goal's projects"],
    ["views/_workflow/forms/ConditionEditor.tsx", "workflow-condition-editor-empty-group-problem", "An empty group is a problem"],
    ["views/_workflow/forms/ConnectorStepForm.tsx", "workflow-connector-step-form-file-path-inside-checkout", "A path inside the run's checkout"],
    ["views/_workflow/GoalWorkflowTab.tsx", "workflow-goal-workflow-tab-proposed-by-workflow-agent", "Proposed by the Workflow Agent"],
    ["views/_workflow/GoalWorkflowTab.tsx", "workflow-goal-workflow-tab-designed-for-goal", "Designed for this goal"],
    ["views/_board/boardModel.mjs", "board-board-close-record-moves-archived", "The record moves to Archived"],
    ["views/_board/boardModel.mjs", "board-board-over-doing-limit", "Over the Doing limit"],
    ["views/_work/projectForm.mjs", "work-project-form-host-own-choice", "— the host's own choice"],
    ["views/_work/removeProjectModel.mjs", "work-remove-project-including-every-checkout", ", including every checkout"],
    ["views/_work/commitFocusModel.mjs", "work-commit-focus-patch-cut-file-list-complete", "The patch was cut where the read stops"],
    ["views/_work/ConflictBlock.tsx", "work-conflict-block-started-from-both-sides", "Started from both sides"],
    ["views/_work/commitActionsModel.mjs", "work-commit-actions-none-of-git-refuses-them-ref-name", "None of ~ ^ : ? * [ or"],
  ];
  const { bundle } = desktopMessages();
  for (const [screen, id, began] of MOVED) {
    // A comment may say what the sentence means; the screen's own markup may not.
    const text = withoutComments(read(screen));
    assert.ok(text.includes(`("${id}")`), `${screen} says ${id}`);
    assert.ok(!text.includes(began), `${screen} spells none of it`);
    const message = bundle.getMessage(id);
    assert.ok(message?.value, `${id} is a message`);
    assert.ok(bundle.formatPattern(message.value).startsWith(began), `${id} says what the screen said`);
  }
});

test("the window's catalog is every namespace but the CLI's — the crates' engine.ftl included — so a platform sentence a post carries (`said`) renders here, and the timeline draws it through contentOf", () => {
  // The bundle: Vite's glob takes every `.ftl` of every language and leaves out the CLI's file alone; the test installer says the same.
  const catalog = read("i18n/catalog.ts");
  assert.ok(catalog.includes('"../../../locales/*/**/*.ftl"'), "every file of every language");
  assert.ok(catalog.includes('"!../../../locales/*/cli.ftl"'), "the CLI's file left out");
  assert.equal((catalog.match(/"!/g) ?? []).length, 1, "and nothing else left out");
  assert.ok(read("i18n/testing.mjs").includes('const NOT_THE_WEBVIEW = new Set(["cli.ftl"]);'), "a test installs the same set");
  // The messages the platform authors a post with (17 §A platform sentence in a post) are installed here, from the crates' file.
  for (const id of ["engine-conversation-reply-cut", "guided-say-design-off", "guided-say-cycle-failed"]) assert.ok(has(id), `${id} is in the installed catalog`);
  // The one place a message's words are drawn reads them through the model, so a `said` is rendered in this language and never its English beside it.
  const chat = withoutComments(read("views/_studio/Chat.tsx"));
  assert.ok(chat.includes("<Markdown text={contentOf(message)}"), "the row's words are contentOf's");
  assert.ok(chat.includes("contentOf(replyTo)"), "the reply-to strip reads the same");
  assert.ok(!chat.includes("message.content"), "nothing draws the content past the model");
  assert.ok(!chat.includes("tx("), "no second rendering of the said beside the words");
});

test("the door, the boot, the header and the test preload are wired", () => {
  const door = read("i18n/l10n.mjs");
  assert.ok(door.includes("useIsolating: false"), "no isolation marks around a placeable");
  assert.ok(door.includes("export function t(") && door.includes("export function attr(") && door.includes("export function tx("), "the three ways to say a word");
  const main = read("main.tsx");
  assert.ok(main.indexOf("bootLocale()") < main.indexOf("createRoot("), "the English catalog is installed before the first render");
  assert.ok(main.indexOf('from "./i18n/boot"') < main.indexOf('from "react"'), "the language boots before any other module says a word");
  const html = readRoot("desktop/index.html");
  assert.ok(html.includes('get("locale")') && html.includes('"lang"'), "the stored language stamps <html lang> before the first paint");
  const api = read("api.ts");
  assert.ok(api.includes('"accept-language": locale()'), "every request names the language it wants");
  const app = read("App.tsx");
  assert.ok(app.split("syncLocaleFromNode()").length === 4, "the setting is followed at boot, on settings_changed and when the node comes back after it was away");
  const pkg = JSON.parse(readRoot("desktop/package.json"));
  assert.ok(pkg.scripts.test.includes("--import ./src/i18n/preload.mjs"), "npm test speaks English");
  assert.ok(pkg.scripts["test:coverage"].includes("--import ./src/i18n/preload.mjs"));
  assert.ok("@fluent/bundle" in pkg.dependencies && "@fluent/langneg" in pkg.dependencies);
  assert.ok(readRoot("scripts/test").includes("--import ./src/i18n/preload.mjs"), "scripts/test desktop speaks English");
  assert.ok(readRoot("desktop/vite.config.ts").includes('"../locales"'), "the dev server may serve the catalog");
  assert.ok(readRoot("Justfile").includes("i18n-baseline:"), "the baselines have one recipe");
  // The Tauri shell holds no catalog: the webview pushes the menus' words once, and the shell sets them on its items.
  assert.ok(app.includes("void pushShellWords();"), "the shell's menu words are pushed at boot");
  assert.ok(read("shell/shellWords.ts").includes('invoke("shell_words"'), "through the one command");
  const shell = readRoot("desktop/src-tauri/src/main.rs");
  assert.ok(shell.includes("fn shell_words(") && shell.includes("shell_words,"), "which the shell registers");
  assert.ok(readRoot("desktop/src-tauri/src/tray/mod.rs").includes("pub fn say(&self, words: &ShellWords)"), "and the menu bar icon's lines take");
  assert.ok(readRoot("desktop/src-tauri/src/edit_menu.rs").includes("pub fn say(&self, words: &crate::words::ShellWords)"), "as the Edit menu's verbs do");
  const store = read("i18n/localeStore.ts");
  assert.ok(store.includes("LOCALE_KEY") && store.includes("root.lang = locale") && store.includes("root.dir = textDirection(locale)"));
});
