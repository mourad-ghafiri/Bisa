/**
 * The client's facts: the base, the failure words, the read deadline. Run
 * with `node --test desktop/src/apiModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import { API_READ_TIMEOUT_MS, DEFAULT_BASE, answerUnreadableWords, baseOf, bearer, codeOf, deadlineSeconds, detailOf, errorDetail, failureOf, isOffline, isRefusal, isServerError, looseRefusal, normalizeBase, refusalOf, streamEndedWords, takesDeadline, tokenToKeep } from "./apiModel.mjs";

test("a loose file's refusal from the shell is the node's own status and body for the same fact: a conflict's 409 with the current text, a missing file's 404, a file over the bound's 413 with its size and limit, anything else a 400", () => {
  const MIB = 1024 * 1024;
  const big = looseRefusal({ kind: "too_large", message: "/Users/me/big.log is 25165824 bytes; the editor stops at 20971520", size: 24 * MIB, limit: 20 * MIB }, "the shell refused");
  assert.deepEqual(big, { status: 413, message: "/Users/me/big.log is 25165824 bytes; the editor stops at 20971520", body: { error: "/Users/me/big.log is 25165824 bytes; the editor stops at 20971520", size: 24 * MIB, limit: 20 * MIB } });
  const conflict = looseRefusal({ kind: "conflict", message: "the file changed on disk", current_hash: "h2", current_text: "theirs" }, "the shell refused");
  assert.deepEqual(conflict, { status: 409, message: "the file changed on disk", body: { error: "the file changed on disk", current_hash: "h2", current_text: "theirs" } });
  assert.deepEqual(looseRefusal({ kind: "missing", message: "no such file" }, "the shell refused"), { status: 404, message: "no such file", body: { error: "no such file" } });
  assert.deepEqual(looseRefusal({ kind: "refused", message: "not a text file" }, "the shell refused"), { status: 400, message: "not a text file", body: { error: "not a text file" } });
  assert.equal(looseRefusal({ kind: "refused" }, "the shell refused").message, "the shell refused", "a tag with no words takes the given sentence");
  assert.deepEqual(looseRefusal(new Error("boom"), "the shell refused"), { status: 400, message: "boom", body: { error: "boom" } }, "untagged: a 400 in its own words");
  assert.equal(looseRefusal("plain", "the shell refused").message, "plain");
  for (const r of [big, conflict]) assert.ok(isRefusal(r), "each reads as a refusal, apart from a failure");
});

test("the base is the environment's word, else the shell's, else the default — never with a trailing slash", () => {
  assert.equal(baseOf("http://127.0.0.1:5000/", null), "http://127.0.0.1:5000");
  assert.equal(baseOf("  http://h:1//  ", "http://shell:2"), "http://h:1", "the environment wins, trimmed");
  assert.equal(baseOf(undefined, "http://127.0.0.1:4491/"), "http://127.0.0.1:4491");
  assert.equal(baseOf("", ""), DEFAULT_BASE, "empty words are no words");
  assert.equal(baseOf(null, undefined), DEFAULT_BASE);
  assert.equal(normalizeBase("http://a/"), "http://a");
  assert.equal(normalizeBase("http://a"), "http://a");
});

test("a refusal reads as the node's sentence, else the body's first line, else the status line; the code and the detail are guarded", () => {
  assert.equal(errorDetail(409, "Conflict", { error: "the branch moved", code: "moved" }), "the branch moved");
  assert.equal(errorDetail(409, "Conflict", { error: "  " }, '{"error":"  "}'), "409 Conflict", "a blank sentence is no sentence, and a JSON body's text is not repeated");
  assert.equal(errorDetail(502, "Bad Gateway", undefined, "upstream reset\nmore"), "upstream reset", "a non-JSON body: its first line");
  assert.equal(errorDetail(500, "Internal Server Error", undefined, ""), "500 Internal Server Error");
  assert.equal(errorDetail(500, "", undefined, "   \n\n  "), "500", "whitespace is no line");
  assert.equal(errorDetail(500, "Internal Server Error", undefined, "x".repeat(300)).length, 201, "a long line is cut with an ellipsis");
  assert.equal(codeOf({ code: "publish_declined" }), "publish_declined");
  assert.equal(codeOf({ code: 7 }), null, "a code is a word");
  assert.equal(codeOf({ code: "" }), null);
  assert.equal(codeOf(null), null);
  assert.equal(codeOf("nope"), null);
  assert.deepEqual(detailOf({ detail: { paths: ["a"] } }), { paths: ["a"] });
  assert.equal(detailOf({ detail: null }), null);
  assert.equal(detailOf({ detail: "words" }), null, "a detail is an object");
  assert.equal(detailOf({ detail: [1] }), null, "and not a list");
  assert.equal(detailOf(undefined), null);
  assert.ok(isOffline(0) && !isOffline(404));
  assert.ok(isServerError(500) && isServerError(503) && !isServerError(499) && !isServerError(0));
});

test("no answer is one of three things: the caller gave up, the read outstayed its deadline, or the node could not be reached", () => {
  assert.deepEqual(failureOf(new Error("Failed to fetch"), true), { kind: "aborted", reason: "the caller gave up" });
  const timeout = Object.assign(new Error("signal timed out"), { name: "TimeoutError" });
  assert.deepEqual(failureOf(timeout, false), { kind: "timeout", reason: `no answer within ${API_READ_TIMEOUT_MS / 1000} s` });
  assert.deepEqual(failureOf(timeout, false, false, 5000), { kind: "timeout", reason: "no answer within 5 s" });
  assert.deepEqual(failureOf(new TypeError("Failed to fetch"), false), { kind: "unreachable", reason: "node unreachable", detail: "Failed to fetch" }, "the engine's transport text is detail for the log, never the words a person reads");
  assert.deepEqual(failureOf(new Error(""), false), { kind: "unreachable", reason: "node unreachable", detail: null }, "a nameless failure still has words");
  assert.deepEqual(failureOf(null, false), { kind: "unreachable", reason: "node unreachable", detail: null });
  assert.deepEqual(failureOf("boom", false), { kind: "unreachable", reason: "node unreachable", detail: null });
});

test("every read takes the deadline, the caller's signal or not; a write does not", () => {
  assert.equal(takesDeadline("GET"), true);
  assert.equal(takesDeadline("get"), true);
  for (const m of ["POST", "PUT", "PATCH", "DELETE"]) assert.equal(takesDeadline(m), false, `${m} may take as long as it needs`);
  assert.ok(API_READ_TIMEOUT_MS >= 10_000 && API_READ_TIMEOUT_MS <= 60_000, "a deadline a person would call slow, not a hang");
  // The workspace's load hands every read its signal: were that an excuse, no list would have a deadline.
  const api = readFileSync(new URL("./api.ts", import.meta.url), "utf8");
  const req = api.slice(api.indexOf("async function req<T>("), api.indexOf("\nconst get = "));
  assert.ok(req.includes("takesDeadline(method) ? boundOf(signal, API_READ_TIMEOUT_MS)"), "the bound stands beside the caller's signal, never in its place");
  assert.ok(req.includes("bound?.release()"), "and its clock is let go when the read is over");
  assert.ok(!api.includes("AbortSignal."), "built by hand: the oldest webview the bundle runs in has neither static");
});

test("a read that ran out of time under the caller's own signal is a timeout, and a caller that left is told nothing", () => {
  const aborted = Object.assign(new Error("The operation was aborted."), { name: "AbortError" });
  assert.deepEqual(failureOf(aborted, false, true), { kind: "timeout", reason: `no answer within ${API_READ_TIMEOUT_MS / 1000} s` }, "the deadline ended it: the abort is the clock's");
  assert.equal(failureOf(aborted, true, true).kind, "aborted", "the caller left first, or as well: nothing to say");
  assert.equal(failureOf(aborted, false, false).kind, "unreachable", "an abort that is nobody's is no answer");
});

test("an answer that came and could not be read is the node's fault and never the node being away; a stream that ended early says so", () => {
  assert.equal(answerUnreadableWords(), "the node's answer could not be read");
  assert.match(streamEndedWords(), /ended before it finished/);
  const api = readFileSync(new URL("./api.ts", import.meta.url), "utf8");
  const req = api.slice(api.indexOf("async function req<T>("), api.indexOf("\nconst get = "));
  assert.ok(req.includes("new ApiError(answerUnreadableWords(), res.status, path)"), "it carries the status the node sent — `offline` reads status 0 alone");
  assert.ok(!/await res\.json\(\)/.test(req), "the body is read as text under the bound, then parsed");
  const sse = api.slice(api.indexOf("export function sse<"), api.indexOf("function authHeaders("));
  assert.ok(sse.includes("fail(new Error(streamEndedWords()))"), "the one-shot stream's end is the model's sentence");
  assert.ok(!/new Error\("[A-Z]/.test(sse), `no sentence is written in the client: ${sse.match(/new Error\("[^"]*"\)/)?.[0]}`);
});

test("a token is kept only once there is one: an empty answer is asked again, never remembered", () => {
  assert.equal(tokenToKeep("  9f2c  "), "9f2c", "the value, trimmed — a token read from a file ends in a newline");
  for (const none of ["", "   ", "\n", null, undefined, 0, {}]) assert.equal(tokenToKeep(none), null, `${JSON.stringify(none)} is no token`);
});

test("what the node is sent is written in code: the bearer is never a message a translation could change", () => {
  assert.equal(bearer("9f2c"), "Bearer 9f2c");
  const api = readFileSync(new URL("./api.ts", import.meta.url), "utf8");
  assert.ok(api.includes("authorization: bearer(token)"), "the header is the model's");
  const headers = api.slice(api.indexOf("function authHeaders("), api.indexOf("\n}\n", api.indexOf("function authHeaders(")));
  assert.ok(!/\btr?\(/.test(headers), `no message stands in a header: ${headers}`);
  const catalog = readFileSync(new URL("../../locales/en/desktop/app.ftl", import.meta.url), "utf8");
  assert.ok(!/^app-api-bearer\b/m.test(catalog), "and the catalog holds no such word");
});

test("401: an unauthorized answer reads as what it is, and is the caller's error — never the node being down", () => {
  assert.equal(errorDetail(401, "Unauthorized", { error: "missing or wrong token", code: "unauthorized" }), "missing or wrong token");
  assert.ok(errorDetail(401, "Unauthorized", null, "").includes("401"), "with no body at all, the status is the words");
  assert.ok(!isServerError(401) && !isOffline(401));
  assert.equal(codeOf({ error: "missing or wrong token", code: "unauthorized" }), "unauthorized");
});

test("the client forgets its token with its base when the node restarts, and keeps no empty one", () => {
  const api = readFileSync(new URL("./api.ts", import.meta.url), "utf8");
  const forget = api.slice(api.indexOf("export function forgetApiBase()"), api.indexOf("let apiToken"));
  assert.ok(forget.includes("apiBase = null") && forget.includes("apiToken = null"));
  assert.ok(api.includes('apiToken = tokenToKeep(await invoke<string>("api_token"))'), "the shell's answer is kept by the model's rule");
});

test("a refusal is the node saying no about what was asked — never a malfunction, never no answer at all", () => {
  const at = (status) => Object.assign(new Error("x"), { status });
  for (const status of [400, 404, 409, 413, 422]) assert.equal(isRefusal(at(status)), true, String(status));
  for (const status of [500, 502, 0]) assert.equal(isRefusal(at(status)), false, String(status));
  assert.equal(isRefusal(new Error("no status at all")), false);
  assert.equal(isRefusal(null), false);
  assert.equal(isRefusal("offline"), false);
});

test("the deadline is counted in milliseconds and said in seconds — one change of unit, rounded up, never 0 s", () => {
  assert.equal(deadlineSeconds(30_000), 30);
  assert.equal(deadlineSeconds(API_READ_TIMEOUT_MS), API_READ_TIMEOUT_MS / 1000);
  assert.equal(deadlineSeconds(1500), 2, "a wait of 1.5 s is within 2 s — never a shorter wait than the one that ran out");
  assert.equal(deadlineSeconds(1400), 2);
  assert.equal(deadlineSeconds(400), 1, "never 0 s");
  assert.equal(deadlineSeconds(1), 1);
  for (const none of [0, -5, Number.NaN, Number.POSITIVE_INFINITY, undefined]) assert.equal(deadlineSeconds(none), API_READ_TIMEOUT_MS / 1000, "no deadline named: the reads' own");
  // The sentence is handed seconds and says seconds.
  const timeout = Object.assign(new Error("timed out"), { name: "TimeoutError" });
  assert.equal(failureOf(timeout, false, false, 1500).reason, "no answer within 2 s");
  assert.equal(failureOf(timeout, false, true, 400).reason, "no answer within 1 s");
  assert.equal(failureOf(timeout, false, true, 45_000).reason, "no answer within 45 s");
  assert.ok(!failureOf(timeout, false, true, 30_000).reason.includes("30000"), "the milliseconds never reach the sentence");
});

test("a refusal is named by the message it travels as, the same in every language — or by nothing", () => {
  assert.equal(refusalOf({ error: "the URL must be absolute", text: { id: "error-core-mcp-invalid-url", args: { v0: "x" } } }), "error-core-mcp-invalid-url");
  assert.equal(refusalOf({ error: "l'URL doit être absolue", text: { id: "error-core-mcp-invalid-url" } }), "error-core-mcp-invalid-url", "the words change with the language; the id does not");
  for (const none of [null, undefined, "boom", 5, [], {}, { text: null }, { text: "a sentence" }, { text: { id: "" } }, { text: { id: 7 } }]) assert.equal(refusalOf(none), null);
});

