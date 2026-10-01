/**
 * A conversation reads itself where it is shown (13-conversations, the
 * guide's Inbox chapter), as the sources show it: the thread is the one
 * surface that marks a scope read, no route does, and every screen that
 * shows a thread does so through the one component. A source assertion, as
 * `sidebar.test.mjs` makes them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/readAsShown.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("the thread reads itself as it is shown, and no route marks anything read", () => {
  const chat = src("../views/_studio/Chat.tsx");
  // A thread with a place kept does not start at its bottom, so one put back above new replies stays unread.
  for (const fact of ["useReadAsShown({", "shown: shownToReader({ visible, atBottom, loaded: !conv.loading })", "hostedUnread(ws.hosted, hosted.host, scope)", "const [atBottom, setAtBottom] = useState(() => place.kept === null);", "atBottomOf(el.scrollHeight, el.scrollTop, el.clientHeight)", "data-message={m.id}"]) {
    assert.ok(chat.includes(fact), `the chat ${fact}`);
  }
  assert.ok(!chat.includes("pinnedToBottom"), "the bottom is a fact the read reads, not a ref");
  const hook = src("../views/_studio/useReadAsShown.ts");
  assert.ok(hook.includes("readWanted(unreadCount, row)") && hook.includes("READ_AFTER_MS") && hook.includes("markRead(scope, host)"), "the hook waits the beat and marks through the store");
  const app = src("../App.tsx");
  assert.ok(!app.includes("markRead") && !app.includes("scopeOf"), "the shell has no read duty");
  assert.ok(!src("../router.ts").includes("scopeOf"), "no route knows a conversation scope for reading");
  assert.ok(src("../views/_studio/Conversation.tsx").includes("key={`${kind}:${scope}`}"), "a switch of scope starts the thread afresh");
});

test("a thread comes back where it was being read, and reading waits for the reader to come down", () => {
  const chat = src("../views/_studio/Chat.tsx");
  assert.ok(chat.includes("useThreadPlace(chatKey(kind, scope))"), "one place a thread, under the chat's own key, whichever screen shows it");
  assert.ok(chat.includes("restoreStep({ kept, loading: out, ids, hasOlder: conv.hasOlder, pagesLoaded: pagesRead.current })"), "the model says what a restore does next");
  assert.ok(chat.includes("if (restoring.current) return;") && chat.includes("if (!el.isConnected || restoring.current) return;"), "a scroll the restore made is not kept, nor one read off a detached viewport");
  assert.ok(chat.includes("pinnedBox.current?.contains(document.activeElement)"), "an ask that took the focus wins over a kept place");
  assert.ok(chat.includes('tr("studio-chat-jump-newest")') && chat.includes("place.keep(null)"), "the door back down keeps no place");
  const hook = src("../views/_studio/useThreadPlace.ts");
  assert.ok(hook.includes("threadPlaces.keepQuietly(") && hook.includes("parseThreadPlace(threadPlaces.read("), "kept quietly, read back through the model");
  const messages = src("../views/_studio/useScopeMessages.ts");
  assert.ok(messages.includes("threadOf(key)") && messages.includes("keepThread(held.of, held.thread)") && messages.includes("joinNewest({"), "a thread remounts with what it had, and its newest page joins");
  assert.ok(messages.includes("useReloadOnReconnect("), "and still reads again when the bus comes back");
  const store = src("../views/_studio/threadsStore.ts");
  assert.ok(!/localStorage|sessionStorage|webStorage/.test(store), "the messages are the window's, never the storage's");
});

test("one beat everywhere, one store door, and every screen shows a thread through the one component", () => {
  const inbox = src("../views/Inbox.tsx");
  assert.ok(inbox.includes('import { READ_AFTER_MS } from "./_studio/readModel.mjs";') && !inbox.includes("READ_ON_SELECT_MS"), "the Inbox's beat for a row without a conversation is the model's");
  const store = src("../shell/useWorkspaceData.ts");
  for (const fact of ["rowRead(rows, scope)", "hostedRead(prev, host, scope)", "api.markHostedRead(host, scope)", "markRead: (scope: string, host?: string | null) => void;"]) {
    assert.ok(store.includes(fact), `the store ${fact}`);
  }
  for (const screen of ["../views/_workbench/AgentPane.tsx", "../views/_workbench/AgentModeCenter.tsx", "../views/_studio/ConversationSurface.tsx", "../views/Hosted.tsx", "../views/_goal/GoalConversationPane.tsx", "../views/Channels.tsx", "../views/Messages.tsx", "../views/ConversationDoor.tsx", "../views/Inbox.tsx"]) {
    const text = src(screen);
    assert.ok(text.includes("<Conversation") || text.includes("<Chat") || text.includes("<ConversationThread") || text.includes("<ConversationSurface"), `${screen} shows its thread through the one component`);
    assert.ok(!/\bws\.markRead\(|\bmarkRead\(/.test(text.replace(/setRead\(|markAllRead/g, "")) || screen.endsWith("Inbox.tsx"), `${screen} marks nothing read itself`);
  }
  assert.ok(!inbox.includes("ws.markRead("), "the Inbox's own verbs go through the API, its thread through the hook");
});
