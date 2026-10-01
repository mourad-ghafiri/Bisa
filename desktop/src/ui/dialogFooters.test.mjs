import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";
import { sourceFiles } from "../testWalk.mjs";

/**
 * A dialog's footer holds its actions, and a person reads them in a glance:
 * Cancel, the act, at most one alternative. A fourth button is a question with
 * several answers dressed as a row — its meaning ends up in a paragraph above
 * it, and the row ends up wider than the panel. That is a `ChoiceDialog`: each
 * act on a row of its own, saying what it does.
 *
 * The kit's footer wraps, so a long row is no longer *clipped*
 * (`dialogLayout.test.mjs`); this keeps one from being written at all.
 */
const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, "..");

const MOST = 3;

/** The `footer={…}` expressions of a source, each as its text. */
function footers(text) {
  const out = [];
  for (const m of text.matchAll(/\bfooter=\{/g)) {
    let depth = 1;
    let at = m.index + m[0].length;
    const from = at;
    while (depth > 0 && at < text.length) {
      const c = text[at++];
      if (c === "{") depth++;
      else if (c === "}") depth--;
    }
    out.push({ text: text.slice(from, at - 1), line: text.slice(0, m.index).split("\n").length });
  }
  return out;
}

/**
 * The most buttons one footer shows at a time: a footer that draws one of
 * two fragments (`finished ? <>…</> : <>…</>`) shows one of them, never both.
 */
function mostButtons(footer) {
  const fragments = [...footer.matchAll(/<>([\s\S]*?)<\/>/g)].map((m) => m[1]);
  const count = (s) => (s.match(/<Button\b/g) ?? []).length;
  return Math.max(...(fragments.length > 0 ? fragments : [footer]).map(count));
}

test("no dialog footer outside the kit holds more than three buttons — several acts are a ChoiceDialog", () => {
  const offences = [];
  for (const file of sourceFiles(src, (p) => p.endsWith(".tsx"))) {
    const rel = relative(src, file);
    if (rel.startsWith("ui/")) continue;
    for (const footer of footers(readFileSync(file, "utf8"))) {
      const n = mostButtons(footer.text);
      if (n > MOST) offences.push(`${rel}:${footer.line}: ${n} buttons`);
    }
  }
  assert.deepEqual(offences, [], "offer the acts as the rows of a ChoiceDialog, each with a sentence saying what it does");
});

test("the counting reads a footer the way a person sees it", () => {
  const four = "footer={<><Button/><Button/><Button/><Button/></>}";
  assert.equal(mostButtons(footers(four)[0].text), 4);
  const either = "footer={done ? (<><Button/><Button/></>) : (<><Button/><Button/></>)}";
  assert.equal(mostButtons(footers(either)[0].text), 2, "two fragments are two states, not one row");
  assert.equal(mostButtons(footers("footer={<Button>OK</Button>}")[0].text), 1);
  assert.deepEqual(footers("no footer here"), []);
});
