import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { dirty, mayClose } from "./agentDraftModel.mjs";
import { emptyTeamDraft, maySaveTeam, teamDraftOf, teamSaveOf } from "./teamDraftModel.mjs";

const person = "ab".repeat(32);
const crew = {
  id: "crew",
  name: "Crew",
  purpose: "ships",
  photo: null,
  members: [{ agent: "coder" }, { human: person }, { agent: "general-agent" }, { agent: "workflow-agent" }],
  tags: ["delivery"],
};

test("a team's draft holds what somebody stored, never a core agent", () => {
  assert.deepEqual(teamDraftOf(crew), { name: "Crew", purpose: "ships", photo: null, members: ["agent:coder", `human:${person}`], tags: ["delivery"] });
  assert.deepEqual(teamDraftOf({ name: "Bare", members: [] }), { name: "Bare", purpose: "", photo: null, members: [], tags: [] });
  assert.deepEqual(emptyTeamDraft(), { name: "", purpose: "", photo: null, members: [], tags: [] });
  assert.equal(maySaveTeam({ name: "  " }, false), false);
  assert.equal(maySaveTeam({ name: "Crew" }, true), false, "no second save while one is out");
  assert.equal(maySaveTeam({ name: "Crew" }, false), true);
});

test("an edit names what changed, so a dialog left open never writes back what it only read", () => {
  const seeded = teamDraftOf(crew);
  // Renamed, and nothing else touched: the roster is not sent — a member removed elsewhere meanwhile stays removed.
  assert.deepEqual(teamSaveOf({ ...seeded, name: "Deck crew " }, crew, seeded), { kind: "patch", id: "crew", body: { name: "Deck crew" } });
  // The roster changed: sent whole, in order — and the purpose cleared is `null`.
  const edited = { ...seeded, purpose: "  ", members: [`human:${person}`, "agent:reviewer"] };
  assert.deepEqual(teamSaveOf(edited, crew, seeded), { kind: "patch", id: "crew", body: { purpose: null, members: [{ human: person }, { agent: "reviewer" }] } });
  // Nothing changed: nothing named.
  assert.deepEqual(teamSaveOf(seeded, crew, seeded), { kind: "patch", id: "crew", body: {} });
  // With nothing to compare with, the whole of what the dialog owns.
  assert.deepEqual(Object.keys(teamSaveOf(seeded, crew).body), ["name", "purpose", "photo", "members", "tags"]);
  // A team inside a team — a hand-edited record — is sent as it stands: the node's refusal names it.
  const nested = { ...seeded, members: [...seeded.members, "team:ops"] };
  assert.deepEqual(teamSaveOf(nested, crew, seeded).body.members.at(-1), { team: "ops" });
});

test("a new team says what it has, and no team inside it", () => {
  const draft = { name: " Launch ", purpose: "", photo: null, members: ["agent:coder", "team:ops"], tags: [] };
  assert.deepEqual(teamSaveOf(draft, null), { kind: "create", body: { name: "Launch", members: [{ agent: "coder" }], tags: [] } });
  const pictured = { ...draft, purpose: "ship it", photo: { sha256: "a" }, members: [] };
  assert.deepEqual(teamSaveOf(pictured, null).body, { name: "Launch", purpose: "ship it", photo: { sha256: "a" }, members: [], tags: [] });
});

test("the team dialog guards its edits as the agent editor does, and draws from this model", () => {
  const seeded = teamDraftOf(crew);
  assert.equal(dirty(seeded, teamDraftOf(crew)), false);
  assert.equal(dirty({ ...seeded, tags: [] }, seeded), true);
  assert.equal(mayClose({ dirty: true, busy: false }), "ask");
  const screen = readFileSync(new URL("../Teams.tsx", import.meta.url), "utf8");
  assert.ok(screen.includes("const save = teamSaveOf(d, team, seeded.current);"), "the save body is the model's");
  assert.ok(screen.includes("const leave = mayClose({ dirty: dirty(d, seeded.current), busy });"));
  assert.ok(screen.includes('<fieldset disabled={busy} className="m-0 min-w-0 border-0 p-0">'), "nothing is typed into a save that is out");
});
