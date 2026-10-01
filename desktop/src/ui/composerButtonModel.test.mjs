/**
 * The composer's one slot: Send, or Stop — never both, never neither — and
 * the handover between them. Run with `node --test desktop/src/ui/composerButtonModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { SEND_HANDOVER_GRACE_MS, composerButton, handoverElapsed } from "./composerButtonModel.mjs";

test("a stoppable session is Stop, whatever else is true", () => {
  for (const busy of [false, true]) {
    for (const sendable of [false, true]) {
      const b = composerButton({ busy, stop: { onStop() {} }, sendable, disabled: false });
      assert.equal(b.kind, "stop");
      assert.equal(b.label, "Stop");
      assert.equal(b.enabled, true, "Stop is always pressable");
    }
  }
  assert.equal(composerButton({ busy: false, stop: { label: "Stop Reviewer" }, sendable: true, disabled: false }).label, "Stop Reviewer");
});

test("posting is Stop-shaped and disabled — no frame shows Send between the post and the roster", () => {
  const b = composerButton({ busy: true, stop: null, sendable: true, disabled: false });
  assert.equal(b.kind, "sending");
  assert.equal(b.label, "Stop");
  assert.equal(b.enabled, false);
});

test("at rest the slot is Send, enabled only with something to send and sending allowed", () => {
  assert.deepEqual(composerButton({ busy: false, stop: null, sendable: true, disabled: false }), { kind: "send", label: "Send", hint: "Send (Enter · ⌘Enter)", enabled: true });
  const empty = composerButton({ busy: false, stop: null, sendable: false, disabled: false });
  assert.equal(empty.kind, "send");
  assert.equal(empty.enabled, false);
  assert.match(empty.hint, /remove a chip/);
  assert.equal(composerButton({ busy: false, stop: null, sendable: true, disabled: true }).enabled, false);
});

test("the handover lapses after the grace, never before, and never while posting", () => {
  assert.equal(handoverElapsed(null, 10_000), false, "nothing settled yet");
  assert.equal(handoverElapsed(1000, 1000), false);
  assert.equal(handoverElapsed(1000, 1000 + SEND_HANDOVER_GRACE_MS - 1), false);
  assert.equal(handoverElapsed(1000, 1000 + SEND_HANDOVER_GRACE_MS), true);
  assert.equal(handoverElapsed(1000, 1000 + 50, 50), true, "the grace is a parameter");
  assert.ok(SEND_HANDOVER_GRACE_MS >= 500 && SEND_HANDOVER_GRACE_MS <= 3000, "long enough for a roster frame, short enough to notice");
});
