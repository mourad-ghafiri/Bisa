/**
 * The document a page artifact runs as. Run with `node --test desktop/src/ui/artifact/pageDocument.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { PAGE_CDNS, PAGE_SANDBOX, pageCsp, pageDocument } from "./pageDocument.mjs";
import { inspectorTheme } from "./inspectorTheme.mjs";
import { inspectorScript } from "./pageInspector.mjs";

const THEME = inspectorTheme({}, "light");

test("the sandbox allows scripts and nothing else", () => {
  assert.equal(PAGE_SANDBOX, "allow-scripts");
  assert.ok(!PAGE_SANDBOX.includes("allow-same-origin"), "an opaque origin, always");
});

test("the policy reaches nothing on the network but the libraries, and only when allowed", () => {
  const open = pageCsp({ libraries: true });
  for (const cdn of PAGE_CDNS) assert.ok(open.includes(cdn), cdn);
  assert.ok(open.includes("connect-src 'none'"));
  assert.ok(open.includes("frame-src 'none'"));
  assert.ok(open.includes("form-action 'none'"));
  assert.ok(open.includes("default-src 'none'"));
  assert.ok(open.includes("fonts.googleapis.com") && open.includes("fonts.gstatic.com"));

  const closed = pageCsp({ libraries: false });
  for (const cdn of PAGE_CDNS) assert.ok(!closed.includes(cdn), `${cdn} is off`);
  assert.ok(!closed.includes("fonts."));
  assert.ok(closed.includes("script-src 'unsafe-inline' 'unsafe-eval'"), "the page's own scripts still run");
  assert.ok(closed.includes("img-src data: blob: https:"), "a picture the page carries still shows");
});

test("the metas land first in the head, whatever shape the page came in", () => {
  const whole = pageDocument("<!doctype html><html><head><title>x</title><script>1</script></head><body>hi</body></html>", {
    libraries: true,
    scheme: "dark",
  });
  const head = whole.indexOf("<head>") + "<head>".length;
  assert.ok(whole.slice(head).startsWith('<meta http-equiv="Content-Security-Policy"'), "the policy is the first thing in the head");
  assert.ok(whole.includes('<meta name="color-scheme" content="dark light">'));
  assert.ok(whole.indexOf("Content-Security-Policy") < whole.indexOf("<script>"), "before any script");

  const noHead = pageDocument("<html><body>hi</body></html>");
  assert.ok(noHead.includes("<html><head><meta http-equiv"), "a head is made");
  assert.ok(noHead.includes('content="light dark"'));

  const fragment = pageDocument("<h1>hi</h1>");
  assert.ok(fragment.startsWith("<!doctype html><html><head><meta http-equiv"));
  assert.ok(fragment.endsWith("<body><h1>hi</h1></body></html>"));

  const attr = pageDocument('<head lang="en">x</head>');
  assert.ok(attr.startsWith('<head lang="en"><meta http-equiv'));
  assert.ok(!pageDocument("<p>q</p>", { libraries: false }).includes("cdnjs"));
});

test("the inspector lands after the policy and before the page's first script, and only when asked", () => {
  const page = "<!doctype html><html><head><title>x</title><script>1</script></head><body>hi</body></html>";
  for (const shape of [page, "<html><body><script>1</script></body></html>", "<h1>hi</h1><script>1</script>"]) {
    const doc = pageDocument(shape, { inspector: THEME });
    const policy = doc.indexOf("Content-Security-Policy");
    const inspector = doc.indexOf(inspectorScript(THEME));
    const pages = doc.indexOf("<script>1</script>");
    assert.ok(policy !== -1 && inspector !== -1 && pages !== -1, shape);
    assert.ok(policy < inspector && inspector < pages, `policy, then the inspector, then the page: ${shape}`);
    assert.ok(doc.includes(`<script>${inspectorScript(THEME)}</script>`), "one inline script, the policy already allows it");
  }
  assert.ok(!pageDocument(page).includes("bisa:inspect"), "not asked: not there");
  assert.ok(!pageDocument(page, { inspector: null }).includes("bisa:inspect"));
  assert.ok(pageDocument(page, { inspector: THEME }).includes("connect-src 'none'"), "the walls are the same with it");
  assert.equal(PAGE_SANDBOX, "allow-scripts", "and so is the sandbox");
});
