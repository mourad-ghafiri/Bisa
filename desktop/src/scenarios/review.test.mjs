/**
 * A person's afternoon reviewing an agent's edits in the Project IDE (ide/09
 * §Modes and the change ledger), as the models see it: an edit arrives and
 * the card, the pane bar and the file's dot all read it; undoing one hunk and
 * keeping the rest; switching to `plan` and building it back; and `auto`'s
 * own promise that a pending change is kept when the next message is sent. A
 * scenario steps the models the way the components do — no DOM, no fetch.
 *
 * Run with `node --test desktop/src/scenarios/review.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { BUILD_MESSAGE, MODE_ICON, isCheckoutOrigin, modeOf, modeChoices, nextMode, planBannerWords } from "../views/_studio/conversationModeModel.mjs";
import {
  autoKeptHint,
  fileChipTone,
  fileChipWords,
  fileVerbs,
  pendingPathsOf,
  skippedWords,
  turnCardsByAnchor,
  turnWords,
} from "../views/_studio/turnChangesModel.mjs";
import { changedFileRows, changedFilesHeaderWords } from "../views/_studio/changedFilesModel.mjs";
import { composerButton } from "../ui/composerButtonModel.mjs";
import {
  DEFAULT_LENS_LAYOUT,
  fileSettleTarget,
  hasNextHunk,
  hasPreviousHunk,
  hunkPositionWords,
  hunkSettleTarget,
  lensStorageKey,
  nextHunkIndex,
  previousHunkIndex,
  reviewOpenDrafts,
  toggleLensLayout,
  undoEnabled,
} from "../views/_workbench/reviewLensModel.mjs";
import { docModeKey } from "../views/_workbench/fileDocModel.mjs";
import { allowBody, denyBody, scopesFor, tierWords } from "../views/_studio/conversationAskModel.mjs";
import { reloadOnReconnect } from "../shell/workspaceLoadModel.mjs";
import { rootKey } from "../views/_workbench/workbenchModel.mjs";

const WID = "01JSCENARIOWORKSTREAM000000";
const CONVERSATION = "01JSCENARIOCONVERSATION0000";
const ROOT = rootKey("workstream", WID);
const SRC = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(SRC, rel), "utf8");

function file(over = {}) {
  return { path: "src/header.tsx", kind: "modified", state: "pending", opaque: false, overlapped: false, added: 2, removed: 1, ...over };
}

/** What the node answers for a conversation's changes; the mode is the conversation's, `manual` unless said. */
function view(turns, mode = "manual") {
  return { conversation: CONVERSATION, workstream: WID, mode, pending: turns.flatMap((t) => t.files).filter((f) => f.state === "pending").length, owed: true, turns };
}

test("a conversation about a checkout carries a mode, and every mode has a glyph and a meaning", () => {
  assert.equal(isCheckoutOrigin("workstream"), true);
  assert.equal(isCheckoutOrigin("goal"), false);
  assert.equal(modeOf({}), "manual");
  for (const s of modeChoices(true)) assert.ok(MODE_ICON[s.id]);
});

test("an edit arrives: the turn's card, the pane bar's count, and the file's dot all read the same ledger", () => {
  const turn = { turn: "t1", prompt: "m1", reply: "m2", agent: "reviewer", mode: "manual", files: [file()] };
  const changes = view([turn]);

  // The card, keyed to the reply.
  const { byMessage, atEnd } = turnCardsByAnchor(changes);
  assert.equal(atEnd.length, 0);
  const card = byMessage.get("m2")[0];
  assert.equal(card.words, turnWords([file()]));
  assert.equal(fileChipWords(card.files[0]), "to review");
  assert.equal(fileChipTone(card.files[0]), "accent");
  assert.deepEqual(fileVerbs(card.files[0]), ["keep", "undo", "undo_with_note"]);

  // The bar above the composer: the same rows, said as a summary.
  assert.deepEqual(changedFileRows(changes).map((f) => f.path), ["src/header.tsx"]);
  assert.equal(changedFilesHeaderWords(changes, (id) => (id === "reviewer" ? "Reviewer" : id)), "1 file changed by Reviewer · +2 −1");

  // The dot: the set of paths a tab or an explorer row reads.
  assert.deepEqual([...pendingPathsOf(changes)], ["src/header.tsx"]);
});

test("undo one hunk, then keep the rest — the review lens's own navigation and settle targets", () => {
  const hunks = [
    { id: "h1", base: { start: 0, len: 2 }, disk: { start: 0, len: 2 }, removed: "old top\n", added: "new top\n" },
    { id: "h2", base: { start: 10, len: 1 }, disk: { start: 10, len: 1 }, removed: "old bottom\n", added: "new bottom\n" },
  ];
  const fileReview = { path: "src/header.tsx", kind: "modified", opaque: false, overlapped: false, base_text: "…", disk_hash: "sha:1", hunks };

  // The lens opens on the first change.
  let index = 0;
  assert.equal(hunkPositionWords(hunks, index), "Change 1 of 2");
  assert.equal(hasPreviousHunk(hunks, index), false);
  assert.equal(hasNextHunk(hunks, index), true);

  // Undo the first hunk — a clean buffer, so it is enabled — with the file's own disk hash.
  assert.equal(undoEnabled(false), true);
  const undoTarget = hunkSettleTarget(fileReview.path, hunks[0], fileReview.disk_hash);
  assert.deepEqual(undoTarget, { grain: "hunk", path: "src/header.tsx", hunk: "h1", disk_hash: "sha:1" });

  // A dirty buffer disables the second hunk's Undo — the server undoes against the disk.
  assert.equal(undoEnabled(true), false);

  // Step to the second change and keep the rest of the file at once.
  index = nextHunkIndex(hunks, index);
  assert.equal(hunkPositionWords(hunks, index), "Change 2 of 2");
  assert.equal(hasNextHunk(hunks, index), false);
  const keepRestTarget = fileSettleTarget(fileReview.path);
  assert.deepEqual(keepRestTarget, { grain: "file", path: "src/header.tsx" });

  // Stepping back is still possible.
  assert.equal(previousHunkIndex(hunks, index), 0);

  // The server settled: the file is no longer pending. The card and the dot agree.
  const settled = view([{ turn: "t1", prompt: "m1", reply: "m2", agent: "reviewer", mode: "manual", files: [file({ state: "kept" })] }]);
  assert.deepEqual([...pendingPathsOf(settled)], []);
  assert.equal(changedFilesHeaderWords(settled), "", "nothing pending: the bar has nothing to say");

  // A settle can skip a file that moved since — the toast's own words.
  assert.equal(skippedWords([{ path: "src/other.ts", why: "changed since" }]), "src/other.ts — changed since");
});

test("switching to plan, then building it back", () => {
  assert.equal(nextMode("manual"), "auto");
  assert.equal(nextMode("auto"), "plan");
  // The picker disables plan for an unguarded harness rather than switching into a dead end.
  const unguarded = modeChoices(false).find((s) => s.id === "plan");
  assert.equal(unguarded.disabled, true);
  // Guarded, the switch to plan is offered, and the banner it earns once a
  // reply lands says the same two things every time.
  const guarded = modeChoices(true).find((s) => s.id === "plan");
  assert.equal(guarded.disabled, false);
  const words = planBannerWords();
  assert.equal(words.build, "Build this plan");
  assert.equal(BUILD_MESSAGE, "Build this plan.");
  // Building is the node's one act — the record remembers the mode the plan
  // was entered from — and the words are posted after it answered; the pane
  // keeps no mode of its own, so a reload between the two changes nothing.
  const pane = read("views/_workbench/useConversationPane.tsx");
  const build = pane.slice(pane.indexOf("const buildPlan = async"), pane.indexOf("/** A turn's card"));
  assert.ok(build.includes("await api.buildPlan(current.id);"), "the node's act");
  assert.ok(build.indexOf("api.buildPlan(") < build.indexOf("api.postConversationMessage(current.id, { content: BUILD_MESSAGE })"), "then the words");
  assert.ok(!build.includes("patchConversation"), "never a mode this side chose");
  assert.ok(!/BeforePlan/.test(pane), "no screen remembers the mode before the plan");
  const api = read("api.ts");
  assert.ok(api.includes("post<{ conversation: ConversationView }>(`/conversations/${id}/plan/build`, undefined, s)"), "POST /conversations/{id}/plan/build, no body");
});

test("auto keeps a pending change on the next message, without a click", () => {
  const changes = view([{ turn: "t1", prompt: "m1", reply: "m2", agent: "reviewer", mode: "auto", files: [file()] }], "auto");
  assert.equal(changes.mode, "auto");
  assert.equal(changes.turns[0].mode, "auto", "the turn remembers the mode it ran in, whatever the conversation is switched to after");
  assert.match(autoKeptHint(), /next message/);
  // Still restorable until then — the pending count says so.
  assert.equal(changes.pending, 1);
});

test("the review lens remembers its on/off choice per document, like the doc mode", () => {
  assert.equal(lensStorageKey(ROOT, "src/header.tsx"), `${ROOT}|review-lens|src/header.tsx`);
});

test("an ask beside the changes: allow once, allow for the conversation only when grantable, or deny", () => {
  const grantable = { grantable: true, tier: "exec" };
  assert.deepEqual(scopesFor(grantable), ["once", "conversation"]);
  assert.equal(tierWords(grantable.tier), "a command");
  assert.deepEqual(allowBody(grantable, "conversation"), { answer: "allow", scope: "conversation" });
  const notGrantable = { grantable: false, tier: "write" };
  assert.deepEqual(scopesFor(notGrantable), ["once"]);
  assert.deepEqual(allowBody(notGrantable, "conversation"), { answer: "allow", scope: "once" }, "never sent for an ask that cannot grant it");
});

test("a node restart takes its asks with it: the card is read away when the bus comes back, not left as a ghost", () => {
  // The node's side: one ask waits, and a restart forgets it — no `ask_settled` is ever sent.
  let waiting = [{ id: "01JASK", agent: "coder", tool: "Bash", tier: "exec", question: "Run `cargo test`?", grantable: true, opened_at: 100 }];
  let shown = waiting;
  let reads = 0;
  const read = () => {
    reads += 1;
    shown = waiting;
  };
  let tell = () => {};
  const stop = reloadOnReconnect((cb) => {
    tell = cb;
    cb("open");
    return () => {
      tell = () => {};
    };
  }, read);

  assert.equal(reads, 0, "a bus that was never away owes no read");
  tell("closed");
  waiting = [];
  tell("connecting");
  assert.equal(shown.length, 1, "while the node is away the card stays: nothing is known yet");
  tell("open");
  assert.deepEqual([reads, shown.length], [1, 0], "back again: the asks are read once, and the card is gone");
  tell("connecting");
  tell("open");
  assert.equal(reads, 1, "a reconnect that never closed reads nothing twice");
  stop();

  // A note that is only blanks is no note: the agent hears the plain refusal.
  assert.deepEqual(denyBody("   "), { answer: "deny" });
  assert.deepEqual(denyBody(" use the staging key "), { answer: "deny", note: "use the staging key" });
});

test("Review opens the file in Source with the lens on, inline — whatever the document remembered", () => {
  const drafts = reviewOpenDrafts(ROOT, "docs/guide.md");
  assert.equal(drafts.modeKey, docModeKey(ROOT, "docs/guide.md"));
  assert.equal(drafts.mode, "source", "a .md that opens rendered lands on its diff");
  assert.equal(drafts.lensKey, lensStorageKey(ROOT, "docs/guide.md"));
  assert.equal(drafts.lensOn, true);
  assert.equal(DEFAULT_LENS_LAYOUT, "inline");
  assert.equal(toggleLensLayout(DEFAULT_LENS_LAYOUT), "side-by-side");
});

test("the composer's one slot: Send at rest, Stop while a turn works, Stop-shaped between the two", () => {
  assert.equal(composerButton({ busy: false, stop: null, sendable: true, disabled: false }).kind, "send");
  assert.equal(composerButton({ busy: true, stop: null, sendable: true, disabled: false }).kind, "sending");
  assert.equal(composerButton({ busy: false, stop: { onStop() {} }, sendable: true, disabled: false }).kind, "stop");
});

test("the surfaces keep their shape: one trailing button, real buttons for the verbs, an inline lens, no git line in the chat", () => {
  const composer = read("ui/Composer.tsx");
  assert.ok(composer.includes("composerButton({ busy, stop, sendable, disabled })"), "the slot is the model's word");
  assert.ok(!composer.includes("{stop && ("), "no second control beside Send");
  assert.equal((composer.match(/onClick=\{button\.kind === "stop"/g) ?? []).length, 1, "one button, two meanings");
  for (const rel of ["views/_studio/ChangedFilesBar.tsx", "views/_studio/ChangedFileRow.tsx", "views/_studio/TurnChangesCard.tsx"]) {
    const src = read(rel);
    assert.ok(!src.includes("hover:underline"), `${rel}: a verb is a button, not a link`);
    assert.ok(src.includes("<Button"), `${rel}: draws the kit's Button`);
  }
  const bar = read("views/_studio/ChangedFilesBar.tsx");
  assert.ok(bar.includes('variant="primary"') && bar.includes('variant="danger"'), "Keep all is the primary, Undo all the danger");
  assert.ok(bar.includes("aria-expanded"), "the header discloses the rows");
  const chat = read("views/_studio/Chat.tsx");
  assert.ok(!chat.includes("files changed in this checkout") && !chat.includes("Keep all"), "the chat draws no git line and no verb of its own — the bar's");
  assert.ok(chat.includes("<ChangedFilesBar"), "the bar is the one strip");
  const editor = read("views/_workbench/EditorDoc.tsx");
  assert.ok(editor.includes('inline={lensLayout === "inline"}') && editor.includes('foldUnchanged={lensLayout === "inline"}'), "the lens reads inline and folded by default");
  assert.ok(editor.includes("ReviewPendingBanner"), "a rendered document says its diff waits in Source");
  const pane = read("views/_workbench/useConversationPane.tsx");
  assert.ok(pane.includes("reviewOpenDrafts(") && pane.includes("writeSessionDraft("), "Review writes the document's drafts before opening it");
  const card = read("views/_studio/TurnChangesCard.tsx");
  assert.ok(card.includes("useFileSettle(") && bar.includes("useFileSettle("), "one owner of how a change is settled");
});
