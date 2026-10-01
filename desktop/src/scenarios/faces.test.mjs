/**
 * A face for everyone (ide/14 §Photos, 14-collaboration), as the sources show
 * it: one photo pipeline with two profiles, one picker, every principal drawn
 * with the face it wears, and the wire carrying a member's face by reference
 * with the bytes on demand. A source assertion, as `sidebar.test.mjs` makes
 * them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/faces.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const root = new URL("..", import.meta.url).pathname;

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx|mjs|mts)$/.test(p)) out.push(p);
  }
  return out;
}

test("the URL string is gone: an agent, a team, a member and the owner carry a photo by content hash, and one picker sets them all", () => {
  for (const p of walk(root)) {
    if (p.endsWith("types.gen.ts") || p.endsWith("faces.test.mjs")) continue;
    assert.ok(!readFileSync(p, "utf8").includes("avatar_url"), `${p} still speaks of avatar_url`);
  }
  const gen = src("../types.gen.ts");
  for (const shape of ["export interface WorkspaceMember", "export interface Directory", "export interface Agent ", "export interface Team ", "export interface MeBody"]) {
    assert.ok(gen.includes(shape), `${shape} is generated`);
  }
  assert.ok(!gen.includes("avatar_url"), "the generated types carry no URL string");
  const field = src("../ui/PhotoField.tsx");
  assert.ok(field.includes("scalePhoto(file, profile)") && field.includes("api.uploadAttachment(scaled)"), "the field scales then uploads");
  for (const editor of ["../views/_work/AgentEditor.tsx", "../views/Teams.tsx", "../views/_settings/IdentityPanel.tsx", "../views/_work/ProjectDetail.tsx"]) {
    assert.ok(src(editor).includes("<PhotoField"), `${editor} picks through the one field`);
  }
  assert.ok(src("../views/_settings/IdentityPanel.tsx").includes("profile={PHOTO_PROFILES.face}"), "a person's photo is a face");
  assert.ok(!src("../views/_work/ProjectDetail.tsx").includes('type="file"'), "no hand-rolled picker is left in the project dialog");
});

test("every principal is drawn with the face it wears, through the one lookup", () => {
  const store = src("../shell/useWorkspaceData.ts");
  assert.ok(store.includes("photoOf: (pubkey: string) => AttachmentRef | null;") && store.includes("photoOfPrincipal(info?.members ?? [], agents, pubkey)"), "the store answers a principal's face");
  for (const [file, fact] of [
    ["../shell/ProfileMenu.tsx", "photo={ws.photoOf(ws.me)}"],
    ["../views/_studio/Chat.tsx", "photo={agent?.photo ?? photo}"],
    ["../views/_studio/Chat.tsx", "hostedPhotoOf(hosted.directory, p) : ws.photoOf"],
    ["../shell/Sidebar.tsx", "photo={ws.photoOf(head)}"],
    ["../views/_settings/PeoplePanel.tsx", "photo={p.photo}"],
    ["../views/Hosted.tsx", "hostedPhotoOf(directory, others[0]!)"],
    ["../views/Agents.tsx", "photo={a.photo}"],
    ["../views/Teams.tsx", "photo={t.photo}"],
    ["../views/_pulse/PulseRow.tsx", "photo={ws.photoOf(item.author)}"],
    ["../ui/AgentPicker.tsx", "photo={c.photo}"],
    ["../views/_work/AssigneePicker.tsx", "photo: m.photo,"],
  ]) {
    assert.ok(src(file).includes(fact), `${file} draws the face: ${fact}`);
  }
  const avatar = src("../ui/Avatar.tsx");
  assert.ok(avatar.includes("usePhotoThumb(photo?.sha256 ?? null)") && avatar.includes("const src = url ?? thumb;"), "the avatar draws a photo's shared thumbnail");
});

test("a member's face travels: the directory names it by hash, the bytes come once on demand, and the owner's row is a directory row too", () => {
  const control = src("../../../crates/bisa-collab/src/control.rs");
  for (const fact of ["pub struct Face {", "Profile {", "WantFace { sha256: String },", "Face { face: Face },", "pub photo: Option<AttachmentRef>,", "MAX_FACE_B64_CHARS"]) {
    assert.ok(control.includes(fact), `the wire ${fact}`);
  }
  assert.ok(!control.includes("photo: Option<Face>"), "a join carries no face: a stranger's join decodes nothing");
  const host = src("../../../crates/bisa-net/src/host.rs");
  assert.ok(host.includes("Control::Profile { label, photo, face } =>") && host.includes("PeopleChange::ProfileChanged => send_directory(inner, None).await"), "the host keeps a profile and tells everyone");
  assert.ok(host.includes("photo: m.photo,"), "the directory carries every row's face — the owner's too");
  const guest = src("../../../crates/bisa-guest/src/session.rs");
  assert.ok(guest.includes("replies.push(self.profile_control());") && guest.includes("faces_wanted(members)") && guest.includes("Control::Face { face } => match face.decode()"), "a guest says who it is after the welcome and asks for the faces it lacks");
  assert.ok(src("../../../crates/bisa-guest/src/faces.rs").includes("pub trait FaceStore"), "the guest keeps faces through a port");
  const pump = src("../../../crates/bisa-cli/src/net.rs");
  assert.ok(pump.includes("impl FaceStore for WorkspaceFaces") && pump.includes("pump.guests.announce_profile().await"), "the node's pump is the port, and carries the owner's profile out");
  const core = src("../../../crates/bisa-core/src/attachment.rs");
  assert.ok(core.includes("pub const MAX_FACE_BYTES: u64 = 16 * 1024;"), "a face is bounded for the wire");
  assert.ok(src("../../../crates/bisa-node/src/collab.rs").includes('.route("/workspace/me", put(me))'), "the owner's door");
});
