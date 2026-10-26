/**
 * A resumed harness picks its work up. Run with
 * `node --test desktop/src/terminal/resumeStartModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { RESUME_START_DEFAULT, RESUME_START_KEY, RESUME_START_WORDS, START_MIN_MS, START_QUIET_MS, START_WITHIN_MS, armed, armedDrawn, due, isTyping, nudgeText, onExit, onInput, onOutput, pending, readResumeStart, said } from "./resumeStartModel.mjs";

test("the switch is a registry key, on by default, read from the resolved settings", () => {
  const settings = readFileSync(new URL("../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  assert.ok(settings.includes(`"${RESUME_START_KEY}",`), "the key is in the registry");
  assert.equal(RESUME_START_DEFAULT, true);
  assert.equal(readResumeStart(null), true);
  assert.equal(readResumeStart([]), true);
  assert.equal(readResumeStart([{ key: RESUME_START_KEY, value: false }]), false);
  assert.equal(readResumeStart([{ key: RESUME_START_KEY, value: "no" }]), true, "anything that is not a boolean is the default");
});

test("the nudge is due once the prompt is drawn: output seen, then quiet — never before a beat, never twice", () => {
  let s = armed(1000);
  assert.equal(due(s, 1000 + START_QUIET_MS), false, "nothing seen yet");
  s = onOutput(s, 1100);
  assert.equal(due(s, 1100 + START_QUIET_MS - 1), false, "not quiet long enough");
  assert.equal(due(s, 1100 + START_QUIET_MS), true, "quiet: the prompt is drawn");
  // A burst keeps it waiting.
  s = onOutput(s, 1500);
  assert.equal(due(s, 1500 + START_QUIET_MS - 1), false);
  assert.equal(due(s, 1500 + START_QUIET_MS), true);
  assert.ok(START_QUIET_MS > START_MIN_MS, "quiet already implies the beat when one chunk arrives");
  // Said once, never again.
  s = said(s);
  assert.equal(due(s, 5000), false);
  assert.equal(pending(s, 5000), false);
  assert.equal(nudgeText(), `${RESUME_START_WORDS}\r`, "the words, then Enter");
});

test("the first frame is not the prompt: a beat after the first byte, whatever the quiet", () => {
  // A chunk at 1000, then nothing: due only once both the beat and the quiet have passed.
  const s = onOutput(armed(1000), 1000);
  assert.equal(due(s, 1000 + START_MIN_MS - 1), false);
  assert.equal(due(s, 1000 + Math.max(START_MIN_MS, START_QUIET_MS)), true);
});

test("the person's words go first, an exit ends it, and a harness still signing in is never nudged", () => {
  let s = onOutput(armed(0), 100);
  const typed = onInput(s);
  assert.equal(typed.spent, true);
  assert.equal(due(typed, 5000), false, "the nudge is dropped for good");
  assert.equal(pending(typed, 5000), false);
  assert.equal(onOutput(typed, 6000), typed, "nothing changes a spent nudge");
  const exited = onExit(s);
  assert.equal(due(exited, 5000), false);
  // The window: quiet, but too late.
  s = onOutput(armed(0), START_WITHIN_MS - 100);
  assert.equal(due(s, START_WITHIN_MS + START_QUIET_MS), false, "past the window");
  assert.equal(pending(s, START_WITHIN_MS + 1), false, "and nothing left to wait for");
  assert.equal(pending(armed(0), 1), true);
});

test("words at a prompt already drawn: due after a beat of quiet from arming, never before; output restarts the quiet; the window runs from arming; typing drops them", () => {
  const s = armedDrawn(1000);
  assert.equal(s.firstOutputAt, 1000, "the prompt is taken as drawn now");
  assert.equal(due(s, 1000), false, "not at once");
  assert.equal(due(s, 1000 + START_QUIET_MS - 1), false, "not before the beat");
  assert.equal(due(s, 1000 + START_QUIET_MS), true, "quiet for the beat: due");
  const busy = onOutput(s, 1000 + START_QUIET_MS - 100);
  assert.equal(due(busy, 1000 + START_QUIET_MS), false, "output restarts the quiet");
  assert.equal(due(busy, 1000 + START_QUIET_MS - 100 + START_QUIET_MS), true);
  assert.equal(due(onOutput(s, 1000 + START_WITHIN_MS - 10), 1000 + START_WITHIN_MS + START_QUIET_MS), false, "past the window from arming");
  assert.equal(pending(s, 1000 + START_WITHIN_MS + 1), false);
  assert.equal(due(onInput(s), 1000 + START_QUIET_MS), false, "the person's words go first");
  assert.equal(due(said(s), 1000 + START_QUIET_MS), false, "said once");
});

test("what counts as typing: a key, a paste and a path are; the emulator's own replies — focus in and out, a cursor position, device attributes, a mouse report — are not", () => {
  for (const data of ["a", "\r", "\x1b", "\x03", "\x1b[A", "\x1b[200~two\rlines\x1b[201~", "/Users/me/file.txt", "\x1bOA"]) {
    assert.equal(isTyping(data), true, JSON.stringify(data));
  }
  for (const data of ["\x1b[I", "\x1b[O", "\x1b[12;40R", "\x1b[?1;2c", "\x1b[>0;276;0c", "\x1b[<0;10;20M", "\x1b[<0;10;20m", ""]) {
    assert.equal(isTyping(data), false, JSON.stringify(data));
  }
  assert.equal(isTyping(null), false);
});
