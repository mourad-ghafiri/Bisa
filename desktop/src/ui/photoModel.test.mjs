/**
 * A project's or a group's photo as facts. Run with
 * `node --test desktop/src/ui/photoModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { PHOTO_EDGE, PHOTO_MIME, PHOTO_PROFILES, PHOTO_TYPES, THUMB_CACHE_MAX, THUMB_EDGE, coverCrop, evictOrder, fitsProfile, isPhotoType, photoName, photoOfPrincipal, photoRefusal, thumbKey } from "./photoModel.mjs";

test("the edges are what is kept and what is drawn, and the encoding is PNG", () => {
  assert.equal(PHOTO_EDGE, 256, "crisp at 32 px on a 3× display");
  assert.equal(THUMB_EDGE, 64, "the largest draw is 32 px; a 2× display wants 64");
  assert.equal(PHOTO_MIME, "image/png");
  assert.equal(THUMB_CACHE_MAX, 200);
  assert.ok(isPhotoType("image/jpeg") && isPhotoType("IMAGE/PNG") && isPhotoType("image/webp") && isPhotoType("image/gif"));
  assert.ok(!isPhotoType("image/svg+xml") && !isPhotoType("text/plain") && !isPhotoType(null));
});

test("the cover-crop is the centred square of the source, drawn at the edge and never scaled up", () => {
  assert.deepEqual(coverCrop(4000, 3000, 256), { sx: 500, sy: 0, sw: 3000, sh: 3000, dw: 256, dh: 256 }, "a landscape loses its sides");
  assert.deepEqual(coverCrop(3000, 4000, 256), { sx: 0, sy: 500, sw: 3000, sh: 3000, dw: 256, dh: 256 }, "a portrait loses top and bottom");
  assert.deepEqual(coverCrop(256, 256, 256), { sx: 0, sy: 0, sw: 256, sh: 256, dw: 256, dh: 256 });
  assert.deepEqual(coverCrop(40, 60, 256), { sx: 0, sy: 10, sw: 40, sh: 40, dw: 40, dh: 40 }, "a small picture keeps its own size");
  assert.deepEqual(coverCrop(301, 200, 64), { sx: 50, sy: 0, sw: 200, sh: 200, dw: 64, dh: 64 }, "an odd margin rounds down");
  assert.deepEqual(coverCrop(0, 100, 256), { sx: 0, sy: 0, sw: 0, sh: 0, dw: 0, dh: 0 }, "nothing to crop");
  assert.deepEqual(coverCrop(100, 100, 0), { sx: 0, sy: 0, sw: 0, sh: 0, dw: 0, dh: 0 });
});

test("two profiles: a picture is a 256 px PNG kept on this machine, a face a 96 px JPEG small enough to travel", () => {
  assert.deepEqual(Object.keys(PHOTO_PROFILES), ["picture", "face"]);
  assert.equal(PHOTO_PROFILES.picture.edge, PHOTO_EDGE);
  assert.equal(PHOTO_PROFILES.picture.mime, PHOTO_MIME);
  assert.equal(PHOTO_PROFILES.picture.maxBytes, 512 * 1024, "the node's MAX_PHOTO_BYTES");
  assert.equal(PHOTO_PROFILES.face.edge, 96);
  assert.equal(PHOTO_PROFILES.face.mime, "image/jpeg", "a face must fit; PNG at 96 px does not reliably");
  assert.equal(PHOTO_PROFILES.face.maxBytes, 16 * 1024, "the node's MAX_FACE_BYTES — what rides the control channel");
  assert.deepEqual([...PHOTO_PROFILES.face.qualities], [0.85, 0.7, 0.5], "tried in turn until it fits");
  assert.deepEqual(PHOTO_PROFILES.picture.qualities, [], "a PNG has no quality to lower");
  assert.ok(fitsProfile(16 * 1024, PHOTO_PROFILES.face) && !fitsProfile(16 * 1024 + 1, PHOTO_PROFILES.face));
  assert.deepEqual([...PHOTO_TYPES], ["image/png", "image/jpeg", "image/gif", "image/webp"], "what the pickers accept");
  assert.match(photoRefusal("size", PHOTO_PROFILES.face), /face is still over 16 KB/);
  assert.match(photoRefusal("size"), /picture is still over 512 KB/, "the picture profile is the default");
});

test("a scaled photo is named after the picked file, as the photo it became, in its profile's encoding", () => {
  assert.equal(photoName("logo.jpg"), "logo-photo.png");
  assert.equal(photoName("me.png", PHOTO_PROFILES.face), "me-face.jpg", "a face is a JPEG, marked as a face");
  assert.equal(photoName("", PHOTO_PROFILES.face), "face-face.jpg");
  assert.equal(photoName("IMG_4821.HEIC.jpeg"), "IMG_4821.HEIC-photo.png", "only the last extension goes");
  assert.equal(photoName("/Users/me/Pictures/team photo.png"), "team photo-photo.png", "the file's own name, never its path");
  assert.equal(photoName(".hidden"), ".hidden-photo.png", "a leading dot is not an extension");
  assert.equal(photoName(""), "photo-photo.png");
  assert.equal(photoName(null), "photo-photo.png");
});

test("the refusals say why, the cache key carries the edge, and eviction lets the least recently drawn go first", () => {
  assert.match(photoRefusal("type"), /not a picture/);
  assert.match(photoRefusal("decode"), /could not be read/);
  assert.match(photoRefusal("encode"), /could not be scaled/);
  assert.equal(thumbKey("abc", 64), "abc@64");
  const entries = [
    { key: "a", at: 30 },
    { key: "b", at: 10 },
    { key: "c", at: 20 },
    { key: "d", at: 10 },
  ];
  assert.deepEqual(evictOrder(entries, 2), ["b", "d"], "oldest first; a tie by key");
  assert.deepEqual(evictOrder(entries, 4), [], "fits: nothing goes");
  assert.deepEqual(evictOrder(entries, 0), ["b", "d", "c", "a"]);
  assert.deepEqual(evictOrder([], 3), []);
});

test("a principal's face is their row's — the owner's included — else an agent's, else none", () => {
  const face = { sha256: "f".repeat(64) };
  const picture = { sha256: "a".repeat(64) };
  const members = [{ pubkey: "owner", photo: face }, { pubkey: "bob", photo: null }];
  const agents = [{ pubkey: "scout", photo: picture }, { pubkey: "plain" }];
  assert.equal(photoOfPrincipal(members, agents, "owner"), face);
  assert.equal(photoOfPrincipal(members, agents, "scout"), picture);
  assert.equal(photoOfPrincipal(members, agents, "bob"), null, "a member without a face");
  assert.equal(photoOfPrincipal(members, agents, "plain"), null);
  assert.equal(photoOfPrincipal(members, agents, "stranger"), null);
  assert.equal(photoOfPrincipal(null, null, "owner"), null);
});
