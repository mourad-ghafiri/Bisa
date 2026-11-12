/**
 * A team's roster, edited from more than one place.
 *
 * The Teams screen's pane used to keep a read of its own. An edit made in
 * the dialog left it behind, and *Remove* then wrote the roster the pane
 * still showed — the edit undone, by a button that said nothing of the
 * kind. These are the afternoons that went wrong, told through the rules
 * the screen now draws by (`rosterModel.mjs`, `heldAnswerModel.mjs`).
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { asksDelete, definitionRemoved, shownTeam, storedMembers, teamMoved, withoutMember, KIND_TEAM } from "../views/rosterModel.mjs";
import { heldFor, holding } from "../views/_work/heldAnswerModel.mjs";

const core = [{ agent: "general-agent" }, { agent: "workflow-agent" }];
/** The node's list answer: the stored roster, then the two agents nobody stored. */
const listed = (name, stored) => ({ id: "crew", name, members: [...stored, ...core] });
/** The node's answer to a PATCH: the record as stored. */
const patched = (team, body) => ({ ...team, ...body, members: body.members ?? storedMembers(team) });

test("an edit, then a remove: the edit stays", () => {
  // The pane is open on the team as the list read it.
  let list = listed("Crew", [{ agent: "coder" }, { agent: "mapper" }]);
  let held = null;
  const drawn = () => shownTeam(list, heldFor(list, held));
  assert.equal(drawn().name, "Crew");

  // The dialog renames it and adds a reviewer; the list is read again on the team's frame.
  assert.equal(teamMoved({ scope: "crew", kind: KIND_TEAM, snapshot: true }), true);
  list = listed("Deck crew", [{ agent: "coder" }, { agent: "mapper" }, { agent: "reviewer" }]);
  assert.equal(drawn().name, "Deck crew", "the pane draws the list's fresh copy");

  // Remove the mapper: the roster written is the one on screen, less that one.
  const body = withoutMember(drawn(), { agent: "mapper" });
  assert.deepEqual(body, { members: [{ agent: "coder" }, { agent: "reviewer" }] }, "the reviewer the edit added is kept");
  held = holding(list, patched(drawn(), body));
  assert.equal(drawn().name, "Deck crew", "and so is the name");
  assert.deepEqual(storedMembers(drawn()), [{ agent: "coder" }, { agent: "reviewer" }]);
  assert.equal(drawn().members.length, 4, "the two core agents are still drawn, after the stored ones");
});

test("two removes in a row, the second before the list is read again: the first one stays removed", () => {
  const list = listed("Crew", [{ agent: "coder" }, { agent: "mapper" }, { human: "ab".repeat(32) }]);
  let held = null;
  const drawn = () => shownTeam(list, heldFor(list, held));

  const first = withoutMember(drawn(), { agent: "coder" });
  held = holding(list, patched(drawn(), first));
  // The list has not been read again — `list` is the same list — and the second remove is pressed.
  const second = withoutMember(drawn(), { agent: "mapper" });
  assert.deepEqual(second, { members: [{ human: "ab".repeat(32) }] }, "built from what the node answered: the coder is not put back");
  // From the list's own copy it would have been.
  assert.deepEqual(withoutMember(list, { agent: "mapper" }).members, [{ agent: "coder" }, { human: "ab".repeat(32) }]);

  // The list is read again: the read is the truth, and the held answer is nothing.
  held = holding(list, patched(drawn(), second));
  const reread = listed("Crew", [{ human: "ab".repeat(32) }]);
  assert.equal(heldFor(reread, held), null);
  assert.deepEqual(storedMembers(shownTeam(reread, heldFor(reread, held))), [{ human: "ab".repeat(32) }]);
});

test("a team removed elsewhere while its delete dialog stood open: the dialog goes with it, and the next team opens none", () => {
  // Delete was asked of the crew; at the prompt somebody removed it first.
  const asked = "crew";
  assert.equal(asksDelete(asked, { id: "crew" }), true);
  const frame = { scope: "crew", kind: KIND_TEAM, snapshot: true, removed: true };
  assert.equal(definitionRemoved(frame) && teamMoved(frame), true, "the list is read again and no longer holds it");
  assert.equal(asksDelete(asked, null), false, "no record in hand: no dialog");
  // The next team picked is not asked to be deleted by a yes left behind.
  assert.equal(asksDelete(asked, { id: "ops" }), false);
});
