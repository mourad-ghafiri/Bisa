/**
 * The ratchet sees a sentence where one is — every word a screen draws, a
 * labelled prop, a words table, a template — and not a class list, a log
 * line, a comment, an id said through `t()`, a line marked as not for a
 * person. Run with `node --test desktop/src/i18n/ratchetModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { MARKS, PROSE_KEYS, bareProse, compare, isIdentifierProp, isProse, isProseProp, jsxProse, kindOf, marked, withoutComments } from "./ratchetModel.mjs";

const texts = (src, kind) => bareProse(src, kind).map((f) => f.text);
const drawn = (src) => jsxProse(src).map((f) => f.text);

test("a sentence has a space and letters and starts like one; a class list and an id do not", () => {
  assert.ok(isProse("Nothing needs you"));
  assert.ok(isProse("{x} need you"));
  assert.ok(!isProse("flex items-center gap-2"));
  assert.ok(!isProse("appearance.theme"));
  assert.ok(!isProse("a b"));
  assert.ok(!isProse("  "));
  assert.ok(!isProse("/goals/{x}"));
  assert.ok(PROSE_KEYS.includes("label") && PROSE_KEYS.includes("aria-label") && PROSE_KEYS.includes("placeholder"), "the props a sentence sits in");
});

test("a template of one word beside a placeable is a sentence cut short; one word alone is an identifier", () => {
  for (const cut of ["{x} more", "rule {x}", "goal {x}", "running {x} · {x}", "{x} file{x}", "step {x} {x}"]) assert.ok(isProse(cut), cut);
  assert.ok(!isProse("more"), "no space");
  assert.ok(!isProse("{x} {x}"), "no word at all");
  assert.ok(!isProse("{x} s"), "a unit's letter");
  assert.ok(!isProse("{x} kB") && !isProse("{x} MB"), "a unit");
  assert.ok(!isProse("rounded-control {x}") && !isProse("hover:bg-raised {x}"), "a class list with a placeable");
  assert.ok(isProse("capabilities: {x}") && isProse("failed: {x}"), "a word that ends on a colon is a label, not a class");
});

test("a sentence may open on a separator or a quote, and carry a bracket or a semicolon; code still is not one", () => {
  for (const said of ["· scope {x}", ", … {x} more", "— missing {x}", "@{x} added.", "#{x} updated.", "“{x}” was noted", "Drag it anywhere; it keeps its distance", "Press the new chord (Esc cancels)", "{x}\\n\\n(About {x}.)"]) assert.ok(isProse(said), said);
  for (const code of ["calc({x}% - 3px)", "oklch(0.62 0.13 {x})", "const a = 1; return a", "a && b", "Array<string> of things", "items[0] of things", "fn() then go", "a | b of things", "&amp; then some"]) assert.ok(!isProse(code), code);
});

test("blind spot one — a text node that holds a comma, a semicolon, a bracket, a bar or an equals sign is seen", () => {
  assert.deepEqual(drawn("const a = <p>Drag it anywhere; it keeps its distance.</p>;"), ["Drag it anywhere; it keeps its distance."]);
  assert.deepEqual(drawn("const a = <p>Press the new chord… (Esc cancels)</p>;"), ["Press the new chord… (Esc cancels)"]);
  assert.deepEqual(drawn("const a = <p>Token, secret or key</p>;"), ["Token, secret or key"]);
  assert.deepEqual(drawn("const a = <p>mine | theirs</p>;"), ["mine | theirs"]);
  assert.deepEqual(drawn("const a = <p>width = height</p>;"), ["width = height"]);
});

test("blind spot two — a text node beside an expression is seen, each piece of it, and so is a literal the expression renders", () => {
  assert.deepEqual(drawn("const a = <span>Hosted by {name} ·</span>;"), ["Hosted by"]);
  assert.deepEqual(drawn('const a = <p>@{name} reaches {n} {n === 1 ? "agent" : "agents"}</p>;'), ["reaches", "agent", "agents"]);
  assert.deepEqual(drawn('const a = <span>{n} problem{n === 1 ? "" : "s"}</span>;'), ["problem", "s"], "a plural spelt in code");
  assert.deepEqual(drawn('const a = <span>{label ?? "someone"}</span>;'), ["someone"]);
  assert.deepEqual(drawn('const a = <span>{name || "Untitled slide"}</span>;'), ["Untitled slide"]);
  assert.deepEqual(drawn('const a = <span>{busy && "working"}</span>;'), ["working"]);
  assert.deepEqual(drawn('const a = <span>{"(" + kind + ")"}{(paused ? "paused" : "running") as string}</span>;'), ["paused", "running"]);
  assert.deepEqual(drawn("const a = <>Brings {what} with it.</>;"), ["Brings", "with it."], "a fragment's children are text nodes too");
});

test("blind spot three — a template of fewer than two words is seen where it is drawn, and where a model says it", () => {
  assert.deepEqual(drawn("const a = <span>{`${n} rules`}</span>;"), ["{x} rules"]);
  assert.deepEqual(drawn('const a = <span>{`${n} sub-agent${n === 1 ? "" : "s"}`}</span>;'), ["{x} sub-agent{x}", "s"], "and the literal inside its placeable");
  assert.deepEqual(drawn("const a = <Dot title={`started ${when}`} />;"), ["started {x}"]);
  assert.deepEqual(texts("export const words = (n) => `+${n} more`;\nexport const who = (rule) => `rule ${rule}`;\n", "mjs"), ["+{x} more", "rule {x}"]);
});

test("a prop that is a sentence counts whatever its length, and a class on the same line hides nothing", () => {
  assert.deepEqual(drawn('const a = <Icon aria-label="yes" className="inline text-ok" />;'), ["yes"]);
  assert.deepEqual(drawn('const a = <Field className="w-40" label={on ? "unset" : value} hint={t("x")} />;'), ["unset"]);
  assert.deepEqual(drawn('const a = <p className="px-1 text-3xs">The last {cap} commits; the rest is under History.</p>;'), ["The last", "commits; the rest is under History."]);
  assert.deepEqual(drawn('const a = <Chip tone="quiet" data-state="a state here" key="a key">{t("id")}</Chip>;'), [], "a prop that is not a sentence's is not read");
});

test("a prop is a sentence's by its name, an identifier's by its name, and every other prop is read as any literal is", () => {
  for (const name of ["label", "title", "aria-label", "placeholder", "keyLabel", "valuePlaceholder", "keyPlaceholder", "emptyLibrary", "confirmLabel", "what", "subject", "error"]) assert.ok(isProseProp(name), name);
  for (const name of ["className", "tone", "variant", "size", "key", "data-state", "onClick", "value"]) assert.ok(!isProseProp(name), name);
  for (const name of ["className", "bodyClassName", "data-pane", "name", "key", "id", "href", "viewBox", "preserveAspectRatio", "aria-hidden"]) assert.ok(isIdentifierProp(name), name);
  for (const name of ["label", "tone", "error", "emptyLibrary", "onClick"]) assert.ok(!isIdentifierProp(name), name);
  // A word in a sentence's prop counts alone; a sentence in any other prop still counts; a class and a name never do.
  assert.deepEqual(drawn('const a = <PairEditor keyLabel="Variable" valuePlaceholder="value" tone="quiet" />;'), ["Variable", "value"]);
  assert.deepEqual(texts('const a = <Picker emptyLibrary={`${RULE} Nothing is in the library yet.`} mode="quiet mode here" className="a class list here" name="the field name" />;', "tsx"), ["{x} Nothing is in the library yet.", "quiet mode here"]);
});

test("what is not a word is not counted: a separator, an entity, a call's answer, a number — and a generic is not markup", () => {
  assert.deepEqual(drawn('const a = <span>{count} · {t("screens-x")} — 12 &amp; &lt;{tag}&gt; {bytesWords(size)}</span>;'), []);
  assert.deepEqual(drawn("const [a, set] = useState<Row | null>(null);\nconst b = rows as Array<Row>;\nconst c = <T,>(x: T) => x;\n"), []);
  assert.deepEqual(drawn('const a = <p>{rich("panel-blurb", { code: (inner) => <code>{inner}</code> })}</p>;'), []);
  assert.deepEqual(drawn("const a = <p>The platform&apos;s own</p>;"), ["The platform&apos;s own"], "an entity beside words leaves the words");
});

test("a marked exception is still excused: on the line, in the tag, or on a comment line of its own just above", () => {
  assert.deepEqual(MARKS, ["for the agent", "for the machine", "for the log", "content, never translated"]);
  const tsx = `
export function Panel({ row }) {
  return (
    <div>
      <Chip>index.html</Chip> {/* for the machine */}
      {/* content, never translated: the addon's own name. */}
      <span>The addon named {row.name}</span>
      <TextInput value={row.id} /* for the machine */ placeholder="github" />
      <TextInput
        value={row.owner}
        placeholder="acme" // content, never translated
      />
      <code>{"{params.<name>}"}</code> {/* for the machine */}
      <span>{row.ok ? "found" : "missing"}</span>
    </div>
  );
}
`;
  assert.deepEqual(drawn(tsx), ["found", "missing"], "only the unmarked line is left");
  assert.ok(marked(["// for the machine", 'const a = "Bearer of it";'], 2) && !marked(["const b = 1; // for the machine", 'const a = "Bearer of it";'], 2), "the line above excuses only when it is a comment of its own");
  assert.deepEqual(texts('// for the agent\nexport const prompt = "Answer in one line";\nexport const said = "Said to a person";\n', "mjs"), ["Said to a person"]);
});

test("JSX text, a labelled prop and a words table count; a class, a log, a comment and t() do not", () => {
  const tsx = `
// A comment with a sentence in it
/* and a block one
   with another sentence */
export function Panel({ n }) {
  log.info("panel", "the panel drew itself");
  const words = { title: "Quit Bisa?", confirmLabel: "Quit" };
  return (
    <div className="flex items-center gap-2" data-pane="a pane here">
      <Button label="Save the document" aria-label="Save" title={t("panel-save")}>Save</Button>
      <p>{t("panel-lead")}</p>
      <span>Nothing needs you</span>
      <em>{n} need you</em>
      <i>{\`\${n} agents at work\`}</i>
    </div>
  );
}
`;
  // `Save` is said twice on one line — the prop and the text node — and counted once; `Quit` alone in a table is a key's word.
  assert.deepEqual(texts(tsx, "tsx"), ["Quit Bisa?", "Save", "Save the document", "Nothing needs you", "need you", "{x} agents at work"]);
});

test("a model's string literals count once each, and an identifier comparison does not", () => {
  const mjs = `
export function words(state) {
  if (state === "waiting") return "Waiting on you";
  const noun = state === "one" ? "agent" : "agents";
  return \`Working — \${n} \${noun}\`;
}
export const KEYS = Object.freeze({ quit: "desktop.confirm_quit" });
throw new Error("a developer sentence here");
`;
  assert.deepEqual(texts(mjs, "mjs"), ["Waiting on you", "Working — {x} {x}"]);
});

test("each literal is read off the parser's tree: a template nested in a placeable, a sentence after a regex that holds a quote, a path's slash", () => {
  const mjs = `
export function line(set, where, missing) {
  return \`\${set.join(", ")} \${where}\${missing.length > 0 ? \` — \${missing.join(", ")} still wanted\` : ""}\`;
}
const QUOTED = /^"([^"]*)"$/;
export const refused = "Nothing to run.";
export const where = "in a 0600 file under identity/";
export const classes = "flex items-center gap-2";
export const word = 'a single-quoted identifier here';
`;
  assert.deepEqual(texts(mjs, "mjs"), ["— {x} still wanted", "Nothing to run.", "in a 0600 file under identity/"]);
  assert.deepEqual(texts("log.warn(\n  \"api\",\n  \"the node did not answer\",\n);\nexport const said = \"The node did not answer\";\n", "ts"), ["the node did not answer", "The node did not answer"], "a log line is excused on its own line only — a call that wraps says its sentence on a line of its own, and is marked there");
});

test("a literal is excused by where it stands, never by what else its line holds: a class, a data word, an icon's key", () => {
  const tsx = `
const ROWS = [{ icon: "archive", text: \`workflow \${id} archived\` }];
export function Row({ on, n }) {
  return (
    <li className={cn("block truncate", on && "font medium")} data-state="a state here" name="the row name" onClick={() => toast.ok("Saved the row")}>
      <Chip className="text-dim shrink-0" tone="quiet">{label(n)}</Chip>
    </li>
  );
}
`;
  assert.deepEqual(texts(tsx, "tsx"), ["workflow {x} archived", "Saved the row"], "the sentence beside an icon's key is seen, a handler's sentence is seen; the classes, the data word and the name are not");
});

test("a line marked for the machine, for the agent or as content is not a sentence for a person", () => {
  const mjs = `
export const bearer = (token) => \`Bearer of \${token}\`; // for the machine
export const prompt = "Answer in one line"; // for the agent
export const subject = "Notes of the day"; // content, never translated
export const said = "Said to a person";
`;
  assert.deepEqual(texts(mjs, "mjs"), ["Said to a person"]);
});

test("comments go, files that are not the desktop's words are left alone, and the comparison names what moved", () => {
  assert.equal(withoutComments("a // b\n// c\n/* d\ne */ f").replace(/\s+/g, " ").trim(), "a // b f", "a trailing comment is kept whole: a URL may hold //");
  assert.equal(kindOf("views/Inbox.tsx"), "tsx");
  assert.equal(kindOf("shell/trayModel.mjs"), "mjs");
  assert.equal(kindOf("shell/trayModel.test.mjs"), null);
  assert.equal(kindOf("shell/trayModel.d.mts"), null);
  assert.equal(kindOf("scenarios/tray.test.mjs"), null);
  assert.equal(kindOf("types.gen.ts"), null);
  assert.equal(kindOf("i18n/ratchetModel.mjs"), null);
  const { up, down } = compare({ "a.tsx": 2, "b.mjs": 1 }, { "a.tsx": 3, "c.ts": 1 });
  assert.deepEqual(up, ["a.tsx: 2 → 3", "c.ts: 0 → 1"]);
  assert.deepEqual(down, ["b.mjs: 1 → 0"]);
});
