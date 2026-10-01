import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  applyTextEdits,
  documentsToReopen,
  hoverMarkdown,
  sameServer,
  serverChange,
  serverChip,
  symbolKindName,
  toDocumentSymbols,
  toLocations,
  toMarkerSeverity,
  toMarkers,
  toMonacoRange,
  toMonacoSymbolKind,
  toWorkspaceSymbols,
  toLspPosition,
} from "./lspModel.mjs";

const R = (l1, c1, l2, c2) => ({ start: { line: l1, character: c1 }, end: { line: l2, character: c2 } });

test("positions and ranges shift by one, both ways", () => {
  assert.deepEqual(toLspPosition({ lineNumber: 3, column: 5 }), { line: 2, character: 4 });
  assert.deepEqual(toMonacoRange(R(1, 2, 3, 4)), { startLineNumber: 2, startColumn: 3, endLineNumber: 4, endColumn: 5 });
});

test("severities and symbol kinds are mapped, not cast", () => {
  assert.deepEqual([1, 2, 3, 4, undefined].map(toMarkerSeverity), [8, 4, 2, 1, 8]);
  assert.equal(toMonacoSymbolKind(1), 0, "File");
  assert.equal(toMonacoSymbolKind(12), 11, "Function");
  assert.equal(toMonacoSymbolKind(99), 12, "unknown falls back to Variable");
  assert.equal(symbolKindName(5), "class");
  assert.equal(symbolKindName(22), "enum member");
  assert.equal(symbolKindName(26), "type parameter");
  assert.equal(symbolKindName(1), "file");
  assert.equal(symbolKindName(27), "symbol", "a kind LSP does not name");
  assert.equal(symbolKindName(undefined), "symbol");
  const markers = toMarkers([{ range: R(0, 0, 0, 3), message: "unused", severity: 2, code: { value: "E1" }, source: "ra" }]);
  assert.deepEqual(markers[0], { startLineNumber: 1, startColumn: 1, endLineNumber: 1, endColumn: 4, message: "unused", severity: 4, code: "E1", source: "ra" });
});

test("symbols come in two shapes and locations in three", () => {
  const hier = toDocumentSymbols([{ name: "f", kind: 12, range: R(0, 0, 2, 0), selectionRange: R(0, 3, 0, 4), children: [{ name: "x", kind: 13, range: R(1, 0, 1, 5), selectionRange: R(1, 0, 1, 1) }] }]);
  assert.equal(hier[0].name, "f");
  assert.equal(hier[0].kind, 11);
  assert.equal(hier[0].children[0].name, "x");
  const flat = toDocumentSymbols([{ name: "g", kind: 12, location: { uri: "a.rs", range: R(4, 0, 4, 1) }, containerName: "mod" }]);
  assert.equal(flat[0].range.startLineNumber, 5);
  assert.equal(flat[0].detail, "mod");
  assert.deepEqual(toLocations({ uri: "a.rs", range: R(9, 2, 9, 5) }), [{ path: "a.rs", line: 10, column: 3 }]);
  assert.deepEqual(toLocations([{ targetUri: "b.rs", targetSelectionRange: R(0, 0, 0, 1) }, { uri: null, range: R(0, 0, 0, 1) }]), [{ path: "b.rs", line: 1, column: 1 }]);
  assert.deepEqual(toLocations(null), []);
});

test("workspace symbols become the palette's rows: a word for the kind, the 1-based line, and none for a symbol outside the root", () => {
  const rows = toWorkspaceSymbols([
    { name: "main", kind: 12, location: { uri: "src/main.rs", range: R(9, 0, 9, 4) } },
    { name: "Config", kind: 23, location: { uri: "src/config.rs" } },
    { name: "outside", kind: 12, location: { uri: null, range: R(0, 0, 0, 1) } },
    { name: "nowhere", kind: 12 },
  ]);
  assert.deepEqual(rows, [
    { name: "main", kind: "function", path: "src/main.rs", line: 10 },
    { name: "Config", kind: "struct", path: "src/config.rs", line: 1 },
  ]);
  assert.deepEqual(toWorkspaceSymbols(null), []);
  assert.deepEqual(toWorkspaceSymbols({ error: "no server" }), [], "an answer that is no list is no rows");
  const palette = readFileSync(new URL("../../shell/Omnibox.tsx", import.meta.url), "utf8");
  assert.ok(palette.includes("setSymbols(toWorkspaceSymbols(r.result));"), "the palette draws the model's rows");
});

test("hover contents in every shape become markdown", () => {
  assert.equal(hoverMarkdown("plain"), "plain");
  assert.equal(hoverMarkdown({ kind: "markdown", value: "**b**" }), "**b**");
  assert.equal(hoverMarkdown({ language: "rust", value: "fn x()" }), "```rust\nfn x()\n```");
  assert.equal(hoverMarkdown(["a", { kind: "plaintext", value: "b" }]), "a\n\nb");
  assert.equal(hoverMarkdown(null), "");
});

test("text edits apply from the end and overlaps are refused", () => {
  const text = "let x = 1;\nlet y = 2;\n";
  const out = applyTextEdits(text, [
    { range: R(0, 4, 0, 5), newText: "count" },
    { range: R(1, 8, 1, 9), newText: "3" },
  ]);
  assert.equal(out, "let count = 1;\nlet y = 3;\n");
  assert.equal(applyTextEdits(text, []), text);
  // A whole-document replacement, as formatters send.
  assert.equal(applyTextEdits(text, [{ range: R(0, 0, 2, 0), newText: "fmt\n" }]), "fmt\n");
  // Overlapping edits: unchanged rather than corrupted.
  assert.equal(applyTextEdits(text, [{ range: R(0, 0, 0, 5), newText: "a" }, { range: R(0, 3, 0, 8), newText: "b" }]), text);
  // Out-of-range positions clamp instead of throwing.
  assert.equal(applyTextEdits("ab", [{ range: R(5, 5, 9, 9), newText: "!" }]), "ab!");
});

const lsp = (method, over = {}) => ({ type: "lsp", scope: "workstream", id: "W1", language: "rust", method, params: null, ...over });
const open = (path, over = {}) => ({ scope: "workstream", id: "W1", path, language: "rust", following: "yes", ...over });

test("a server's lifecycle is read off its three notifications, and nothing else is one", async () => {
  assert.deepEqual(serverChange(lsp("bisa/serverStopped")), { scope: "workstream", id: "W1", language: "rust", event: "stopped" });
  assert.deepEqual(serverChange(lsp("bisa/serverStarted", { params: { command: "rust-analyzer" } })), { scope: "workstream", id: "W1", language: "rust", event: "started" });
  assert.deepEqual(serverChange(lsp("bisa/serverFailed", { params: { reason: "exited" } })), { scope: "workstream", id: "W1", language: "rust", event: "failed" });
  assert.equal(serverChange(lsp("textDocument/publishDiagnostics")), null, "a diagnostic is the document's, not the server's lifecycle");
  assert.equal(serverChange(lsp("toString")), null, "a method is looked up as a name, never as a property of the table");
  assert.equal(serverChange({ type: "file_changed", scope: "workstream", id: "W1" }), null);
  assert.equal(serverChange(lsp("bisa/serverStopped", { language: "" })), null, "a frame that names no language names no server");
  assert.equal(serverChange(null), null);
  // The three names are the engine's own.
  const engine = readFileSync(new URL("../../../../crates/bisa-engine/src/lsp.rs", import.meta.url), "utf8");
  for (const method of ["bisa/serverStarted", "bisa/serverFailed", "bisa/serverStopped"]) assert.ok(engine.includes(`"${method}"`), `${method} is what the engine says`);
  assert.deepEqual([...engine.matchAll(/"(bisa\/server[A-Za-z]+)"/g)].map((m) => m[1]).filter((m, i, all) => all.indexOf(m) === i).sort(), ["bisa/serverFailed", "bisa/serverStarted", "bisa/serverStopped"], "and the engine says no fourth");
});

test("one server a root and language: a change is a document's only when all three are its own", () => {
  const change = serverChange(lsp("bisa/serverStopped"));
  assert.equal(sameServer(change, open("src/main.rs")), true);
  assert.equal(sameServer(change, open("src/main.rs", { id: "W2" })), false, "another root");
  assert.equal(sameServer(change, open("src/main.rs", { scope: "project" })), false, "another kind of root");
  assert.equal(sameServer(change, open("src/app.ts", { language: "typescript" })), false, "another language");
  assert.equal(sameServer(change, open("notes.txt", { language: null })), false, "a document no server was ever said to follow");
  assert.equal(sameServer(change, open("notes.txt", { language: undefined })), false, "one whose first open has not answered");
  assert.equal(sameServer(null, open("src/main.rs")), false);
});

test("a server that stopped: every open document of its root and language is opened again on the next one — and no other", () => {
  const documents = [open("src/main.rs"), open("src/lib.rs", { following: "asking" }), open("src/gone.rs", { following: "no" }), open("src/app.ts", { language: "typescript" }), open("src/main.rs", { id: "W2" }), open("README", { language: null, following: "no" })];
  const again = documentsToReopen(serverChange(lsp("bisa/serverStopped")), documents);
  assert.deepEqual(again.map((d) => `${d.id}:${d.path}`), ["W1:src/main.rs", "W1:src/lib.rs", "W1:src/gone.rs"], "a Restart, a crash, a setting that changed: whatever each was doing — an open still out may have landed on the server that went");
  assert.equal(again[0], documents[0], "the documents themselves, for the caller to act on");
  assert.deepEqual(documentsToReopen(serverChange(lsp("bisa/serverStopped", { language: "go" })), documents), [], "a server none of them is on");
  assert.deepEqual(documentsToReopen(serverChange(lsp("bisa/serverStopped")), []), []);
  assert.deepEqual(documentsToReopen(serverChange(lsp("bisa/serverStopped")), null), []);
});

test("a server that is there again takes the documents on none; one followed, or being opened, is not opened twice", () => {
  const documents = [open("src/main.rs"), open("src/lib.rs", { following: "asking" }), open("src/gone.rs", { following: "no" }), open("src/app.ts", { language: "typescript", following: "no" })];
  assert.deepEqual(documentsToReopen(serverChange(lsp("bisa/serverStarted")), documents).map((d) => d.path), ["src/gone.rs"], "started by another document's open, or by a request");
  assert.deepEqual(documentsToReopen({ scope: "workstream", id: "W1", language: "rust", event: "restarted" }, documents).map((d) => d.path), ["src/gone.rs"], "the person asked after the crash loop's verdict: the node says nothing on the stream, the act is the change");
  assert.deepEqual(documentsToReopen(serverChange(lsp("bisa/serverFailed")), documents), [], "a server that gave up has nothing to open anything on");
  assert.deepEqual(documentsToReopen(null, documents), [], "a frame that is no lifecycle");
  assert.deepEqual(documentsToReopen(serverChange(lsp("textDocument/publishDiagnostics")), documents), []);
});

test("a crash loop ends where the node ends it: each stop opens the document once more, and the verdict opens nothing", () => {
  // The document as the client holds it across the loop.
  let doc = open("src/main.rs");
  const stopped = serverChange(lsp("bisa/serverStopped"));
  for (let crash = 1; crash <= 3; crash++) {
    assert.equal(documentsToReopen(stopped, [doc]).length, 1, `crash ${crash}: opened on the next server`);
    doc = { ...doc, following: "asking" };
    assert.equal(documentsToReopen(serverChange(lsp("bisa/serverStarted")), [doc]).length, 0, "the start its own open caused opens nothing more");
    doc = { ...doc, following: crash < 3 ? "yes" : "no" };
  }
  // The fourth start is the node's verdict: it answers no server, says `serverFailed`, and starts nothing.
  assert.equal(documentsToReopen(serverChange(lsp("bisa/serverFailed", { params: { reason: "restarted 3 times in a minute" } })), [doc]).length, 0);
  // Restart: the verdict is forgotten, and the document is opened again.
  assert.equal(documentsToReopen({ scope: "workstream", id: "W1", language: "rust", event: "restarted" }, [doc]).length, 1);
});

test("the chip says the server's state in the catalog's words, its tone, its hover and whether Restart is offered", () => {
  const row = (state, over = {}) => ({ language: "rust", command: "rust-analyzer", available: true, install_hint: null, state, documents: 1, ...over });
  assert.deepEqual(serverChip("rust", row({ state: "running" })), { tone: "ok", words: "rust server running", title: "rust-analyzer · running", restart: false });
  assert.deepEqual(serverChip("rust", row({ state: "starting" })), { tone: "dim", words: "rust server starting", title: "rust-analyzer · starting", restart: false });
  assert.deepEqual(serverChip("rust", row({ state: "stopped" })), { tone: "dim", words: "rust server stopped", title: "rust-analyzer · stopped", restart: false });
  assert.deepEqual(serverChip("rust", row({ state: "failed", reason: "rust-analyzer restarted 3 times in a minute; not started again until you ask" })), {
    tone: "danger",
    words: "rust server failed",
    title: "rust-analyzer: rust-analyzer restarted 3 times in a minute; not started again until you ask",
    restart: true,
  }, "given up after repeated crashes: the reason on the chip's hover, and the one state Restart is offered in");
  const missing = serverChip("go", row({ state: "stopped" }, { command: "gopls", available: false, install_hint: "go install golang.org/x/tools/gopls@latest" }));
  assert.equal(missing.words, "go: no server installed");
  assert.equal(missing.title, "gopls is not on your PATH — go install golang.org/x/tools/gopls@latest");
  assert.equal(serverChip("go", row({ state: "stopped" }, { command: "gopls", available: false })).title, "gopls is not on your PATH");
  assert.equal(serverChip(null, row({ state: "running" })), null, "no server follows the document");
  assert.equal(serverChip("rust", null), null, "the status row is not read yet");
});

test("the client opens a document again through the model, clears a gone server's diagnostics, and the chip re-reads its row on the server's own frames", () => {
  const here = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const client = here("./lspClient.ts");
  assert.ok(client.includes("if (documentsToReopen(change, [mine]).length === 0) return;"), "which documents are opened again is the model's");
  assert.ok(client.includes('if (change.event !== "started" && change.event !== "restarted" && sameServer(change, mine)) clearMarkers();'), "a stopped or failed server's diagnostics are nobody's");
  assert.ok(client.includes("const change = serverChange(p);") && client.includes("onServer.current?.(change);"), "a lifecycle frame reaches the document");
  assert.ok(client.includes("if (mine !== asked) return;"), "of two opens out, the newest's answer stands");
  assert.ok(client.includes("if (opening.current) return;") && client.includes("if (latest.current !== carried) void api.lspChange("), "an edit never races an open: it waits, then goes whole");
  assert.ok(client.includes('hear({ scope, id, language, event: "restarted" })'), "a person's Restart is said to every open document");
  assert.ok(client.includes("if (r.following && r.language) {") && client.includes("languageRef.current = r.language;\n            onLanguage(r.language);"), "a language the node named with no server following is kept: it is which server's start opens the document again");
  assert.ok(client.includes("if (r.following) void api.lspClose("), "only a document a server took is closed on the node");
  assert.ok(client.includes("useReloadOnReconnect(() => {") && client.includes("void api.lspChange(scope, id, path, latest.current).catch((e: unknown) => log.debug(\"lsp\", \"a document could not be sent again after the node came back"), "a node that restarted says nothing of the servers it lost: the document goes again, whole, when the bus is back");
  const doc = here("./EditorDoc.tsx");
  assert.ok(doc.includes("const chip = serverChip(lspLanguage, server);"), "the chip is the model's");
  assert.ok(!doc.includes("server.state.state"), "the wire's word for a state is never drawn");
  assert.ok(doc.includes("if (sameServer(serverChange(e.payload), { scope, id, language: lspLanguage })) reloadLspStatus();"), "the row is read again when the server starts, fails or stops");
  assert.ok(doc.includes("lspRestarted(scope, id, lspLanguage);"), "and Restart opens what the verdict left on no server");
});
