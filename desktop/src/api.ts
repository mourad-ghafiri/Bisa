/**
 * Typed client for the node's HTTP API.
 *
 * One function per route, shapes from `types.ts` (generated where the Rust
 * derives `JsonSchema`). Every call takes an optional `AbortSignal` so a view
 * that unmounts mid-flight cancels rather than setting state on a dead
 * component.
 */

import { API_READ_TIMEOUT_MS, DEFAULT_BASE, answerUnreadableWords, baseOf, bearer, codeOf, detailOf, errorDetail, failureOf, isServerError, looseRefusal, refusalOf, streamEndedWords, takesDeadline, tokenToKeep } from "./apiModel.mjs";
import { locale, t as tr } from "./i18n/l10n.mjs";
import type { FailureKind } from "./apiModel.mjs";
import { errorFields, log } from "./log";
import type {
  CreateSimulator,
  MobileDevice,
  MobileDevelopmentProject,
  MobileDevelopmentShot,
  MobileDevelopmentStatus,
  ConversationOrigin,
  ConversationsResponse,
  ConversationView,
  ChangesView,
  FileReviewView,
  Settled,
  AnswerAskBody,
  AsksResponse,
  LiveTurnsResponse,
  CrashReportView,
  GitConflict,
  GitMergePreview,
  GitOperationFacts,
  GitResolution,
  LogsView,
  NetworkCheck,
  UpdateCheck,
  NetworkStatus,
  NodeInfo,
  PasteboardHoldings,
  Pasted,
  PasteReport,
  RunStatus,
  RunSummary,
  RunView,
  StopOutcome,
  DecideBody,
  Home,
  McpProbeReport,
  McpHealthView,
  InitRepositoryReply,
  Readiness,
  MeBody,
  PersonBody,
  NewInviteBody,
  JoinHostBody,
  HostedPostBody,
  NewGoalBody,
  StopBody,
  CloseGoalBody,
  SetWorkflowBody,
  PutWorkflowBody,
  ListeningBody,
  GoalListeningBody,
  StartRunBody,
  NewChannelBody,
  PatchChannelBody,
  NewMessageBody,
  NewAgentBody,
  PatchAgentBody,
  TeamBody,
  TeamPatchBody,
  NewSkillBody,
  PatchSkillBody,
  AddonPatchBody,
  NewMcpBody,
  PatchMcpBody,
  ProbeMcpBody,
  PatchProjectBody,
  PatchWorkstreamBody,
  GitConfigWrite,
  ReviewNoteCreate,
  NewConversationBody,
  PatchConversationBody,
  SettleChangesBody,
  NewWorkstreamBody,
  PrBody,
  PrReview,
  Inspect,
  SessionsStopped,
} from "./types";
import type {
  CommitSummary,
  GitMergeMode,
  GitRebasePlan,
  AgentDef,
  PetDef,
  Addon,
  Addons,
  AddonPermission,
  AddonProblems,
  AddonOffers,
  AddonFetchResult,
  Answer,
  PlaceWorkstreamBody,
  FolderRepo,
  RepoCommitRow,
  PullOutcomeBody,
  SuggestedNotesMessage,
  DrawingRow,
  DrawingDetail,
  DrawingPending,
  DrawResult,
  NewDrawingBody,
  PatchDrawingBody,
  CacheStatsDto,
  NewNoteBody,
  NoteRow,
  PatchNoteBody,
  AttachmentRef,
  BudgetSpent,
  GoalDocumentRow,
  GoalPlan,
  Retired,
  Retirement,
  WorkflowPlan,
  CatalogEntry,
  CatalogKind,
  ChannelDef,
  ChannelListEntry,
  DecideOutcome,
  DeciderStatus,
  DecisionRequest,
  DecisionResponse,
  JudgementRecord,
  FileContent,
  FileScope,
  FileTree,
  HarnessRow,
  HarnessUsage,
  GuardPreview,
  RedactPreview,
  SecurityStatus,
  InboxRow,
  Installed,
  Goal,
  Governance,
  IdeFile,
  FileSides,
  CommitFileDiff,
  CommitFileSides,
  ResolvedSetting,
  SettingDef,
  SettingScope,
  GoalRow,
  GoalView,
  Disposal,
  GuidanceInfo,
  McpServerView,
  MemberRow,
  MessageRow,
  ArtifactListRow,
  Effort,
  ModelHealthRow,
  ModelInfo,
  NewProjectBody,
  NewWorkflowBody,
  Problem,
  Workflow,
  WorkflowRow,
  WorkflowRun,
  Placement,
  Project,
  WorkstreamCommitted,
  WorkstreamAmended,
  ProjectCreated,
  ProjectDetail,
  WorkstreamFileDiff,
  WorkstreamGitFiles,
  WorkstreamBlame,
  WorkstreamCommitView,
  BranchInfo,
  RemoteBranchInfo,
  TagInfo,
  RemoteInfo,
  GitInProgress,
  PullMode,
  PullOutcome,
  GitRecoveryRef,
  GitStash,
  GitStashPush,
  Recovery,
  GitDone,
  WorkstreamStatus,
  WorkstreamCheckout,
  CodeHostCapabilities,
  PullRequest,
  CheckRun,
  MergeStrategy,
  ReviewSummary,
  ReviewThread,
  RepoRef,
  LspServerStatus,
  WorkstreamHistory,
  GraphMatches,
  GraphRefScope,
  GraphWindow,
  SearchHit,
  SearchParams,
  SearchSummary,
  ProjectReviewNotes,
  ReviewNote,
  ReviewNotesSent,
  ProjectRow,
  WorkstreamStaged,
  WorkstreamGitStatus,
  PrOutcome,
  PulseCursor,
  PulsePage,
  PushOutcome,
  ReactionRow,
  RecallRecord,
  Reference,
  SessionRow,
  SessionsResponse,
  SkillDef,
  SuggestedCommitMessage,
  SuggestedPullRequest,
  TagFacet,
  TeamDef,
  WorkItemDetail,
  WorkItemRef,
  Workstream,
  WorkstreamDetail,
  WorkstreamDiff,
  WorkstreamRef,
  WorkspaceInfo,
  Directory,
  Hosted,
  HostedChannelRow,
  HostedMessage,
  HostedPosted,
  Invite,
  MemberRole,
  PermissionRow,
  PersonRow,
  RelayCheck,
  RoleRow,
  SyncReport,
  AccountsView,
  AccountCheck,
  ConnectorAccountRow,
  ConnectorDefinition,
  ConnectorDetail,
  ConnectorRow,
  ConnectorValidation,
  NewConnectorAccount,
  OAuthStart,
  CodeHostConnection,
  CodeHostHealth,
  CodeHostKind,
  LoginPlan,
  RemoteInspection,
  ConnectionCheck,
  GitProfileView,
  GitProfilesView,
  HostGreeting,
  ProfileSpec,
  RepoConnection,
  SshNewKey,
  SshOverview,
  SshPublicKey,
  SshResolved,
  WorkstreamScriptsView,
  RunCommand,
  ServedFolder,
  BrowserPending,
  BrowserResult,
  GitIdentityView,
  CommitterView,
  GitConfigView,
  HookSecret,
  ListenerView,
  SignalView,
} from "./types";


let apiBase: string | null = null;

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Resolve the API base once: env override → Tauri sidecar → default port. */
export async function resolveApiBase(): Promise<string> {
  if (apiBase) return apiBase;
  const env = (import.meta.env.BISA_API_BASE ?? import.meta.env.VITE_API_BASE) as
    | string
    | undefined;
  if (env) {
    apiBase = baseOf(env, null);
    return apiBase;
  }
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      apiBase = baseOf(null, await invoke<string>("api_base"));
      nodeFailure = null;
      return apiBase;
    } catch (e) {
      // The shell has no node yet: every call is offline until
      // `node:restarted` drops this base and the next call asks again.
      nodeFailure = e instanceof Error ? e.message : String(e);
      log.error("shell", "the shell has no node", { reason: nodeFailure });
      apiBase = DEFAULT_BASE;
      return apiBase;
    }
  }
  apiBase = DEFAULT_BASE;
  return apiBase;
}

/** Why the shell has no node, as it said — `null` while one is running or was never asked for. */
let nodeFailure: string | null = null;

export function nodeFailureReason(): string | null {
  return nodeFailure;
}

export function apiBaseSync(): string {
  return apiBase ?? DEFAULT_BASE;
}

/**
 * Drop the cached base so the next call resolves it again — what the shell
 * does when the sidecar says the node restarted: the watchdog keeps the
 * port, a person's *Restart* may not, and a page that kept the old one would
 * talk to a dead port for the rest of its life.
 */
export function forgetApiBase(): void {
  apiBase = null;
  // The token goes with it: a first start that failed minted none, and the
  // restart that worked did.
  apiToken = null;
}

let apiToken: string | null = null;

/**
 * The control-plane token every request carries. From the environment in
 * UI-only dev (`BISA_API_TOKEN=… npm run dev`, the value in the node's
 * `run/token`), from the shell under Tauri (which minted it and handed it to
 * its sidecar). Absent, requests still go out and come back `401`, which the
 * error path shows as what it is.
 */
export async function resolveApiToken(): Promise<string> {
  if (apiToken !== null) return apiToken;
  const env = tokenToKeep(import.meta.env.BISA_API_TOKEN);
  if (env !== null) {
    apiToken = env;
    return apiToken;
  }
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    // Kept only once the shell has one (`apiModel.tokenToKeep`): asked again otherwise.
    apiToken = tokenToKeep(await invoke<string>("api_token"));
    return apiToken ?? "";
  }
  apiToken = "";
  return apiToken;
}

function apiTokenSync(): string {
  return apiToken ?? "";
}

/**
 * A URL something other than `fetch` will load — an `<img>`, an
 * `EventSource` — with the token in the query, because neither can send a
 * header. Loopback only, and the token is the app's own.
 */
export function withToken(url: string): string {
  const token = apiTokenSync();
  if (!token) return url;
  return `${url}${url.includes("?") ? "&" : "?"}token=${encodeURIComponent(token)}`;
}

/**
 * A one-shot SSE stream over a route: every `data:` frame is parsed and
 * handed to `onFrame`, which either keeps reading, `finish`es with a value,
 * or `fail`s. The token rides in the query, as `bus.ts` does, because an
 * EventSource cannot send a header. Aborting resolves with `partial` — a
 * search cancelled by the next keystroke is not an error.
 */
export function sse<Frame, Out>(
  path: string,
  onFrame: (frame: Frame, finish: (out: Out) => void, fail: (e: Error) => void) => void,
  partial: Out,
  signal?: AbortSignal,
): Promise<Out> {
  return new Promise<Out>((resolve, reject) => {
    const source = new EventSource(withToken(`${apiBaseSync()}${path}`));
    let settled = false;
    const close = () => {
      source.close();
      signal?.removeEventListener("abort", onAbort);
    };
    const finish = (out: Out) => {
      if (settled) return;
      settled = true;
      close();
      resolve(out);
    };
    const fail = (e: Error) => {
      if (settled) return;
      settled = true;
      close();
      reject(e);
    };
    const onAbort = () => finish(partial);
    if (signal?.aborted) {
      close();
      resolve(partial);
      return;
    }
    signal?.addEventListener("abort", onAbort);
    source.onmessage = (ev) => {
      try {
        onFrame(JSON.parse(ev.data as string) as Frame, finish, fail);
      } catch (e) {
        fail(e instanceof Error ? e : new Error(String(e)));
      }
    };
    // The server closing the stream after `done` also fires `onerror`; by
    // then `finish` has run and this is a no-op. A stream that ends before
    // its last frame — the node went away, or was asked to stop — did not
    // finish, in the model's words.
    source.onerror = () => fail(new Error(streamEndedWords()));
  });
}

function authHeaders(extra?: Record<string, string>): Record<string, string> {
  const token = apiTokenSync();
  // The language the window speaks: the node renders its own sentences —
  // an error body's — in it (17 — Internationalisation).
  const headers: Record<string, string> = { ...extra, "accept-language": locale() };
  return token ? { ...headers, authorization: bearer(token) } : headers;
}

// ---------------------------------------------------------------------------
// The desktop shell
//
// Two things the node cannot do because they are not on the other end of a
// socket: a native folder picker and showing a path in the file manager. Both
// are Tauri *plugin* commands, which — unlike the shell's own commands — are
// permission-gated, so each also needs its line in
// `src-tauri/capabilities/default.json` or it fails silently at runtime.
//
// A browser dev session has neither, and the rule is that it degrades to
// something that still works (a path you type) rather than to a button that
// does nothing when clicked. That is what [`inDesktopShell`] is for: callers
// ask before they offer.
// ---------------------------------------------------------------------------

/** Running inside the Tauri shell, rather than a plain browser. */
export function inDesktopShell(): boolean {
  return isTauri();
}

/**
 * Put `text` on the shell's clipboard; `true` when it landed. A browser has
 * no shell and answers `false`, so the kit's clipboard door falls back to the
 * webview's own. The shell's clipboard needs no user gesture — which is why
 * the door prefers it: a menu's chosen action runs after the menu has left.
 */
export async function setClipboardText(text: string): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("copy_text", { text });
    return true;
  } catch (e) {
    log.warn("shell", "the shell declined the clipboard", errorFields(e));
    return false;
  }
}

/**
 * Put a PNG on the shell's clipboard as a picture; `true` when it landed —
 * `setClipboardText`'s twin, for the browser's screenshot (`copy_image`).
 */
export async function setClipboardImage(png: Uint8Array): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("copy_image", { png: Array.from(png) });
    return true;
  } catch (e) {
    log.warn("shell", "the shell declined the clipboard's picture", errorFields(e));
    return false;
  }
}

/**
 * Put the window's own chrome on the side the page landed on, and the
 * mounted family's material behind the page — the OS's under-window effect
 * for glass, nothing for an opaque family. A browser has no window to move
 * and skips it; a shell that refuses (an older OS) is not worth an error —
 * the page is already the right colour, and glass over nothing is still
 * glass.
 */
export async function setWindowAppearance(scheme: "light" | "dark", material: "opaque" | "glass"): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_window_appearance", { scheme, material });
  } catch (e) {
    log.warn("shell", "the shell declined the window appearance", errorFields(e));
  }
}

/**
 * Ask the OS for a folder. `null` when the person cancelled — and `null` in a
 * browser, where there is no picker to ask.
 *
 * The plugin import is dynamic on purpose: `@tauri-apps/plugin-dialog` reaches
 * for `window.__TAURI_INTERNALS__`, and a static import would drag that into
 * the bundle a browser session loads and runs.
 */
export async function pickFolder(title: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({ directory: true, multiple: false, title });
  return typeof chosen === "string" ? chosen : null;
}

/**
 * Show a path in the OS file manager.
 *
 * *Reveal*, not *open*: opening hands the path to whichever application claims
 * it, and a project root is a directory somebody wants to look at. This is what
 * replaces "copy the checkout path and open it yourself".
 */
/**
 * Hand the person a file to save: under the desktop shell a native save
 * dialog and a write to the path they chose (a machine capability, so it is
 * the shell's); in a browser a download. Returns the saved path, or `null`
 * when they cancelled.
 */
export async function exportFile(
  defaultName: string,
  mime: string,
  bytes: Uint8Array,
): Promise<string | null> {
  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: defaultName });
    if (!path) return null;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("export_file", { path, bytes: Array.from(bytes) });
    return path;
  }
  const url = URL.createObjectURL(new Blob([bytes as unknown as BlobPart], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = defaultName;
  a.click();
  URL.revokeObjectURL(url);
  return defaultName;
}

/**
 * Open an http(s) URL in the person's browser, through the desktop's opener
 * (a scoped permission: `opener:allow-open-url`, http and https only). In a
 * browser dev session a new tab, never the same one. Anything else is refused.
 */
export async function openExternal(url: string): Promise<void> {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error(`${url} is not a URL.`);
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error(`Only http and https links open from here, not ${parsed.protocol}`);
  }
  if (!isTauri()) {
    window.open(parsed.toString(), "_blank", "noopener,noreferrer");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(parsed.toString());
}

/**
 * Open the diagnostic log's folder in the file manager — the crash card's
 * way out, from any screen. In the desktop shell the shell knows the folder
 * on its own (`folders.rs`: the file it writes, else what the binary named),
 * so this works with no node — the node may be what is not there. Off the
 * shell the node names the folder.
 */
export async function revealLog(): Promise<void> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("reveal_logs_dir");
    return;
  }
  const view = await api.logs();
  await revealPath(view.dir);
}

/** Open the workspace's data folder in the file manager — the shell's own knowledge, no node needed. */
export async function revealDataFolder(): Promise<void> {
  if (!isTauri()) throw new Error("Showing a folder needs the desktop app.");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("reveal_data_dir");
}

export async function revealPath(path: string): Promise<void> {
  if (!isTauri()) throw new Error("Showing a folder needs the desktop app.");
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

// ---------------------------------------------------------------------------
// Loose files (ide/03 §Loose files)
//
// A file anywhere on this machine, edited in the Project IDE under no project,
// goal or group. Its blast radius is the machine, so the shell reads and
// writes it (ide/01) — the node never learns an absolute path. The shell keeps
// the node's editor rules byte for byte: the same caps, the same sha256 hash,
// compare-and-swap saves; its refusals are raised here as the same
// `ApiError`s the node's routes produce, so the editor merges a loose
// conflict exactly as it merges a root file's.
// ---------------------------------------------------------------------------

/** One file from this machine, in the shape `GET /ide/file` answers. */
export interface LooseFile {
  /** The canonical absolute path — what a save names. */
  path: string;
  name: string;
  size: number;
  binary: boolean;
  truncated: boolean;
  editable: boolean;
  text?: string | null;
  hash?: string | null;
}

/** The shell's tagged refusal, as the same status and body the node would answer (`apiModel.looseRefusal`): a conflict's 409, a missing file's 404, a file over the bound's 413 with its size and limit. */
function looseError(e: unknown, path: string): ApiError {
  const refusal = looseRefusal(e, tr("app-api-shell-refused"));
  return new ApiError(refusal.message, refusal.status, path, refusal.body);
}

/** Read a file from this machine for the editor. Needs the desktop app. */
export async function looseFile(path: string): Promise<LooseFile> {
  if (!isTauri()) throw new ApiError(tr("app-api-opening-file-from-machine-needs-desktop"), 400, path);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<LooseFile>("read_loose_file", { path });
  } catch (e) {
    throw looseError(e, path);
  }
}

/**
 * Compare-and-swap save to a path on this machine; no `baseHash` creates and
 * refuses an existing file. A 409 carries `current_hash` and `current_text`.
 */
export async function writeLooseFile(path: string, text: string, baseHash?: string | null): Promise<{ path: string; hash: string; created: boolean }> {
  if (!isTauri()) throw new ApiError(tr("app-api-saving-file-machine-needs-desktop-app"), 400, path);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<{ path: string; hash: string; created: boolean }>("write_loose_file", { path, text, baseHash: baseHash ?? null });
  } catch (e) {
    throw looseError(e, path);
  }
}

/**
 * The absolute paths of the files the drag that just ended carried — asked
 * right after the DOM's `drop`, whose `File`s have no path. Empty where the
 * shell cannot tell (another platform, or not a file drag).
 */
export async function droppedPaths(): Promise<string[]> {
  if (!isTauri()) return [];
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string[]>("dropped_paths");
  } catch (e) {
    log.warn("shell", "the shell could not read the drop", errorFields(e));
    return [];
  }
}

const NOTHING_HELD: PasteboardHoldings = { paths: [], image: false };

/**
 * What the machine's clipboard holds that a paste can take: the absolute
 * paths of the files a copy in the file manager put there, and whether a
 * picture is there. Nothing where the shell cannot tell (another platform,
 * a clipboard holding text alone) and outside the shell.
 */
export async function pasteboardHolds(): Promise<PasteboardHoldings> {
  if (!isTauri()) return NOTHING_HELD;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<PasteboardHoldings>("pasteboard_holds");
  } catch (e) {
    log.warn("shell", "the shell could not read the clipboard", errorFields(e));
    return NOTHING_HELD;
  }
}

/**
 * The picture the machine's clipboard holds, as PNG bytes — the shell's
 * read, for a picture the web engine does not hand a paste event. Null
 * when there is none, and outside the shell.
 */
export async function pasteboardImage(): Promise<Uint8Array | null> {
  if (!isTauri()) return null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const bytes = await invoke<ArrayBuffer | Uint8Array | number[]>("pasteboard_image");
    const out = bytes instanceof Uint8Array ? bytes : bytes instanceof ArrayBuffer ? new Uint8Array(bytes) : Uint8Array.from(bytes);
    return out.length ? out : null;
  } catch (e) {
    log.warn("shell", "the shell could not read the clipboard's picture", errorFields(e));
    return null;
  }
}

/**
 * Write the clipboard's picture into a folder of a checkout under `name`,
 * by absolute path, in the shell (ide/01; the node's watcher announces what
 * lands). The shell reads the picture as it writes, and refuses a taken or
 * improper name in a sentence. Needs the desktop app.
 */
export async function pasteImageInto(dest: string, name: string): Promise<Pasted> {
  if (!isTauri()) throw new ApiError(tr("app-api-pasting-picture-from-clipboard-needs-desktop"), 400, dest);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<Pasted>("paste_image_into", { dest, name });
  } catch (e) {
    throw new ApiError(e instanceof Error ? e.message : String(e), 400, dest);
  }
}

/**
 * Write the clipboard's picture to a file of its own in the machine's
 * temporary folder, under `name` (Finder's `name 2` when taken), and answer
 * its absolute path — what a terminal types for a pasted screenshot (ide/06),
 * so a shell or a harness opens it as it would a copied file. Needs the
 * desktop app.
 */
export async function pasteImageToTemp(name: string): Promise<string> {
  if (!isTauri()) throw new ApiError(tr("app-api-pasting-picture-from-clipboard-needs-desktop"), 400, name);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<string>("paste_image_to_temp", { name });
  } catch (e) {
    throw new ApiError(e instanceof Error ? e.message : String(e), 400, name);
  }
}

/**
 * Copy files from this machine into a folder of a checkout, by absolute
 * path, in the shell (ide/01: the machine's capability; the node's watcher
 * announces what lands). Answers what was pasted, renamed and left out.
 * Needs the desktop app.
 */
export async function pasteInto(sources: string[], dest: string): Promise<PasteReport> {
  if (!isTauri()) throw new ApiError(tr("app-api-pasting-files-from-machine-needs-desktop"), 400, dest);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<PasteReport>("paste_into", { sources, dest });
  } catch (e) {
    throw new ApiError(e instanceof Error ? e.message : String(e), 400, dest);
  }
}

/** Ask the OS for one or more files. `[]` when the person cancelled, and in a browser. */
export async function pickFiles(title: string): Promise<string[]> {
  if (!isTauri()) return [];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({ directory: false, multiple: true, title });
  if (Array.isArray(chosen)) return chosen.filter((c): c is string => typeof c === "string");
  return typeof chosen === "string" ? [chosen] : [];
}

/** Ask the OS where to save: the native dialog with `defaultPath`; `null` when cancelled, and in a browser. */
export async function pickSavePath(defaultPath: string, title?: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const chosen = await save({ defaultPath, title });
  return typeof chosen === "string" ? chosen : null;
}

/**
 * Open an artifact's named copy with whatever application claims it (ide/12).
 * A machine capability, so it is the shell's; the shell refuses any path
 * that is not a named copy inside the store.
 */
export async function openArtifactFile(path: string): Promise<void> {
  if (!isTauri()) throw new Error("Opening a file needs the desktop app.");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_artifact", { path });
}

/**
 * Save an artifact where the person chooses: the save dialog, then the shell
 * copies the named copy there — no bytes cross the bridge. Answers the path,
 * or `null` when they cancelled; in a browser, a download of the bytes.
 */
export async function saveArtifactCopy(
  from: string,
  defaultName: string,
  bytes: () => Promise<Uint8Array>,
  mime: string,
): Promise<string | null> {
  if (!isTauri()) return exportFile(defaultName, mime, await bytes());
  const { save } = await import("@tauri-apps/plugin-dialog");
  const to = await save({ defaultPath: defaultName });
  if (!to) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("copy_artifact", { from, to });
  return to;
}

/** Every failure the client can produce, with the node's message when it gave one. */
export class ApiError extends Error {
  /**
   * Which refusal, when the node named one (`ErrorBody.code`): a `409` stands
   * for several things, and a client switches on this, never on the status.
   */
  readonly code: string | null;

  constructor(
    message: string,
    readonly status: number,
    readonly path: string,
    /** The parsed error body, when the node sent one — a `409` carries what it lost the race to. */
    readonly body?: unknown,
    /** Why there was no answer at all, when `status` is 0 (`apiModel.failureOf`). */
    readonly failure: FailureKind | null = null,
  ) {
    super(message);
    this.name = "ApiError";
    this.code = codeOf(body);
    this.detail = detailOf(body);
    this.refusal = refusalOf(body);
  }

  /** The typed facts behind `code`, when the node sent any (a conflict's paths, a refused fast-forward's counts). */
  readonly detail: Record<string, unknown> | null;

  /**
   * Which sentence the refusal is — the id of the message it travels as
   * (`ErrorBody.text.id`), the same in every language. A form that places a
   * refusal beside the input it is about switches on this; the words
   * (`message`) are for the person and are never matched.
   */
  readonly refusal: string | null;

  /**
   * The node is unreachable (not running, or still starting). A read that
   * outstayed its deadline is not this: the node is up, and only that read
   * is degraded.
   */
  get offline(): boolean {
    return this.status === 0 && this.failure !== "timeout";
  }

  /** The read outstayed `API_READ_TIMEOUT_MS`; the last value stands. */
  get timedOut(): boolean {
    return this.failure === "timeout";
  }
}

/** Per-request switches beyond the body and the signal. */
interface ReqOptions {
  /** Let the request outlive the page — a save on the way out. */
  keepalive?: boolean;
}

/**
 * A read's bound: one signal that ends when the caller leaves or when the
 * deadline runs out, and which of the two it was. Built by hand: the static
 * that joins two signals came with WebKit 17.4 and the one that makes a
 * deadline with WebKit 16, and the bundle runs from macOS 11 on.
 */
interface Bound {
  signal: AbortSignal;
  /** Whether the deadline — not the caller — ended the read. */
  timedOut: () => boolean;
  /** The read is over: the clock and the caller's listener are let go. */
  release: () => void;
}

function boundOf(caller: AbortSignal | undefined, ms: number): Bound {
  const ac = new AbortController();
  let late = false;
  const leave = () => ac.abort();
  const timer = setTimeout(() => {
    late = true;
    ac.abort();
  }, ms);
  if (caller?.aborted) ac.abort();
  else caller?.addEventListener("abort", leave, { once: true });
  return {
    signal: ac.signal,
    timedOut: () => late,
    release: () => {
      clearTimeout(timer);
      caller?.removeEventListener("abort", leave);
    },
  };
}

async function req<T>(
  method: string,
  path: string,
  body?: unknown,
  signal?: AbortSignal,
  options?: ReqOptions,
): Promise<T> {
  const base = await resolveApiBase();
  await resolveApiToken();
  // Every read is given a deadline (`apiModel.takesDeadline`), beside the
  // caller's own signal when there is one: a list that never answers becomes
  // one named degraded read, never a pane that waits for the rest of the
  // session. A write takes as long as it needs.
  const bound = takesDeadline(method) ? boundOf(signal, API_READ_TIMEOUT_MS) : null;
  try {
    let res: Response;
    let text = "";
    try {
      res = await fetch(`${base}${path}`, {
        method,
        headers: authHeaders(body === undefined ? undefined : { "content-type": "application/json" }),
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: bound?.signal ?? signal,
        keepalive: options?.keepalive,
      });
      // The body is read under the same bound, once, as text: a node that
      // sent its status line and stopped is as silent as one that sent
      // nothing. A refusal whose body could not be read keeps its status line.
      if (!res.ok) text = await res.text().catch(() => "");
      else if (res.status !== 204) text = await res.text();
    } catch (e) {
      const failure = failureOf(e, signal?.aborted === true, bound?.timedOut() ?? false, API_READ_TIMEOUT_MS);
      // The caller gave up: nothing to say, the error is theirs.
      if (failure.kind === "aborted") throw e;
      if (failure.kind === "timeout") log.warn("api", "the node did not answer in time", { method, path, reason: failure.reason });
      else log.error("api", "the node did not answer", { method, path, reason: failure.reason, detail: failure.detail ?? null });
      throw new ApiError(failure.reason, 0, path, undefined, failure.kind);
    }
    if (!res.ok) {
      // JSON when it parses, else its first line is the sentence
      // (`apiModel.errorDetail`) — never discarded.
      let parsedBody: unknown;
      try {
        parsedBody = text ? (JSON.parse(text) as unknown) : undefined;
      } catch {
        parsedBody = undefined;
      }
      const detail = errorDetail(res.status, res.statusText, parsedBody, text);
      // A refusal (4xx) is the caller's to show; a malfunction (5xx) is the
      // log's. The detail is the node's own sentence, never the body.
      if (isServerError(res.status)) log.error("api", "the node answered a server error", { method, path, status: res.status, detail });
      throw new ApiError(detail, res.status, path, parsedBody);
    }
    if (res.status === 204) return undefined as T;
    try {
      return JSON.parse(text) as T;
    } catch (e) {
      // The node answered, and what it said is no answer: the node is
      // there — never *unreachable* — and the fault is the log's.
      log.error("api", "the node's answer could not be read", { method, path, status: res.status, bytes: text.length, ...errorFields(e) });
      throw new ApiError(answerUnreadableWords(), res.status, path);
    }
  } finally {
    bound?.release();
  }
}

const get = <T>(p: string, s?: AbortSignal) => req<T>("GET", p, undefined, s);
const post = <T>(p: string, b?: unknown, s?: AbortSignal) => req<T>("POST", p, b ?? {}, s);
const put = <T>(p: string, b: unknown, s?: AbortSignal, o?: ReqOptions) => req<T>("PUT", p, b, s, o);
const patch = <T>(p: string, b: unknown, s?: AbortSignal) => req<T>("PATCH", p, b, s);
const del = <T>(p: string, b?: unknown, s?: AbortSignal) => req<T>("DELETE", p, b, s);

/// `?tag=` for a list route. Repeatable, matching the node's extractor; an
/// empty list means no filter rather than "match nothing".
const tagQuery = (tags?: string[]): string =>
  tags?.length ? `?${tags.map((t) => `tag=${encodeURIComponent(t)}`).join("&")}` : "";

/**
 * Where a page continues from: the oldest row shown — its moment in whole
 * seconds and, so two rows posted within one second are never split, its id.
 */
export interface PageBefore {
  at: number;
  id?: string;
}

const page = (before?: PageBefore, limit?: number): string => {
  const q = new URLSearchParams();
  if (before !== undefined) {
    q.set("before", String(before.at));
    if (before.id) q.set("before_id", before.id);
  }
  if (limit !== undefined) q.set("limit", String(limit));
  const s = q.toString();
  return s ? `?${s}` : "";
};

/**
 * The four things `GET /usage/{kind}/{id}` can be asked about — the same four
 * whose `DELETE` refuses while something still points at them.
 *
 * Declared here rather than imported: the node's `UsageKind` does not derive
 * `JsonSchema` (it only ever appears in a path segment), so there is nothing
 * generated to import, and a `string` parameter would let a typo reach the
 * node as a 400 the caller never anticipated.
 */
export type UsageKind = "agent" | "team" | "skill" | "mcp";

/**
 * A folder repository's routes — the notes' under `/notes/git`, the
 * drawings' under `/drawings/git` — as one shape the repository strip takes
 * (`shell/repo/RepoStrip.tsx`), so the strip is one component for both.
 */
export interface RepoApi {
  status: (s?: AbortSignal) => Promise<FolderRepo>;
  commit: (message: string, s?: AbortSignal) => Promise<RepoCommitRow>;
  suggest: (s?: AbortSignal) => Promise<SuggestedNotesMessage>;
  push: (s?: AbortSignal) => Promise<FolderRepo>;
  fetch: (s?: AbortSignal) => Promise<FolderRepo>;
  /** Fast-forward only, through the consented tier; the drawings' repository answers 404 — it offers no pull. */
  pull: (s?: AbortSignal) => Promise<PullOutcomeBody>;
  setRemote: (url: string, s?: AbortSignal) => Promise<FolderRepo>;
  setIdentity: (name: string, email: string, s?: AbortSignal) => Promise<FolderRepo>;
}

function repoApi(prefix: "/notes/git" | "/drawings/git"): RepoApi {
  return {
    status: (s) => get<FolderRepo>(prefix, s),
    commit: (message, s) => post<RepoCommitRow>(`${prefix}/commit`, { message }, s),
    suggest: (s) => post<SuggestedNotesMessage>(`${prefix}/message`, {}, s),
    push: (s) => post<FolderRepo>(`${prefix}/push`, {}, s),
    fetch: (s) => post<FolderRepo>(`${prefix}/fetch`, {}, s),
    pull: (s) => post<PullOutcomeBody>(`${prefix}/pull`, {}, s),
    setRemote: (url, s) => put<FolderRepo>(`${prefix}/remote`, { url }, s),
    setIdentity: (name, email, s) => put<FolderRepo>(`${prefix}/identity`, { name, email }, s),
  };
}

export const api = {
  // --- health / workspace -------------------------------------------------
  health: (s?: AbortSignal) => get<{ ok: boolean; version: string }>("/health", s),
  /// `GET /updates` — the latest release of the platform as GitHub lists it, held by the node for `cache.updates.ttl_ms`; `refresh` asks GitHub again. Facts, never a comparison: the desktop knows its own version (`shell/updateModel.mjs`).
  updateCheck: (refresh = false, s?: AbortSignal) => get<UpdateCheck>(`/updates${refresh ? "?refresh=true" : ""}`, s),
  /// `GET /node` — what this node is: the process, where it listens, where the workspace lives, whether the engine is paused, the sessions live.
  nodeInfo: (s?: AbortSignal) => get<NodeInfo>("/node", s),
  workspace: (s?: AbortSignal) => get<WorkspaceInfo>("/workspace", s),

  // --- collaboration (14-collaboration) -----------------------------------
  /// The four roles with their permissions, and every permission with its words.
  roles: (s?: AbortSignal) => get<{ roles: RoleRow[]; permissions: PermissionRow[] }>("/workspace/roles", s),
  /// The people hosted here — every member but the owner.
  people: (s?: AbortSignal) => get<{ people: PersonRow[] }>("/workspace/people", s),
  /// Your own profile (14-collaboration): what to be called and your face — kept
  /// when absent, cleared with `null`; the face a small picture this machine
  /// holds. Every workspace this node is a member of is told.
  setMe: (body: MeBody, s?: AbortSignal) =>
    put<{ me: MemberRow }>("/workspace/me", body, s),
  /// Admit a person by pubkey without an invitation.
  addPerson: (body: PersonBody & { role?: MemberRole }, s?: AbortSignal) =>
    post<{ person: MemberRow; people: PersonRow[] }>("/workspace/people", body, s),
  setPersonRole: (pubkey: string, role: MemberRole, s?: AbortSignal) =>
    put<{ person: MemberRow; people: PersonRow[] }>(`/workspace/people/${pubkey}/role`, { role }, s),
  removePerson: (pubkey: string, s?: AbortSignal) => del<{ removed: string; people: PersonRow[] }>(`/workspace/people/${pubkey}`, undefined, s),
  invites: (s?: AbortSignal) => get<{ invites: Invite[] }>("/workspace/invites", s),
  /// Make a single-use invitation; the answer carries the link and the code, and nothing else ever does.
  createInvite: (body: NewInviteBody & { role?: MemberRole }, s?: AbortSignal) =>
    post<{ invite: Invite; link: string; code: string }>("/workspace/invites", body, s),
  revokeInvite: (id: string, s?: AbortSignal) => del<{ invite: Invite }>(`/workspace/invites/${id}`, undefined, s),
  admitInvite: (id: string, s?: AbortSignal) => post<{ invite: Invite }>(`/workspace/invites/${id}/admit`, {}, s),
  refuseInvite: (id: string, s?: AbortSignal) => post<{ invite: Invite }>(`/workspace/invites/${id}/refuse`, {}, s),
  /// The workspaces this node is a guest of.
  hosts: (s?: AbortSignal) => get<{ hosts: Hosted[] }>("/hosts", s),
  joinHost: (body: JoinHostBody, s?: AbortSignal) => post<{ host: Hosted }>("/hosts/join", body, s),
  leaveHost: (host: string, s?: AbortSignal) => del<{ left: string }>(`/hosts/${host}`, undefined, s),
  hostedChannels: (host: string, s?: AbortSignal) => get<{ channels: HostedChannelRow[] }>(`/hosts/${host}/channels`, s),
  hostedDms: (host: string, s?: AbortSignal) => get<{ dms: HostedChannelRow[] }>(`/hosts/${host}/dms`, s),
  hostedMembers: (host: string, s?: AbortSignal) => get<{ members: Directory[] }>(`/hosts/${host}/members`, s),
  hostedMessages: (host: string, scope: string, before?: PageBefore, limit?: number, s?: AbortSignal) =>
    get<{ scope: string; messages: HostedMessage[] }>(`/hosts/${host}/channels/${scope}/messages${page(before, limit)}`, s),
  postHostedMessage: (host: string, scope: string, body: HostedPostBody, s?: AbortSignal) =>
    post<{ id: string; posted: HostedPosted }>(`/hosts/${host}/channels/${scope}/messages`, body, s),
  reactHosted: (host: string, id: string, emoji: string, s?: AbortSignal) => post<{ id: string }>(`/hosts/${host}/messages/${id}/react`, { emoji }, s),
  retractHosted: (host: string, id: string, s?: AbortSignal) => post<{ id: string }>(`/hosts/${host}/messages/${id}/retract`, {}, s),
  markHostedRead: (host: string, scope: string, s?: AbortSignal) => post<{ ok: boolean }>(`/hosts/${host}/read`, { scope }, s),
  /// The wire as it stands: whether a pump runs, every relay's health, the counts.
  sync: (s?: AbortSignal) => get<SyncReport>("/sync", s),
  checkRelay: (url: string, s?: AbortSignal) => post<RelayCheck>("/sync/relays/check", { url }, s),
  reconnectRelays: (s?: AbortSignal) => post<{ ok: boolean }>("/sync/relays/reconnect", {}, s),

  // --- goals ------------------------------------------------------------
  /// Every goal in the lists; `archived` adds the ones put away.
  goals: (opts: { archived?: boolean } = {}, s?: AbortSignal) => get<{ goals: GoalRow[] }>(`/goals${opts.archived ? "?archived=true" : ""}`, s),
  /// What retiring a goal would touch — the facts the retirement dialog is drawn from.
  goalRetirement: (id: string, s?: AbortSignal) => get<{ goal: string; retirement: Retirement }>(`/goals/${id}/retirement`, s),
  /// Retire a goal on a plan: stop its work, close it, settle the projects born of it, archive or delete it.
  retireGoal: (id: string, plan: GoalPlan, s?: AbortSignal) => post<{ goal: string; retired: Retired }>(`/goals/${id}/retire`, plan, s),
  /// Put a goal away (closing it first) or take it back out.
  archiveGoal: (id: string, archived: boolean, s?: AbortSignal) => post<{ goal: Goal; ended?: SessionsStopped }>(`/goals/${id}/archive`, { archived }, s),
  goal: (id: string, s?: AbortSignal) => get<GoalView>(`/goals/${id}`, s),
  /// Capture a goal. Without `workflow` a guided goal wakes the Workflow
  /// Agent to propose one; with `workflow` and `inputs` its work begins at
  /// once — a run, or listening when the workflow begins on events, the
  /// public hooks' secrets that minted answered beside the goal, once.
  /// `workflow` is an installed workflow's id or a catalog slug.
  /// Sending `inputs` beside `workflow` starts the run; omit it to hold.
  /// `assignees` are in `Assignee`'s wire form; `documents` are descriptors
  /// `uploadAttachment` answered.
  createGoal: (body: NewGoalBody, s?: AbortSignal) => post<{ goal: Goal; secrets?: HookSecret[] }>("/goals", body, s),
  /// The files a person gave the goal as context: each with the name it is
  /// kept under, its absolute path, and whether the bytes are on this node.
  goalDocuments: (id: string, s?: AbortSignal) => get<{ goal: string; documents: GoalDocumentRow[] }>(`/goals/${id}/documents`, s),
  /// Give a goal more documents — uploaded descriptors, materialised under
  /// its `documents/` (a taken name numbered) and announced as `document_added`.
  addGoalDocuments: (id: string, documents: AttachmentRef[], s?: AbortSignal) =>
    post<{ goal: string; documents: GoalDocumentRow[] }>(`/goals/${id}/documents`, { documents }, s),
  /// Set who carries a goal; an empty list clears it. Work goes to the
  /// agents named (a team expands to its agents) and the humans named may
  /// decide the goal's gates. Entries use `Assignee`'s wire form:
  /// `agent:<id>`, `human:<hex>`, `team:<id>`.
  setAssignees: (id: string, assignees: string[], s?: AbortSignal) =>
    put<{ goal: Goal }>(`/goals/${id}/assignees`, { assignees }, s),
  /// One of the goal's runs, whole, by id — an earlier or a queued run for the run picker. 404 for another goal's run.
  goalRunById: (id: string, run: string, s?: AbortSignal) => get<{ run: WorkflowRun }>(`/goals/${id}/runs/${run}`, s),
  /// Withdraw a queued run from the goal's queue. 404 for another goal's run, 409 for one that is live or over.
  withdrawRun: (id: string, run: string, s?: AbortSignal) => del<{ run: WorkflowRun }>(`/goals/${id}/runs/${run}`, undefined, s),
  /// Stop the goal: its sessions ended, its queued runs withdrawn, its live
  /// run cancelled. The goal stays open, ready for a new run.
  stopGoal: (id: string, body?: StopBody, s?: AbortSignal) => post<StopOutcome>(`/goals/${id}/stop`, body ?? {}, s),
  /// Restart the goal: a new run of its last run's workflow and inputs,
  /// started at once ahead of the queue; a live run is cancelled first.
  restartGoal: (id: string, s?: AbortSignal) => post<{ run: WorkflowRun; status: RunStatus; ended?: SessionsStopped }>(`/goals/${id}/restart`, {}, s),
  /// Point the goal at a workflow (id or slug), at none, or — with a
  /// `definition` — record a design of the goal's own and point at it.
  /// Refused while a run is unfinished.
  setGoalWorkflow: (id: string, body: SetWorkflowBody, s?: AbortSignal) => put<{ goal: Goal; workflow?: Workflow; problems?: Problem[] }>(`/goals/${id}/workflow`, body, s),
  /// Make a run of the goal's workflow: started at once when nothing is
  /// live on the goal, queued behind its live run otherwise — `status` says
  /// which. A design that begins on events is armed instead: the goal
  /// listens, and each occurrence runs it (`listening_changed` says so) —
  /// no run is made then, and a public hook's secret minted by the arming
  /// comes back once.
  startRun: (id: string, inputs: Record<string, unknown> = {}, s?: AbortSignal) =>
    post<{ run?: WorkflowRun | null; status?: RunStatus; secrets?: HookSecret[] }>(`/goals/${id}/run`, { inputs }, s),
  /// Ask the Workflow Agent to design the goal's workflow again — after a
  /// stall, a failure or a restart. 409 when there is nothing to design.
  designGoal: (id: string, s?: AbortSignal) => post<{ guidance: GuidanceInfo }>(`/goals/${id}/design`, {}, s),
  /// Close for good: the live run and the queued runs are cancelled,
  /// questions withdrawn, workstreams released; the reason is recorded with
  /// the goal.
  closeGoal: (id: string, body?: CloseGoalBody, s?: AbortSignal) => post<{ goal: Goal; ended?: SessionsStopped }>(`/goals/${id}/close`, body ?? {}, s),
  /**
   * Decide a gate of a goal. `answer` is an {@link Answer}, not a string;
   * `inputs` matter for one gate — adopting a proposed workflow starts its
   * run with them.
   */
  decide: (id: string, body: DecideBody, s?: AbortSignal) => post<DecideOutcome>(`/goals/${id}/decide`, body, s),
  /**
   * Decide an ask through the home it is filed at — a goal, or a run of the
   * workspace (`NeedsAction.home`, `GateEntry.home`): the one door every ask
   * card uses, whichever kind of run asked.
   */
  decideIn: (home: Home, body: DecideBody, s?: AbortSignal) =>
    post<DecideOutcome>(home.home === "goal" ? `/goals/${home.goal}/decide` : `/runs/${home.run}/decide`, body, s),

  // --- workflows and runs -----------------------------------------------
  /// Workflows with their problems and what holds them. The library by
  /// default (workspace + catalog); `scope: "all"` adds every goal's designs;
  /// `goal` asks for one goal's designs instead.
  workflows: (
    opts: { scope?: "library" | "all"; goal?: string; tags?: string[]; archived?: boolean } = {},
    s?: AbortSignal,
  ) => {
    const q = new URLSearchParams();
    if (opts.scope) q.set("scope", opts.scope);
    if (opts.goal) q.set("goal", opts.goal);
    if (opts.archived) q.set("archived", "true");
    for (const t of opts.tags ?? []) q.append("tag", t);
    const qs = q.toString();
    return get<{ workflows: WorkflowRow[] }>(`/workflows${qs ? `?${qs}` : ""}`, s);
  },
  workflow: (wfid: string, s?: AbortSignal) => get<WorkflowRow>(`/workflows/${wfid}`, s),
  /// Record a local workflow as a draft: kept with its problems, which come
  /// back beside it. A start is what refuses problems.
  createWorkflow: (body: NewWorkflowBody, s?: AbortSignal) =>
    post<{ workflow: Workflow; problems: Problem[] }>("/workflows", body, s),
  /// Save an edited definition as the next revision of the one at
  /// `revision`. Problems come back beside the saved definition rather than
  /// refusing it — a save mid-edit is allowed to be incomplete. 409 when the
  /// stored copy has moved past `revision`; nothing is written then.
  putWorkflow: (wfid: string, body: PutWorkflowBody, s?: AbortSignal, o?: ReqOptions) =>
    put<{ workflow: Workflow; problems: Problem[] }>(`/workflows/${wfid}`, body, s, o),
  /// What retiring a workflow would touch; while `used_by` is not empty only archiving is possible.
  workflowRetirement: (wfid: string, s?: AbortSignal) => get<{ workflow: string; retirement: Retirement }>(`/workflows/${wfid}/retirement`, s),
  /// Retire a workflow on a plan; 409 with the holders for a delete while anything uses it.
  retireWorkflow: (wfid: string, plan: WorkflowPlan, s?: AbortSignal) => post<{ workflow: string; retired: Retired }>(`/workflows/${wfid}/retire`, plan, s),
  /// Put a workflow away or take it back out.
  archiveWorkflow: (wfid: string, archived: boolean, s?: AbortSignal) => post<{ workflow: WorkflowRow }>(`/workflows/${wfid}/archive`, { archived }, s),
  /// Copy a goal's design into the library. 409 for a library workflow.
  promoteWorkflow: (wfid: string, s?: AbortSignal) =>
    post<{ workflow: Workflow }>(`/workflows/${wfid}/promote`, {}, s),
  /// Run the workflow in the workspace — no goal: started at once, beside
  /// any other run of it. 400 when it cannot start there (`needs_goal`, a
  /// missing input, problems).
  runWorkflow: (wfid: string, inputs: Record<string, unknown> = {}, s?: AbortSignal) =>
    post<{ run: WorkflowRun; status: RunStatus }>(`/workflows/${wfid}/runs`, { inputs }, s),
  /// The workflow's runs of the workspace, newest first, each numbered among them.
  workflowRuns: (wfid: string, s?: AbortSignal) => get<{ workflow: string; runs: RunSummary[] }>(`/workflows/${wfid}/runs`, s),
  /// Stop every run of the workspace of the workflow that is going; answers the runs stopped.
  stopWorkflow: (wfid: string, s?: AbortSignal) => post<{ workflow: string; runs: string[]; ended?: SessionsStopped }>(`/workflows/${wfid}/stop`, {}, s),
  /// Restart every run of the workspace of the workflow that is going; answers the new runs.
  restartWorkflow: (wfid: string, s?: AbortSignal) => post<{ workflow: string; runs: string[]; ended?: SessionsStopped }>(`/workflows/${wfid}/restart`, {}, s),

  // --- one run, by its id — a goal's or the workspace's -------------------
  /// One run, whole: its summary, who it waits on and what it owes a person.
  run: (rid: string, s?: AbortSignal) => get<RunView>(`/runs/${rid}`, s),
  /// Stop a run of the workspace. 409 for a goal's run — stop it from its goal.
  stopRun: (rid: string, body?: StopBody, s?: AbortSignal) => post<{ run: WorkflowRun; ended?: SessionsStopped }>(`/runs/${rid}/stop`, body ?? {}, s),
  /// Restart a run of the workspace: a new run at the same start, with its inputs and its event. 409 for a goal's run.
  restartRun: (rid: string, s?: AbortSignal) => post<{ run: WorkflowRun; status: RunStatus; ended?: SessionsStopped }>(`/runs/${rid}/restart`, {}, s),
  /// Answer a waiting `human` step of any run; through its gate when one is open.
  answerStep: (rid: string, step: string, answer: Answer, s?: AbortSignal) =>
    post<{ run: WorkflowRun }>(`/runs/${rid}/steps/${step}/answer`, { answer }, s),
  /// Release a `wait` step of any run a person is holding.
  releaseStep: (rid: string, step: string, s?: AbortSignal) =>
    post<{ run: WorkflowRun }>(`/runs/${rid}/steps/${step}/release`, {}, s),
  /// A `human` step of any run, done by hand.
  markStepDone: (rid: string, step: string, s?: AbortSignal) =>
    post<{ run: WorkflowRun }>(`/runs/${rid}/steps/${step}/done`, {}, s),
  /// Every problem a definition has, without recording it.
  validateWorkflow: (body: NewWorkflowBody, s?: AbortSignal) =>
    post<{ problems: Problem[] }>("/workflows/validate", body, s),

  // --- events: listening, listeners, hooks, signals, test runs ------------
  /// Turn a library workflow On: its start events are heard from now on, each
  /// occurrence a run of the workspace binding these inputs, under this
  /// per-run budget. Refused while it has problems, is archived, reads its
  /// goal, or the inputs do not bind. A public hook's secret comes back once.
  turnOnWorkflow: (wfid: string, body: ListeningBody, s?: AbortSignal) =>
    put<{ workflow: WorkflowRow; listeners: ListenerView[]; secrets: HookSecret[] }>(`/workflows/${wfid}/listening`, body, s),
  /// Turn it Off: its start events are no longer heard; the occurrences queued for it settle unheard.
  turnOffWorkflow: (wfid: string, s?: AbortSignal) => del<{ workflow: WorkflowRow }>(`/workflows/${wfid}/listening`, undefined, s),
  /// The workflow's listeners while it is On: what each listens for, when it next comes due, its backlog, its hook's calls.
  workflowListeners: (wfid: string, s?: AbortSignal) => get<ListenerView[]>(`/workflows/${wfid}/listeners`, s),
  /// Mint a new secret for one of its public hook starts — shown once; the old one stops working.
  rotateWorkflowHookSecret: (wfid: string, step: string, s?: AbortSignal) => post<HookSecret>(`/workflows/${wfid}/hooks/${step}/secret`, {}, s),
  /// A goal listens again — after a failed run or a spent budget paused it — with these inputs.
  listenGoal: (id: string, body: GoalListeningBody, s?: AbortSignal) =>
    put<{ goal: GoalView; listeners: ListenerView[]; secrets: HookSecret[] }>(`/goals/${id}/listening`, body, s),
  /// The goal stops listening: its start events are no longer heard; a live run goes on.
  stopListeningGoal: (id: string, s?: AbortSignal) => del<{ goal: GoalView }>(`/goals/${id}/listening`, undefined, s),
  /// The goal's listeners while it listens.
  goalListeners: (id: string, s?: AbortSignal) => get<ListenerView[]>(`/goals/${id}/listeners`, s),
  /// Mint a new secret for one of the goal's public hook starts — shown once.
  rotateGoalHookSecret: (id: string, step: string, s?: AbortSignal) => post<HookSecret>(`/goals/${id}/hooks/${step}/secret`, {}, s),
  /// The newest occurrences, queued or settled — one host's (`workspace:<workflow>`, `goal:<goal>`) when `host` names it. Never a payload.
  signals: (opts: { host?: string; limit?: number } = {}, s?: AbortSignal) => {
    const q = new URLSearchParams();
    q.set("limit", String(opts.limit ?? 50));
    if (opts.host) q.set("host", opts.host);
    return get<SignalView[]>(`/signals?${q.toString()}`, s);
  },
  /// Let a held signal through — an outside payload the content screen would not pass, read by a person: it is queued again and begins its run. 409 for one that is not held.
  releaseSignal: (id: string, s?: AbortSignal) => post<{ signal: SignalView }>(`/signals/${id}/release`, {}, s),
  /// A test run of a library workflow in the workspace: it begins at `start`, an event start, as if `event` — a sample payload — had happened.
  testRunWorkflow: (wfid: string, body: Omit<StartRunBody, "event"> & { start: string; event: unknown }, s?: AbortSignal) =>
    post<{ run: WorkflowRun; status: RunStatus }>(`/workflows/${wfid}/runs`, body, s),
  /// A run of the goal at a named start — its start by hand while it listens (*Run now*), or an event start as a test run with a sample payload.
  startGoalRunAt: (id: string, body: Omit<StartRunBody, "event"> & { start: string; event?: unknown }, s?: AbortSignal) =>
    post<{ run: WorkflowRun; status: RunStatus }>(`/goals/${id}/run`, body, s),


  // --- inbox / pulse ------------------------------------------------------
  inbox: (filter?: string, s?: AbortSignal) =>
    get<{ rows: InboxRow[] }>(`/inbox${filter && filter !== "all" ? `?filter=${filter}` : ""}`, s),
  /// One page of the activity feed: every concept or one, strictly before
  /// the cursor the last page answered, newest first.
  pulse: (concept: string, before: PulseCursor | null, limit: number, s?: AbortSignal) => {
    const q = new URLSearchParams();
    if (concept !== "all") q.set("concept", concept);
    if (before) {
      q.set("before", String(before.at));
      q.set("before_seq", String(before.seq));
    }
    q.set("limit", String(limit));
    return get<PulsePage>(`/pulse?${q.toString()}`, s);
  },
  markRead: (scope: string, s?: AbortSignal) => post<{ ok: boolean }>("/read", { scope }, s),
  markUnread: (scope: string, s?: AbortSignal) => post<{ ok: boolean }>("/unread", { scope }, s),

  // --- channels / messages -------------------------------------------------
  channels: (s?: AbortSignal) => get<{ channels: ChannelListEntry[] }>("/channels", s),
  channel: (id: string, s?: AbortSignal) => get<{ channel: ChannelDef }>(`/channels/${id}`, s),
  /// `agents` is the standing roster — a directory, not an audience and not
  /// a subscription: rostered agents still answer only when addressed;
  /// `teams` expand to their members at post time; `humans` are the hosted
  /// members a guest reaches (14-collaboration).
  createChannel: (body: NewChannelBody, s?: AbortSignal) => post<{ channel: ChannelDef }>("/channels", body, s),
  /// Edit a standing channel's topic, roster and tags. The audience and the
  /// kind are not editable — changing an audience would change who a past
  /// message was encrypted to — and the node refuses this on a direct channel.
  patchChannel: (id: string, body: PatchChannelBody, s?: AbortSignal) => patch<{ channel: ChannelDef }>(`/channels/${id}`, body, s),
  channelMessages: (id: string, before?: PageBefore, limit?: number, s?: AbortSignal) =>
    get<{ scope: string; messages: MessageRow[]; reactions: ReactionRow[] }>(
      `/channels/${id}/messages${page(before, limit)}`,
      s,
    ),
  postChannelMessage: (
    id: string,
    body: NewMessageBody,
    s?: AbortSignal,
  ) => post<{ id: string }>(`/channels/${id}/messages`, body, s),

  goalMessages: (id: string, before?: PageBefore, limit?: number, s?: AbortSignal) =>
    get<{ scope: string; messages: MessageRow[]; reactions: ReactionRow[] }>(
      `/goals/${id}/messages${page(before, limit)}`,
      s,
    ),
  postGoalMessage: (
    id: string,
    body: NewMessageBody,
    s?: AbortSignal,
  ) => post<{ id: string }>(`/goals/${id}/messages`, body, s),

  dms: (s?: AbortSignal) => get<{ dms: ChannelListEntry[] }>("/dms", s),
  openDm: (members: string[], s?: AbortSignal) => post<{ channel: ChannelDef }>("/dms", { members }, s),

  retractMessage: (id: string, s?: AbortSignal) => post<{ id: string }>(`/messages/${id}/retract`, {}, s),
  react: (id: string, emoji: string, s?: AbortSignal) =>
    post<{ id: string }>(`/messages/${id}/react`, { emoji }, s),
  retractReaction: (id: string, s?: AbortSignal) =>
    post<{ id: string }>(`/reactions/${id}/retract`, {}, s),

  // --- agents / teams / sessions -----------------------------------------
  agents: (s?: AbortSignal) => get<{ agents: AgentDef[] }>("/agents", s),
  agent: (id: string, s?: AbortSignal) => get<{ agent: AgentDef }>(`/agents/${id}`, s),
  /// `skills` and `mcps` are library and registry ids — references, not
  /// content; `photo` is an attachment this machine holds (ide/14 §Photos).
  createAgent: (body: NewAgentBody, s?: AbortSignal) => post<{ agent: AgentDef }>("/agents", body, s),
  patchAgent: (id: string, body: PatchAgentBody, s?: AbortSignal) =>
    patch<{ agent: AgentDef }>(`/agents/${id}`, body, s),
  deleteAgent: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/agents/${id}`, undefined, s),
  recall: (id: string, s?: AbortSignal) =>
    get<{ agent: string; records: RecallRecord[] }>(`/agents/${id}/recall`, s),

  teams: (s?: AbortSignal) => get<{ teams: TeamDef[] }>("/teams", s),
  /// A team plus the goals it is carrying.
  team: (id: string, s?: AbortSignal) =>
    get<{ team: TeamDef; goals: GoalRow[] }>(`/teams/${id}`, s),
  createTeam: (body: TeamBody & { members: ({ human: string } | { agent: string })[] }, s?: AbortSignal) => post<{ team: TeamDef }>("/teams", body, s),
  patchTeam: (id: string, body: TeamPatchBody, s?: AbortSignal) =>
    patch<{ team: TeamDef }>(`/teams/${id}`, body, s),
  deleteTeam: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/teams/${id}`, undefined, s),

  sessions: (s?: AbortSignal) => get<SessionsResponse>("/sessions", s),
  session: (id: string, s?: AbortSignal) => get<SessionRow>(`/sessions/${id}`, s),
  abortSession: (id: string, s?: AbortSignal) => post<{ ok: boolean; ended?: SessionsStopped }>(`/sessions/${id}/abort`, {}, s),
  transcript: (id: string, fromByte?: number, s?: AbortSignal) =>
    get<{ text: string; next_byte: number }>(
      `/sessions/${id}/transcript${fromByte ? `?from_byte=${fromByte}` : ""}`,
      s,
    ),

  // --- skills / MCP registry / tags ---------------------------------------
  /// The shared skill library. A skill is a procedure an agent follows; the
  /// agent's system prompt is its role. Agents carry ids, not copies.
  skills: (tags?: string[], s?: AbortSignal) =>
    get<{ skills: SkillDef[] }>(`/skills${tagQuery(tags)}`, s),
  createSkill: (body: NewSkillBody, s?: AbortSignal) => post<{ skill: SkillDef }>("/skills", body, s),
  patchSkill: (id: string, body: PatchSkillBody, s?: AbortSignal) =>
    patch<{ skill: SkillDef }>(`/skills/${id}`, body, s),
  deleteSkill: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/skills/${id}`, undefined, s),

  /// Pets: an animated companion, in Codex's package format so a pet made
  /// anywhere works here. Local to this machine like the theme — a pet has no
  /// GEP kind and never syncs.
  pets: (s?: AbortSignal) => get<{ pets: PetDef[] }>("/pets", s),
  /// Copy a package in from a folder. `path` comes from `pickFolder`.
  installPet: (path: string, s?: AbortSignal) => post<{ pet: PetDef }>("/pets", { path }, s),
  deletePet: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/pets/${id}`, undefined, s),
  /// The sprite sheet, for a CSS `background-image`. A URL rather than bytes,
  /// the same way `attachmentUrl` gets a photo into the webview.
  petSpriteUrl: (id: string) => withToken(`${apiBaseSync()}/pets/${encodeURIComponent(id)}/sprite`),

  /// Addons (18 — Addons): the overlay widgets installed here. The record
  /// is the node's; the windows' places are this machine's (`addonsStore`).
  addons: (s?: AbortSignal) => get<Addons>("/addons", s),
  /// Import a folder — `path` from `pickFolder` — with the grants and the
  /// switch the person chose in the review; nothing is granted by default.
  importAddon: (path: string, granted: AddonPermission[], enabled: boolean, s?: AbortSignal) =>
    post<{ addon: Addon }>("/addons", { path, granted, enabled }, s),
  /// The built-ins the catalog offers, each with its manifest, for the review before an install.
  addonOffers: (s?: AbortSignal) => get<AddonOffers>("/addons/offer", s),
  /// A folder's manifest and every problem it has as an addon, writing nothing.
  validateAddon: (path: string, s?: AbortSignal) => post<AddonProblems>("/addons/validate", { path }, s),
  patchAddon: (id: string, body: AddonPatchBody, s?: AbortSignal) =>
    patch<{ addon: Addon }>(`/addons/${encodeURIComponent(id)}`, body, s),
  deleteAddon: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/addons/${encodeURIComponent(id)}`, undefined, s),
  /// The network broker: the one way an addon reaches the internet, judged by the node.
  addonFetch: (id: string, url: string, accept: string | null, s?: AbortSignal) =>
    post<AddonFetchResult>(`/addons/${encodeURIComponent(id)}/fetch`, accept ? { url, accept } : { url }, s),
  /// A bundle file's URL, for the frame that runs it. **Never `withToken`**:
  /// the route answers without one, by design — a token in this URL would be
  /// a token the addon could read.
  addonFileUrl: (id: string, path: string) => `${apiBaseSync()}/addons/${encodeURIComponent(id)}/files/${path}`,

  /// Notes: a markdown scratchpad on a goal, a project, or the workspace.
  /// Local to this machine by design — a note is somebody's own thinking, and
  /// `docs/GEP.md`'s test denies a kind to anything a second node could not
  /// act on — so unlike everything above these never sync.
  ///
  /// `scopeQuery` builds the query string; a workspace note carries no id and
  /// the node refuses one that does.
  notes: (query: string, s?: AbortSignal) => get<{ notes: NoteRow[] }>(`/notes${query}`, s),
  note: (id: string, s?: AbortSignal) => get<{ note: NoteRow }>(`/notes/${id}`, s),
  createNote: (body: NewNoteBody, s?: AbortSignal) =>
    post<{ note: NoteRow }>("/notes", body, s),
  /// `base_hash` is the `hash` of the note you were editing. A stale one comes
  /// back as a 409 whose body carries `current` — see `ApiError`.
  patchNote: (id: string, body: PatchNoteBody, s?: AbortSignal) =>
    patch<{ note: NoteRow }>(`/notes/${id}`, body, s),
  deleteNote: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/notes/${id}`, undefined, s),

  /// The notes repository: `<data_dir>/notes/` is one git repository holding
  /// every note (ide/04 §The notes repository); the drawings repository is
  /// its twin over `<data_dir>/drawings/` (19 — Drawings), without a pull —
  /// a drawing's record arrives by sync. Local until you push — there is no
  /// project and no goal behind either, so the Publish gate does not apply;
  /// the push is your own act from a button. Every act answers the fresh
  /// status, except the commit (the commit it made), the suggestion (the
  /// IDE's shape) and the pull (the safety ref, the outcome and the status).
  notesRepo: repoApi("/notes/git"),
  drawingsRepo: repoApi("/drawings/git"),

  /// Drawings (19 — Drawings): a picture on the canvas, filed under a scope
  /// like a note — a record of the workspace that travels, unlike a note.
  /// A list carries no scene; the detail does. `scopeQuery` builds the
  /// query string as it does for notes.
  drawings: (query: string, s?: AbortSignal) => get<{ drawings: DrawingRow[] }>(`/drawings${query}`, s),
  drawing: (id: string, s?: AbortSignal) => get<{ drawing: DrawingDetail }>(`/drawings/${id}`, s),
  createDrawing: (body: NewDrawingBody, s?: AbortSignal) => post<{ drawing: DrawingDetail }>("/drawings", body, s),
  /// `base_hash` is the `hash` of the scene you were drawing on; it is needed
  /// whenever `scene` is present. A stale one comes back as a 409 whose body
  /// carries `current` (the scene) and `current_hash`.
  patchDrawing: (id: string, body: PatchDrawingBody, s?: AbortSignal) => patch<{ drawing: DrawingDetail }>(`/drawings/${id}`, body, s),
  deleteDrawing: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/drawings/${id}`, undefined, s),
  /// The drawing tool calls agents parked for the canvas and nobody answered
  /// yet — read once at start and every twenty seconds while the desktop is
  /// open; a live desktop hears each as a `drawing_request` frame too.
  drawingRequests: (s?: AbortSignal) => get<{ requests: DrawingPending[] }>(`/drawings/requests`, s),
  /// The desktop's answer to one drawing request; 404 when nothing waits
  /// under that id. A snapshot rides as the `AttachmentRef` of a PNG uploaded
  /// through `uploadAttachment` first — never bytes here.
  answerDrawingRequest: (id: string, result: DrawResult, s?: AbortSignal) => post<{ answered: boolean }>(`/drawings/requests/${id}`, result, s),

  /// Registered MCP servers. Local to this machine by design — a stdio
  /// command line names one disk — so these never sync; what travels between
  /// nodes is the id an agent carries.
  mcps: (tags?: string[], s?: AbortSignal) => get<{ mcp: McpServerView[] }>(`/mcp${tagQuery(tags)}`, s),
  createMcp: (body: NewMcpBody, s?: AbortSignal) =>
    post<{ mcp: McpServerView }>("/mcp", body, s),
  patchMcp: (id: string, body: PatchMcpBody, s?: AbortSignal) =>
    patch<{ mcp: McpServerView }>(`/mcp/${id}`, body, s),
  deleteMcp: (id: string, s?: AbortSignal) => del<{ ok: boolean }>(`/mcp/${id}`, undefined, s),
  /// Dial a transport that is nobody's entry yet — the editor's *Test connection*.
  probeMcp: (body: ProbeMcpBody, s?: AbortSignal) => post<{ report: McpProbeReport }>("/mcp/probe", body, s),
  /// Dial a registered server; the answer becomes its health (`mcp_probed` on the bus).
  probeMcpById: (id: string, timeout_secs?: number, s?: AbortSignal) => post<{ report: McpProbeReport; health: McpHealthView }>(`/mcp/${id}/probe`, timeout_secs ? { timeout_secs } : {}, s),

  // --- usage --------------------------------------------------------------
  /// Everything that still points at one object. **Empty means the matching
  /// `DELETE` would succeed.**
  ///
  /// Nothing is deleted while something references it, and the refusal names
  /// what — which is the right answer to a delete and the wrong shape for a
  /// screen. Asking this first is what lets a view show the holders, each one
  /// somewhere to navigate to and detach, rather than offering a button whose
  /// only possible outcome is the node's 400 in a red toast.
  ///
  /// An id nothing knows about is an error rather than an empty list: the
  /// node refuses to answer "nothing references it" for a typo, because that
  /// reads as permission to delete.
  usage: (kind: UsageKind, id: string, s?: AbortSignal) =>
    get<{ usage: Reference[] }>(`/usage/${kind}/${encodeURIComponent(id)}`, s),

  // --- files --------------------------------------------------------------
  /**
   * What a goal, a project or a workstream has on disk.
   *
   * These read through the **node**, not the filesystem, even though the
   * desktop usually runs beside one. What is in a goal's folder is a fact
   * about the workspace, and the node is the workspace; reading the disk
   * directly would give a second answer that works only when the node is
   * local and disagrees the moment it isn't. The terminal goes the other way
   * and lives in the Tauri layer, because a shell is a capability of this
   * machine rather than a fact about the workspace.
   *
   * `path` is always relative to the scope's root and comes back that way, so
   * an entry's `path` hands straight back as the next request's `path`.
   */
  tree: (scope: FileScope, id: string, path?: string, depth?: number, s?: AbortSignal) => {
    const q = new URLSearchParams();
    if (path) q.set("path", path);
    if (depth !== undefined) q.set("depth", String(depth));
    const qs = q.toString();
    return get<FileTree>(`/tree/${scope}/${encodeURIComponent(id)}${qs ? `?${qs}` : ""}`, s);
  },
  file: (scope: FileScope, id: string, path: string, s?: AbortSignal) =>
    get<FileContent>(
      `/file/${scope}/${encodeURIComponent(id)}?path=${encodeURIComponent(path)}`,
      s,
    ),
  /// Where a scope resolves to on disk. Safe to ask before the folder exists —
  /// `exists` says which, rather than the call failing.
  placement: (scope: FileScope, id: string, s?: AbortSignal) =>
    get<Placement>(`/placement/${scope}/${encodeURIComponent(id)}`, s),

  // --- catalog ------------------------------------------------------------
  /// What this build can install, and what already is.
  ///
  /// Every row carries `installed`, so one request draws a picker and its
  /// checkmarks — a client that diffed the catalog against `/agents`,
  /// `/skills`, `/teams` and `/channels` would run four races instead of
  /// asking one question. `installed` says the **id is taken**, not that this
  /// entry took it: an id held by something you made here reads as installed,
  /// and the install refuses it rather than overwriting.
  catalog: (
    opts?: { kind?: CatalogKind; tags?: string[]; match?: "any" | "all" },
    s?: AbortSignal,
  ) => {
    const q = new URLSearchParams();
    if (opts?.kind) q.set("kind", opts.kind);
    // `?tag=` repeats rather than joining, matching the node's extractor.
    for (const t of opts?.tags ?? []) q.append("tag", t);
    if (opts?.match) q.set("match", opts.match);
    const qs = q.toString();
    return get<{ entries: CatalogEntry[] }>(`/catalog${qs ? `?${qs}` : ""}`, s);
  },
  /// Install one entry and everything it needs.
  ///
  /// The install is **transitive** — a team brings its agents, an agent its
  /// skills — so the answer worth reading is `installed`, which names
  /// everything that was *created*, per kind. Installing something already
  /// present answers 200 with every list empty: that is idempotence reported
  /// honestly, not a failure, and it deserves a different sentence rather
  /// than a silent success. A **collision** — an id that exists and did not
  /// come from this entry — is a 400 carrying the store's own message naming
  /// the id, because "install failed" leaves the owner guessing whether their
  /// own work is at risk.
  installCatalogEntry: (kind: CatalogKind, slug: string, s?: AbortSignal) =>
    post<{ installed: Installed }>("/catalog/install", { kind, slug }, s),


  // --- projects -----------------------------------------------------------
  /// Every project in the workspace, once each, with `goals` naming what it
  /// is attached to. One request for the Projects index or a project picker.
  /// Every project in the lists; `archived` adds the ones put away.
  allProjects: (opts: { archived?: boolean } = {}, s?: AbortSignal) => get<{ projects: ProjectRow[] }>(`/projects${opts.archived ? "?archived=true" : ""}`, s),
  /// Put a project away — every session in it stopped, hidden from the rail — or take it back out.
  archiveProject: (pid: string, archived: boolean, s?: AbortSignal) => post<{ project: ProjectRow }>(`/projects/${pid}/archive`, { archived }, s),
  /// The projects attached to one goal — the same rows, filtered.
  projects: (goal: string, s?: AbortSignal) =>
    get<{ goal: string; projects: ProjectRow[] }>(`/goals/${goal}/projects`, s),
  /// Create a project. Provenance is explicit — `new` | `clone` | `import` |
  /// `adopt` — and a managed root (`new`, `clone`, `import`) ends up a git
  /// repository. `adopt` writes nothing into the folder it is given. With a
  /// goal the project is created and attached in one call; without one it
  /// belongs to the workspace alone.
  createProject: (body: NewProjectBody, goal?: string | null, s?: AbortSignal) =>
    post<ProjectCreated>(goal ? `/goals/${goal}/projects` : "/projects", body, s),
  project: (pid: string, s?: AbortSignal) => get<ProjectDetail>(`/projects/${pid}`, s),
  /// Edit a project. `group` and `photo` are tri-state: omitted keeps, `null`
  /// clears, a value sets.
  patchProject: (pid: string, body: PatchProjectBody, s?: AbortSignal) => patch<{ project: Project }>(`/projects/${pid}`, body, s),
  /// Edit what a person may change about a workstream: its name, its note,
  /// whether it is pinned, its due date on the Board (`YYYY-MM-DD`). `null`
  /// clears a name, a note or a due date.
  patchWorkstream: (wid: string, body: PatchWorkstreamBody, s?: AbortSignal) => patch<{ workstream: Workstream }>(`/workstreams/${wid}`, body, s),
  /// Put a card at an index in a Board column (ide/16). A view, never the
  /// lifecycle; the reply is every record the move rewrote — one, or the
  /// whole column when it was renumbered. The body is `boardModel.placeBody`'s:
  /// the index the node counts — among the column's placed cards — held to
  /// the `u32` the wire takes.
  placeWorkstreamCard: (wid: string, body: PlaceWorkstreamBody, s?: AbortSignal) =>
    put<{ workstreams: Workstream[] }>(`/workstreams/${wid}/board/place`, body, s),
  /// Forget a project. **The folder stays on disk** unless `tree` is set, and
  /// an adopted (external) folder is refused even then.
  deleteProject: (pid: string, opts?: { tree?: boolean }, s?: AbortSignal) =>
    del<{ ok: boolean; removed_tree: boolean; path: string; workstreams_forgotten: number; kept?: string | null }>(
      `/projects/${pid}${opts?.tree ? "?tree=true" : ""}`,
      undefined,
      s,
    ),
  /// Attach a project to a goal: one record, nothing on disk moves.
  attachProject: (pid: string, goal: string, s?: AbortSignal) =>
    post<{ project: Project; goals: string[]; attached_to: string }>(
      `/projects/${pid}/attach`,
      { goal },
      s,
    ),
  /// Detach a project from a goal. The folder, its workstreams and every
  /// journal entry that names it stay exactly where they are.
  detachProject: (pid: string, goal: string, s?: AbortSignal) =>
    del<{ ok: boolean; project: string; detached_from: string; goals: string[] }>(
      `/projects/${pid}/attach`,
      { goal },
      s,
    ),
  /// Branch, ahead/behind, remote and dirty counts of a workstream's checkout
  /// — pass a project id for its primary (the two share one id). Neutral
  /// (`git: false`, `clean: true`) for a folder with no repository — not an error.
  workstreamGitStatus: (wid: string, s?: AbortSignal) =>
    get<WorkstreamGitStatus>(`/workstreams/${wid}/git/status`, s),
  /// Turn a plain-folder project's tree into a git repository, on the person's
  /// explicit ask — an adopted folder too, the one write adopt allows.
  /// `gitConfig` is the same local config a creation request can name.
  /// 409 when the record already says git; 400 when the folder is not on disk.
  initRepository: (pid: string, gitConfig: [string, string][] = [], s?: AbortSignal) =>
    post<InitRepositoryReply>(`/projects/${pid}/git/init`, { git_config: gitConfig }, s),
  /// Who carries a project: its agents take work that runs here, its humans
  /// may decide its gates. An empty list clears it.
  setProjectAssignees: (pid: string, assignees: string[], s?: AbortSignal) =>
    put<{ project: Project }>(`/projects/${pid}/assignees`, { assignees }, s),

  // --- a workstream's checkout, file by file --------------------------------
  //
  // Keyed by a workstream. A project's own tree is its primary
  // workstream, whose id is the project's, so every caller that has a project
  // id already has the id of its root checkout.
  //
  // Every one of them 400s on a folder with no repository and 404s on a
  // folder that is not there, so callers gate on `workstreamGitStatus` rather than
  // discovering it as an error.

  /// Every path git has something to say about, with **both** of its letters.
  /// `index` and `worktree` are kept apart because a file can be staged and
  /// modified since — the one fact this surface exists to show. A read, and
  /// only a read: it stages nothing.
  gitFiles: (wid: string, s?: AbortSignal) =>
    get<WorkstreamGitFiles>(`/workstreams/${wid}/git/files`, s),
  /// One file's patch. `staged` selects the index against HEAD instead of the
  /// working tree against the index — the two questions a row in the staged
  /// list and a row in the unstaged list are each asking.
  ///
  /// An **untracked** file answers `{diff: "", untracked: true}` rather than
  /// staging itself to produce a patch. Say so in the UI; do not render an
  /// empty box.
  gitDiff: (wid: string, path: string, staged: boolean, s?: AbortSignal) =>
    get<WorkstreamFileDiff>(
      `/workstreams/${wid}/git/diff?path=${encodeURIComponent(path)}${staged ? "&staged=true" : ""}`,
      s,
    ),
  /// Put paths in the index. Paths are repository-root-relative, exactly as
  /// {@link gitFiles} reported them.
  gitStage: (wid: string, paths: string[], s?: AbortSignal) =>
    post<WorkstreamStaged>(`/workstreams/${wid}/git/stage`, { paths }, s),
  /// Take paths back out of the index. `git restore --staged` — **the files
  /// themselves are not touched.**
  gitUnstage: (wid: string, paths: string[], s?: AbortSignal) =>
    post<WorkstreamStaged>(`/workstreams/${wid}/git/unstage`, { paths }, s),
  /// Commit the checkout. **Omitting `paths` commits what is already
  /// staged**, never everything: the primary's checkout is the user's own
  /// working tree and may hold edits that have nothing to do with this commit.
  ///
  /// 409 on a clean tree or a selection that matched nothing changed. There
  /// is no Publish gate here, deliberately — a commit is local and git can
  /// undo it; the gate stands in front of `push` and `pr`, which cannot be
  /// recalled.
  gitCommit: (wid: string, message: string, paths?: string[], s?: AbortSignal) =>
    post<WorkstreamCommitted>(
      `/workstreams/${wid}/git/commit`,
      paths && paths.length > 0 ? { message, paths } : { message },
      s,
    ),
  /// Rewrite the last commit with what is staged and `message` (consented;
  /// the old commit is pinned in Safety first). Omitting `paths` folds in
  /// what is already staged — a reword when nothing is. 409 `in_progress`;
  /// 400 on a detached HEAD, no commit yet, nobody set to commit, or a
  /// blank message.
  gitAmend: (wid: string, message: string, paths?: string[], s?: AbortSignal) =>
    post<WorkstreamAmended>(
      `/workstreams/${wid}/git/amend`,
      paths && paths.length > 0 ? { message, paths } : { message },
      s,
    ),
  /// Ask the core agent to draft a commit message for what is staged.
  ///
  /// **Always 200, even when there is no answer** — `{suggested: false,
  /// message: "", error}` is the no-harness or timed-out case, and it leaves
  /// the caller with an empty box and a sentence. Nothing here commits: the
  /// session runs read-only with no MCP servers, so the thing that suggests a
  /// commit cannot make one.
  gitSuggestMessage: (wid: string, s?: AbortSignal) =>
    post<SuggestedCommitMessage>(`/workstreams/${wid}/git/message`, {}, s),
  /// Stage one hunk — or the lines of one the person picked — by patching
  /// the index (`git apply --cached`). `reverse` takes the same hunk back
  /// out. **The working tree is untouched either way**; a patch that does
  /// not fit is a 400, not a partial application. Answers with the fresh
  /// file list like every other write here.
  gitStageHunk: (wid: string, patch: string, reverse: boolean, s?: AbortSignal) =>
    post<WorkstreamStaged>(`/workstreams/${wid}/git/hunk`, { patch, reverse }, s),
  /// Who last touched each line of a file (`git blame --porcelain`). Lines
  /// the working tree has that HEAD does not come back `uncommitted`.
  gitBlame: (wid: string, path: string, s?: AbortSignal) =>
    get<WorkstreamBlame>(`/workstreams/${wid}/git/blame?path=${encodeURIComponent(path)}`, s),
  /// The commits that touched a path, newest first, following renames.
  gitHistory: (wid: string, path: string, limit = 100, s?: AbortSignal) =>
    get<WorkstreamHistory>(
      `/workstreams/${wid}/git/history?path=${encodeURIComponent(path)}&limit=${limit}`,
      s,
    ),

  /// One commit in full for the inspector: message, refs, files against the
  /// first parent, and its patch (cut at 2 MiB with `truncated: true`).
  gitShowCommit: (wid: string, sha: string, s?: AbortSignal) =>
    get<WorkstreamCommitView>(`/workstreams/${wid}/git/commit/${encodeURIComponent(sha)}`, s),
  /// One file's patch in one commit, against the first parent (ide/05) —
  /// the Hunks view of a commit's file; cut at 2 MiB with `truncated: true`.
  /// A path the commit did not touch is a 400.
  gitCommitFileDiff: (wid: string, sha: string, path: string, s?: AbortSignal) =>
    get<CommitFileDiff>(`/workstreams/${wid}/git/commit/${encodeURIComponent(sha)}/diff?path=${encodeURIComponent(path)}`, s),
  /// The two whole texts of one file's change in one commit, for a
  /// comparison (ide/05): the first parent's version — at the old path for a
  /// rename, null for a root commit or a new file — against the commit's,
  /// null for a deleted file; binary says so.
  gitCommitFileSides: (wid: string, sha: string, path: string, s?: AbortSignal) =>
    get<CommitFileSides>(`/workstreams/${wid}/git/commit/${encodeURIComponent(sha)}/sides?path=${encodeURIComponent(path)}`, s),
  /// A window of the commit graph (ide/05). The first request on a root lays
  /// out the first thousand rows inline and the rest in the background — poll
  /// while `done` is false. `stale` means a ref moved and a relayout is
  /// running; the rows are the last good ones.
  /// `refs` is the one filter: `all` — every local branch and tag — or
  /// `head`, only what HEAD reaches; the node lays the two out apart.
  ideGraph: (scope: FileScope, id: string, from: number, count: number, refresh = false, refs: GraphRefScope = "all", s?: AbortSignal) =>
    get<GraphWindow>(
      `/ide/graph/${scope}/${id}?from=${from}&count=${count}&refs=${refs}${refresh ? "&refresh=true" : ""}`,
      s,
    ),
  /// Search the whole laid-out log: the row indexes matching `q`, so the
  /// graph can jump to a commit far past the window it has loaded.
  ideGraphSearch: (scope: FileScope, id: string, q: string, from = 0, limit = 500, refs: GraphRefScope = "all", s?: AbortSignal) =>
    get<GraphMatches>(`/ide/graph/${scope}/${id}/search?q=${encodeURIComponent(q)}&from=${from}&limit=${limit}&refs=${refs}`, s),

  /// Who will author commits in this checkout's repository, and where that comes from.
  gitIdentity: (wid: string, s?: AbortSignal) =>
    get<GitIdentityView>(`/workstreams/${wid}/git/identity`, s),
  /// The repository's git config: the schema and every key's local · global · effective value.
  workstreamGitConfig: (wid: string, s?: AbortSignal) => get<GitConfigView>(`/workstreams/${wid}/git/config`, s),
  /// Write the repository's local git config (`{set, unset}`, schema keys only). Answers the platform's
  /// *who commits?* when an identity now resolves, and commits a settlement the missing identity had refused.
  setWorkstreamGitConfig: (wid: string, body: GitConfigWrite, s?: AbortSignal) =>
    put<GitConfigView>(`/workstreams/${wid}/git/config`, body, s),
  /// Your global git config: the schema and each key's global value.
  gitConfig: (s?: AbortSignal) => get<GitConfigView>("/git/config", s),
  /// Write your global git config — the one place the platform writes it, at your request from Settings → Git.
  setGitConfig: (body: GitConfigWrite, s?: AbortSignal) => put<GitConfigView>("/git/config", body, s),
  /// What the *who commits?* dialog seeds from: the global pair and the projects still asking.
  gitCommitter: (s?: AbortSignal) => get<CommitterView>("/git/committer", s),

  // --- branches, tags, remotes, and the consented tier (ide/04) -------------
  // Every route below that moves the working tree is **consented**: the node
  // mints a `HumanConsent` from this request's token, the vcs tier writes a
  // recovery ref under `refs/bisa/safety/` before it runs anything, and
  // the answer carries that ref so the UI can offer *Restore what was here*.
  // A conflict, or local changes git will not overwrite, is a **409** with
  // git's own sentence — a state to act on, not a fault.

  gitBranches: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; branches: BranchInfo[] }>(`/workstreams/${wid}/git/branches`, s),
  /// Remote-tracking branches as of the last fetch — a local read; `gitFetch` brings the remote's news in.
  gitRemoteBranches: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; remote_branches: RemoteBranchInfo[] }>(`/workstreams/${wid}/git/remote-branches`, s),
  /// Create a branch at `start` (HEAD when absent) — with `track`, one that
  /// follows `start` as its upstream. Creating is safe — a new ref, nothing
  /// moves; `switch` checks it out too and that half is consented.
  gitBranchCreate: (wid: string, body: { name: string; start?: string | null; track?: boolean; switch?: boolean }, s?: AbortSignal) =>
    post<GitDone | { workstream: string; branches: BranchInfo[] }>(
      `/workstreams/${wid}/git/branches`,
      { name: body.name, start: body.start ?? null, track: body.track ?? false, switch: body.switch ?? false },
      s,
    ),
  /// Point a branch at an upstream — `origin/main` — or at none. Safe: a line of config.
  gitSetUpstream: (wid: string, name: string, upstream: string | null, s?: AbortSignal) =>
    put<{ workstream: string; branches: BranchInfo[] }>(`/workstreams/${wid}/git/branches/${encodeURIComponent(name)}/upstream`, { upstream }, s),
  /// The commits `name` has that `against` lacks, newest first — what a pick from the branch offers, what an interactive rebase replays.
  gitBranchCommits: (wid: string, name: string, against: string, s?: AbortSignal) =>
    get<{ workstream: string; commits: CommitSummary[] }>(`/workstreams/${wid}/git/branches/${encodeURIComponent(name)}/commits?against=${encodeURIComponent(against)}`, s),
  /// Delete a branch on a remote — through the Publish gate like a push (202
  /// when it opens, 409 `publish_manual`), consented; its tip is pinned first.
  gitRemoteBranchDelete: (wid: string, remote: string, branch: string, s?: AbortSignal) =>
    del<GitDone | { workstream: string; gate: string; status: "awaiting_publish_gate"; deleted: false }>(
      `/workstreams/${wid}/git/remote-branches/${encodeURIComponent(remote)}/${encodeURIComponent(branch)}`,
      undefined,
      s,
    ),
  /// Delete a local branch. Its tip is pinned in the recovery ref first.
  gitBranchDelete: (wid: string, name: string, s?: AbortSignal) =>
    del<GitDone>(`/workstreams/${wid}/git/branches/${encodeURIComponent(name)}`, undefined, s),
  /// Rename a local branch. Consented; never the project's default branch; a workstream on it follows.
  gitBranchRename: (wid: string, name: string, to: string, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/branches/${encodeURIComponent(name)}/rename`, { to }, s),
  /// Move HEAD and the tree to a branch (attached) or a commit (detached).
  gitCheckout: (wid: string, target: string, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/checkout`, { target }, s),
  gitTags: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; tags: TagInfo[] }>(`/workstreams/${wid}/git/tags`, s),
  gitTagCreate: (wid: string, name: string, target?: string | null, message?: string | null, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/tags`, { name, target: target ?? null, message: message ?? null }, s),
  gitTagDelete: (wid: string, name: string, s?: AbortSignal) =>
    del<GitDone>(`/workstreams/${wid}/git/tags/${encodeURIComponent(name)}`, undefined, s),
  gitRemotes: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; remotes: RemoteInfo[] }>(`/workstreams/${wid}/git/remotes`, s),
  gitRemoteAdd: (wid: string, name: string, url: string, s?: AbortSignal) =>
    post<{ workstream: string; remotes: RemoteInfo[] }>(`/workstreams/${wid}/git/remotes`, { name, url }, s),
  gitRemoteRemove: (wid: string, name: string, s?: AbortSignal) =>
    del<GitDone>(`/workstreams/${wid}/git/remotes/${encodeURIComponent(name)}`, undefined, s),
  /// Rebase the current branch onto `upstream` — or, with `onto`, only its
  /// commits since `upstream` onto `onto`; `autostash` carries a dirty tree
  /// across. 409 `conflict` leaves the rebase in progress for
  /// {@link gitContinue}, {@link gitSkip} or {@link gitAbort}.
  gitRebase: (wid: string, body: { upstream: string; onto?: string | null; autostash?: boolean }, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/rebase`, { upstream: body.upstream, onto: body.onto ?? null, autostash: body.autostash ?? false }, s),
  /// An interactive rebase planned in full — every commit since `upstream`, oldest first, each with its action — run with no terminal anywhere.
  gitRebasePlan: (wid: string, plan: GitRebasePlan, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/rebase/plan`, plan, s),
  /// Merge `source` into the current branch by `mode`; a squash leaves the result staged.
  gitMerge: (wid: string, body: { source: string; mode: GitMergeMode; message?: string | null }, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/merge`, { source: body.source, mode: body.mode, message: body.message ?? null }, s),
  /// Cherry-pick `commits` (oldest first) onto the current branch.
  gitCherryPick: (wid: string, body: { commits: string[]; record_origin?: boolean; no_commit?: boolean; mainline?: number | null }, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/cherry-pick`, { commits: body.commits, record_origin: body.record_origin ?? false, no_commit: body.no_commit ?? false, mainline: body.mainline ?? null }, s),
  gitAbort: (wid: string, what: GitInProgress, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/abort`, { what }, s),
  /// Go on with the operation half-done once its conflicted paths are settled; 409 `conflict` names the paths still unmerged.
  gitContinue: (wid: string, what: GitInProgress, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/continue`, { what }, s),
  /// Leave out the commit a rebase, cherry-pick or revert stopped on.
  gitSkip: (wid: string, what: GitInProgress, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/skip`, { what }, s),
  /// Undo `commits` with new ones (consented; a conflict is a 409 `conflict`).
  gitRevert: (wid: string, body: { commits: string[]; no_commit?: boolean; mainline?: number | null }, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/revert`, { commits: body.commits, no_commit: body.no_commit ?? false, mainline: body.mainline ?? null }, s),
  /// Fetch a remote's refs (safe). Answers the checkout's fresh status.
  gitFetch: (wid: string, remote?: string, s?: AbortSignal) =>
    post<{ workstream: string; remote: string; fetched: boolean; status: WorkstreamStatus }>(
      `/workstreams/${wid}/git/fetch`,
      remote ? { remote } : {},
      s,
    ),
  /// Fetch, then move the branch to its upstream by `mode` (consented). 409
  /// `not_fast_forward` with `detail.ahead/behind`, 409 `conflict` with
  /// `detail.paths`/`in_progress`, 409 `in_progress`.
  gitPull: (wid: string, mode: PullMode, remote?: string, s?: AbortSignal) =>
    post<GitDone & { pull: PullOutcome }>(`/workstreams/${wid}/git/pull`, remote ? { mode, remote } : { mode }, s),
  /// Point a remote at a URL, adding it when it does not exist (safe).
  gitRemoteSet: (wid: string, name: string, url: string, s?: AbortSignal) =>
    put<{ workstream: string; remotes: RemoteInfo[] }>(`/workstreams/${wid}/git/remotes/${encodeURIComponent(name)}`, { url }, s),
  /// Throw away working-tree changes: one hunk of the *unstaged* patch, or
  /// whole paths (restored from the index). What was there is in the recovery.
  /// A path the index no longer holds is left out; 409 when none is left.
  gitDiscard: (wid: string, what: { patch: string } | { paths: string[] }, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/discard`, what, s),
  /// Recovery points, newest first.
  gitRecovery: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; recovery: GitRecoveryRef[] }>(`/workstreams/${wid}/git/recovery`, s),
  /// *Restore what was here.* Itself consented and recorded — you can undo an undo.
  gitRestore: (wid: string, ref: string, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/recovery/restore`, { ref }, s),
  /// The stash list, newest first — the repository's, shared by every
  /// workstream of the project; `index` is what `stash@{n}` means right now.
  gitStashes: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; stashes: GitStash[] }>(`/workstreams/${wid}/git/stashes`, s),
  /// One stash entry's patch — the tracked change and the untracked files it
  /// carries — cut at 2 MiB (`truncated`).
  gitStashDiff: (wid: string, sha: string, s?: AbortSignal) =>
    get<{ workstream: string; commit: string; diff: string; truncated: boolean }>(`/workstreams/${wid}/git/stashes/${encodeURIComponent(sha)}/diff`, s),
  /// Park the working tree's changes as a stash entry (consented; the tree is
  /// captured first). 409 `nothing_to_stash`, 409 `in_progress`, 409 on
  /// unmerged paths.
  gitStashPush: (wid: string, body: GitStashPush, s?: AbortSignal) =>
    post<GitDone & { stash: GitStash }>(`/workstreams/${wid}/git/stash`, body, s),
  /// Apply a stash entry and keep it on the list. 409 `stash_moved`
  /// (`detail.now`) when the index no longer holds the sha; 409 `conflict`
  /// with `detail.paths` and no `in_progress` — settle or discard, nothing to
  /// abort.
  gitStashApply: (wid: string, sha: string, index: number, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/stashes/${encodeURIComponent(sha)}/apply`, { index }, s),
  /// Apply a stash entry and drop it — only when the apply went cleanly; the
  /// entry is pinned as a `stash` recovery first. The same 409s as apply.
  gitStashPop: (wid: string, sha: string, index: number, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/stashes/${encodeURIComponent(sha)}/pop`, { index }, s),
  /// Drop a stash entry; pinned as a `stash` recovery first, so Safety
  /// restores it. 409 `stash_moved`.
  gitStashDrop: (wid: string, sha: string, index: number, s?: AbortSignal) =>
    post<GitDone>(`/workstreams/${wid}/git/stashes/${encodeURIComponent(sha)}/drop`, { index }, s),
  /// Push a rewritten workstream branch with `--force-with-lease` — never the
  /// project's default branch, through the Publish gate (202 when it opens),
  /// consented. 409 when the remote moved since the last fetch.
  workstreamPushWithLease: (wid: string, s?: AbortSignal) =>
    post<{ workstream: string; pushed: boolean; recovery?: Recovery; gate?: unknown; status?: string }>(
      `/workstreams/${wid}/push-with-lease`,
      {},
      s,
    ),

  // --- language servers (ide/10) ----------------------------------
  // The editor speaks root-relative paths; the node owns the process and the
  // `file://` mapping. Diagnostics arrive as `lsp` frames on the engine stream.
  lspOpen: (scope: FileScope, id: string, path: string, text: string, s?: AbortSignal) =>
    post<{ path: string; language: string | null; following: boolean }>(`/ide/lsp/${scope}/${id}/open`, { path, text }, s),
  lspChange: (scope: FileScope, id: string, path: string, text: string, s?: AbortSignal) =>
    post<{ path: string }>(`/ide/lsp/${scope}/${id}/change`, { path, text }, s),
  lspClose: (scope: FileScope, id: string, path: string, s?: AbortSignal) =>
    post<{ path: string }>(`/ide/lsp/${scope}/${id}/close`, { path }, s),
  /// One allow-listed request; `params` carry root-relative `uri` fields.
  lspRequest: (scope: FileScope, id: string, path: string, method: string, params: unknown, s?: AbortSignal) =>
    post<{ method: string; result: unknown }>(`/ide/lsp/${scope}/${id}/request`, { path, method, params }, s),
  lspStatus: (scope: FileScope, id: string, s?: AbortSignal) =>
    get<{ servers: LspServerStatus[] }>(`/ide/lsp/${scope}/${id}/status`, s),
  lspRestart: (scope: FileScope, id: string, language: string, s?: AbortSignal) =>
    post<{ language: string; restarted: boolean }>(`/ide/lsp/${scope}/${id}/restart`, { language }, s),

  // --- review notes --------------------------------------------
  // A note is pinned to the hunk it was written on and reaches agents as
  // data: a `DiffHunk` chip in the goal thread on send, and a tool result
  // through `review_notes_list`. Local state under `projects/<slug>/review/`,
  // never synced.

  /// The project's notes, oldest first — those written in one workstream
  /// when `workstream` is given. Resolved ones only when asked for.
  reviewNotes: (pid: string, workstream: string | null, resolved = false, s?: AbortSignal) => {
    const q = new URLSearchParams();
    if (workstream) q.set("workstream", workstream);
    if (resolved) q.set("resolved", "true");
    const qs = q.toString();
    return get<ProjectReviewNotes>(`/projects/${pid}/review${qs ? `?${qs}` : ""}`, s);
  },
  /// Annotate a hunk. 201.
  reviewNoteCreate: (pid: string, body: ReviewNoteCreate, s?: AbortSignal) =>
    post<{ note: ReviewNote }>(`/projects/${pid}/review`, body, s),
  /// Edit the body. **Clears `sent_at`**: the agent has not seen this version.
  reviewNoteEdit: (pid: string, id: string, body: string, s?: AbortSignal) =>
    patch<{ note: ReviewNote }>(`/projects/${pid}/review/${id}`, { body }, s),
  /// Mark dealt with. The note stays on disk, hidden from the default listing.
  reviewNoteResolve: (pid: string, id: string, s?: AbortSignal) =>
    post<{ note: ReviewNote }>(`/projects/${pid}/review/${id}/resolve`, {}, s),
  reviewNoteDelete: (pid: string, id: string, s?: AbortSignal) =>
    del<void>(`/projects/${pid}/review/${id}`, undefined, s),
  /// Hand notes to the agents. `ids` empty means every unsent note.
  reviewNotesSend: (pid: string, ids: string[] = [], s?: AbortSignal) =>
    post<ReviewNotesSent>(`/projects/${pid}/review/send`, { ids }, s),

  // --- conversations (13 — Conversations) -----------------------------
  /// Conversations, the most recently moved first: narrowed to what they are
  /// about (`origin` + `id`) or to every one standing in a project (`project`
  /// — its own and its checkouts'; never both), to those an agent took part
  /// in, to words in their messages or titles (`q`), to one side of
  /// `archived`; paged.
  conversations: (
    q: { origin?: ConversationOrigin["kind"]; id?: string; project?: string; agent?: string; q?: string; archived?: boolean; before?: number; limit?: number } = {},
    s?: AbortSignal,
  ) => {
    const params = new URLSearchParams();
    if (q.origin) params.set("origin", q.origin);
    if (q.id) params.set("id", q.id);
    if (q.project) params.set("project", q.project);
    if (q.agent) params.set("agent", q.agent);
    if (q.q) params.set("q", q.q);
    if (q.archived !== undefined) params.set("archived", String(q.archived));
    if (q.before !== undefined) params.set("before", String(q.before));
    if (q.limit !== undefined) params.set("limit", String(q.limit));
    const query = params.toString();
    return get<ConversationsResponse>(`/conversations${query ? `?${query}` : ""}`, s);
  },
  conversation: (id: string, s?: AbortSignal) => get<{ conversation: ConversationView }>(`/conversations/${id}`, s),
  /// Start a conversation about something. The origin is checked by the node.
  /// `mode` only for a conversation about a checkout; 400 otherwise.
  createConversation: (body: NewConversationBody, s?: AbortSignal) =>
    post<{ conversation: ConversationView }>("/conversations", body, s),
  /// `title` is tri-state (absent keeps, `null` takes it away); `archived`
  /// puts it away or takes it back out; `mode` — 400 on a conversation that
  /// is not about a checkout (09-agents-in-the-ide).
  patchConversation: (id: string, body: PatchConversationBody, s?: AbortSignal) =>
    patch<{ conversation: ConversationView }>(`/conversations/${id}`, body, s),
  /// *Build this plan*: one act of the node's — a conversation in `plan`
  /// goes back to the mode it was in before the plan, which the record
  /// remembers. 409 when the conversation is not in `plan`.
  buildPlan: (id: string, s?: AbortSignal) =>
    post<{ conversation: ConversationView }>(`/conversations/${id}/plan/build`, undefined, s),
  deleteConversation: (id: string, s?: AbortSignal) => del<void>(`/conversations/${id}`, undefined, s),
  conversationMessages: (id: string, before?: PageBefore, limit?: number, s?: AbortSignal) =>
    get<{ scope: string; messages: MessageRow[]; reactions: ReactionRow[] }>(
      `/conversations/${id}/messages${page(before, limit)}`,
      s,
    ),
  /// Post into a conversation. `context` is the chips the person attached —
  /// nothing else is injected into the agent's turn.
  postConversationMessage: (
    id: string,
    body: NewMessageBody,
    s?: AbortSignal,
  ) => post<{ id: string }>(`/conversations/${id}/messages`, body, s),
  /// The turns in flight: each answering agent's words and thinking so far;
  /// empty when nothing runs. What a timeline that joins mid-turn reads once
  /// before the bus's `agent_streamed` frames.
  conversationLive: (id: string, s?: AbortSignal) => get<LiveTurnsResponse>(`/conversations/${id}/live`, s),
  /// The change ledger for a conversation about a checkout (09-agents-in-the-ide):
  /// one card per turn that changed something, folded from the reply
  /// downwards. 400 when the conversation is not about a checkout.
  conversationChanges: (id: string, s?: AbortSignal) => get<ChangesView>(`/conversations/${id}/changes`, s),
  /// One file's pending review — the server's hunks, never computed here.
  /// `{file: null}` when the path has no pending review.
  conversationChangeFile: (id: string, path: string, s?: AbortSignal) =>
    get<{ file: FileReviewView | null }>(`/conversations/${id}/changes/file?path=${encodeURIComponent(path)}`, s),
  /// Keep or undo a target: every pending file, a turn's, one file's, or one
  /// hunk's (with the `disk_hash` it was read at — 409 when it moved since).
  settleChanges: (id: string, body: SettleChangesBody, s?: AbortSignal) =>
    post<Settled>(`/conversations/${id}/changes/settle`, body, s),
  /// Undo everything a turn (and every later turn's overlap) left — a
  /// three-way take-back. Messages stay.
  restoreChanges: (id: string, turn: string, s?: AbortSignal) =>
    post<Settled>(`/conversations/${id}/changes/restore`, { turn }, s),
  /// The open asks in a conversation — a call the Tool & Commands Guard put
  /// to a person rather than deciding, in words.
  conversationAsks: (id: string, s?: AbortSignal) => get<AsksResponse>(`/conversations/${id}/asks`, s),
  /// Allow (once, or for the rest of this conversation — only when
  /// `grantable`) or deny an open ask, with an optional note.
  answerAsk: (
    id: string,
    ask: string,
    answer: AnswerAskBody,
    s?: AbortSignal,
  ) => post<AsksResponse>(`/conversations/${id}/asks/${ask}`, answer, s),

  // --- workstreams ----------------------------------------------------------
  /// A project's workstreams — the primary first — and where each checkout is.
  projectWorkstreams: (pid: string, s?: AbortSignal) =>
    get<{ project: string; workstreams: Workstream[]; checkouts: WorkstreamCheckout[] }>(
      `/projects/${pid}/workstreams`,
      s,
    ),
  /// Open a checkout on its own branch. `goal` defaults to the project's
  /// owner; any other goal must already be able to see the project.
  openWorkstream: (pid: string, body?: NewWorkstreamBody, s?: AbortSignal) => post<{ workstream: Workstream; path: string }>(`/projects/${pid}/workstreams`, body ?? {}, s),
  /// The open pull requests on the code host behind the project's `origin` —
  /// what a workstream can be opened from. Read fresh: the dialog asks on
  /// opening and on Refresh.
  projectPullRequests: (pid: string, s?: AbortSignal) =>
    get<{ project: string; prs: PullRequest[] }>(`/projects/${pid}/prs`, s),
  /// The project's workstream scripts (ide/07 §Workstream scripts): each
  /// phase's text and whether this machine has approved it. The texts are the
  /// `workstreams.script.*` settings, written through `setSettings("project", …)`.
  workstreamScripts: (pid: string, s?: AbortSignal) => get<WorkstreamScriptsView>(`/projects/${pid}/workstream-scripts`, s),
  /// The project's run command for a checkout (ide/18): what the Browser
  /// menu opens in a terminal — the text, whether this machine approved it,
  /// the checkout it runs in. 404 when the project sets none.
  runCommand: (wid: string, s?: AbortSignal) =>
    get<RunCommand & { workstream: string; cwd: string }>(`/workstreams/${wid}/run-command`, s),
  /// The folders of a checkout the node serves on loopback ports (ide/18).
  servers: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; servers: ServedFolder[] }>(`/workstreams/${wid}/servers`, s),
  /// Serve a folder of the checkout — `null` for the checkout itself — on a
  /// fresh loopback port. 409 when it is already served; 400 when it is not
  /// a folder of the checkout.
  serveFolder: (wid: string, folder: string | null, s?: AbortSignal) =>
    post<ServedFolder>(`/workstreams/${wid}/servers`, folder ? { folder } : {}, s),
  /// Stop one served folder.
  stopServer: (wid: string, id: string, s?: AbortSignal) =>
    del<{ stopped: ServedFolder }>(`/workstreams/${wid}/servers/${id}`, undefined, s),
  /// The file of the checkout a URL path of that server lands on — what
  /// makes an annotation on a served page a file chip. `null` when nothing
  /// is there.
  resolveServed: (wid: string, id: string, path: string, s?: AbortSignal) =>
    get<{ path: string | null }>(`/workstreams/${wid}/servers/${id}/resolve?path=${encodeURIComponent(path)}`, s),
  /// Every server the node has up, folders of checkouts and artifacts alike.
  allServers: (s?: AbortSignal) => get<{ servers: ServedFolder[] }>(`/servers`, s),
  /// Serve an artifact's named copy on a loopback port of its own, so the
  /// page opens in the embedded browser with an origin that is not the
  /// node's; `page` is the file's URL. Idempotent.
  serveArtifact: (sha256: string, name: string, s?: AbortSignal) =>
    post<ServedFolder>(`/artifacts/${encodeURIComponent(sha256)}/serve`, { name }, s),
  /// The browser tool calls agents parked for the embedded browser and
  /// nobody answered yet (ide/18) — read once at start; a live desktop hears
  /// each as a `browser_request` frame.
  browserRequests: (s?: AbortSignal) => get<{ requests: BrowserPending[] }>(`/browser/requests`, s),
  /// The desktop's answer to one browser request; 404 when nothing waits
  /// under that id. A screenshot rides as the `AttachmentRef` of a PNG
  /// uploaded through `uploadAttachment` first — never bytes here. A script's
  /// value is any JSON the page gave; `path` is the engine's to set.
  answerBrowserRequest: (id: string, result: Omit<BrowserResult, "value" | "path"> & { value?: unknown }, s?: AbortSignal) =>
    post<{ answered: boolean }>(`/browser/requests/${id}`, result, s),
  /// Mobile development (ide/19): what this machine has — Flutter, Xcode,
  /// the Android SDK, Flutter's doctor — the last look, or one run now.
  mobileDevelopmentStatus: (s?: AbortSignal) => get<MobileDevelopmentStatus>(`/mobile-development/status`, s),
  /// Examine the machine again (*Check again*), and answer the status.
  mobileDevelopmentCheck: (s?: AbortSignal) => post<MobileDevelopmentStatus>(`/mobile-development/status/check`, {}, s),
  /// The simulators, emulators and phones here, of the platforms that are on.
  mobileDevelopmentDevices: (s?: AbortSignal) => get<{ devices: MobileDevice[] }>(`/mobile-development/devices`, s),
  /// Boot a simulator or start an emulator; answers the device once up.
  mobileDevelopmentDeviceBoot: (id: string, s?: AbortSignal) => post<MobileDevice>(`/mobile-development/devices/${encodeURIComponent(id)}/boot`, {}, s),
  mobileDevelopmentDeviceShutdown: (id: string, s?: AbortSignal) => post<MobileDevice>(`/mobile-development/devices/${encodeURIComponent(id)}/shutdown`, {}, s),
  /// Bring the Simulator window forward.
  mobileDevelopmentDeviceShow: (id: string, s?: AbortSignal) => post<MobileDevice>(`/mobile-development/devices/${encodeURIComponent(id)}/show`, {}, s),
  /// Make an iOS simulator from a device type and a runtime the status lists.
  createSimulator: (body: CreateSimulator, s?: AbortSignal) => post<MobileDevice>(`/mobile-development/simulators`, body, s),
  /// Capture a device's screen as a PNG stored beside the store: the
  /// attachment, its named copy's path, its size.
  mobileDevelopmentScreenshot: (id: string, s?: AbortSignal) => post<MobileDevelopmentShot>(`/mobile-development/devices/${encodeURIComponent(id)}/screenshot`, {}, s),
  /// What a checkout holds for mobile: a Flutter app, its platform folders.
  workstreamMobileDevelopment: (wid: string, s?: AbortSignal) => get<MobileDevelopmentProject>(`/workstreams/${wid}/mobile-development`, s),
  /// One frame of a device's screen for the mirror — an `<img>` source, the
  /// token in the query as `attachmentUrl` carries it; JPEG for a simulator
  /// (Android answers PNG), and `seq` is a serial so no cache answers twice.
  /// The mobile run line has no method here: the Tauri shell reads that
  /// route, and the page never names a program.
  mobileDevelopmentFrameUrl: (id: string, seq: number) => withToken(`${apiBaseSync()}/mobile-development/devices/${encodeURIComponent(id)}/frame?format=jpeg&t=${seq}`),
  /// Approve the project's current scripts on this machine — the one write of
  /// the machine-scoped trust list. Answers the view, now trusted.
  approveWorkstreamScripts: (pid: string, s?: AbortSignal) =>
    post<WorkstreamScriptsView>(`/projects/${pid}/workstream-scripts/approve`, {}, s),
  /// Live status of one workstream (ide/07) — cached two seconds on the node.
  workstreamStatus: (wid: string, s?: AbortSignal) =>
    get<{ status: WorkstreamStatus }>(`/workstreams/${wid}/status`, s),
  /// Live status of every open workstream, for the switcher.
  allWorkstreamStatuses: (s?: AbortSignal) =>
    get<{ statuses: WorkstreamStatus[] }>(`/workstreams/status`, s),
  workstream: (wid: string, s?: AbortSignal) => get<WorkstreamDetail>(`/workstreams/${wid}`, s),
  /// Close a workstream. The checkout stays on disk unless `tree` is set.
  /// With `tree`, the checkout is removed too — consented, and what it held
  /// is saved under `refs/bisa/safety/` first (`recovery`).
  /// Close a workstream: the engine stops every session standing in it first
  /// (`stopped_sessions`); the desktop closes the tabs (`closeWorkstream.ts`).
  closeWorkstream: (wid: string, opts?: { tree?: boolean }, s?: AbortSignal) =>
    del<{ workstream: Workstream; removed_tree: boolean; recovery: Recovery | null; stopped_sessions: number }>(
      `/workstreams/${wid}${opts?.tree ? "?tree=true" : ""}`,
      undefined,
      s,
    ),
  workstreamDiff: (wid: string, s?: AbortSignal) =>
    get<WorkstreamDiff>(`/workstreams/${wid}/diff`, s),
  /// Push through the **Publish gate**: a gated project answers 202 with
  /// `{gate}` and nothing has left the machine until a human decides it (the
  /// gate is in the inbox); `auto` answers 200 with `pushed: true`; `manual`
  /// throws a 409 `ApiError`.
  pushWorkstream: (wid: string, s?: AbortSignal) =>
    post<PushOutcome>(`/workstreams/${wid}/push`, {}, s),
  /// Open a pull request on the code host behind `origin` — through the Publish
  /// gate. Send only what {@link codeHostCapabilities} allows; `prFormModel.mjs`
  /// builds the body from the form the capabilities produced.
  openPr: (wid: string, body: PrBody, s?: AbortSignal) => post<PrOutcome>(`/workstreams/${wid}/pr`, body, s),
  /// A pull request's title and body drafted by the core agent from the
  /// branch's commits and its diff against the base — the PR dialog's
  /// *Suggest*. **Always 200**: `{suggested: false, error}` is the no-harness,
  /// timed-out or nothing-beyond-the-base case. Nothing here pushes or opens:
  /// the session runs read-only with no MCP servers.
  suggestPr: (wid: string, s?: AbortSignal) => post<SuggestedPullRequest>(`/workstreams/${wid}/pr/suggest`, {}, s),

  // --- the code host (ide/08) ----------------------------------------
  /// What the code host behind a project's `origin` can do, or `code_host: null` for a
  /// project with no remote or one on a code host this build does not know.
  codeHostCapabilities: (pid: string, s?: AbortSignal) =>
    get<{ project: string; code_host: string | null; repo: RepoRef | null; capabilities: CodeHostCapabilities | null }>(
      `/codehost/capabilities/${pid}`,
      s,
    ),
  /// The workstream's pull request, read fresh from the code host; `pr: null` when none.
  workstreamPr: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; pr: PullRequest | null }>(`/workstreams/${wid}/pr`, s),
  workstreamPrChecks: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; checks: CheckRun[] }>(`/workstreams/${wid}/pr/checks`, s),
  /// The PR's submitted reviews and resolvable inline threads, read from the code host.
  workstreamPrReviews: (wid: string, s?: AbortSignal) =>
    get<{ workstream: string; reviews: ReviewSummary[]; threads: ReviewThread[] }>(`/workstreams/${wid}/pr/reviews`, s),
  /// Resolve or unresolve one review thread.
  workstreamPrResolveThread: (wid: string, threadId: string, resolved: boolean, s?: AbortSignal) =>
    post<{ workstream: string; thread: string; resolved: boolean }>(
      `/workstreams/${wid}/pr/threads/${encodeURIComponent(threadId)}/resolve`,
      { resolved },
      s,
    ),
  /// The person's reply on one review thread — their words untouched — and, with `resolve`, the thread resolved in the same act.
  workstreamPrReplyThread: (wid: string, threadId: string, body: string, resolve: boolean, s?: AbortSignal) =>
    post<{ workstream: string; thread: string; replied: boolean; resolved: boolean }>(
      `/workstreams/${wid}/pr/threads/${encodeURIComponent(threadId)}/reply`,
      { body, resolve },
      s,
    ),
  workstreamPrReview: (wid: string, body: PrReview, s?: AbortSignal) => post<{ workstream: string; reviewed: boolean }>(`/workstreams/${wid}/pr/review`, body, s),
  /// Merge — through the Publish gate: 200 merged, 202 the gate is open.
  workstreamPrMerge: (wid: string, strategy: MergeStrategy, deleteBranch: boolean, s?: AbortSignal) =>
    post<{
      workstream: string;
      merged: boolean;
      sha?: string | null;
      message?: string;
      gate?: unknown;
      status?: string;
      remote_branch_deleted?: boolean | null;
    }>(
      `/workstreams/${wid}/pr/merge`,
      { strategy, delete_branch: deleteBranch },
      s,
    ),
  /** The security policy as the node runs it — rules, problems, classifier readiness, the last decisions (redacted). */
  securityStatus: (s?: AbortSignal) => get<SecurityStatus>(`/security/status`, s),
  /** Try the redaction rules on a text, on a scratch vault. */
  redactPreview: (text: string, s?: AbortSignal) => post<RedactPreview>(`/security/redact-preview`, { text }, s),
  /** Try the guard rules on a tool call — no classifier, nothing recorded. */
  guardPreview: (tool: string, input: unknown, s?: AbortSignal) => post<GuardPreview>(`/security/guard-preview`, { tool, input }, s),

  /** The Decision-Making Agent as it stands on this node: its id, name and description, who answers for it, whether it is ready, every decision point's standing. Asks nobody. */
  decisionsStatus: (s?: AbortSignal) => get<DeciderStatus>(`/decisions/status`, s),
  /** The newest judgements this node asked for, newest first. */
  decisionsRecent: (limit?: number, s?: AbortSignal) =>
    get<JudgementRecord[]>(`/decisions${limit ? `?limit=${limit}` : ""}`, s),
  /** Put one request to the provider as it is set up. Nothing is decided and nothing is recorded. */
  decisionsTry: (request: DecisionRequest, s?: AbortSignal) => post<DecisionResponse>(`/decisions/try`, request, s),
  /** Keep a remote provider's (`jev`, `rlcd`) API key in this machine's keystore. Sent once; never read back. */
  setDecisionKey: (provider: string, key: string, s?: AbortSignal) =>
    put<{ key_stored: boolean }>(`/decisions/key/${provider}`, { key }, s),
  clearDecisionKey: (provider: string, s?: AbortSignal) =>
    del<{ key_stored: boolean }>(`/decisions/key/${provider}`, undefined, s),

  // --- the code host accounts, per kind (ide/08 §Credentials) — never a token
  /// One kind's stored accounts with where each token lives, whether the
  /// environment overrides them, git's helpers, the CLI's login, the default.
  codeHostAccounts: (kind: CodeHostKind, s?: AbortSignal) => get<AccountsView>(`/codehost/${kind}/accounts`, s),
  /// One kind's health for Settings: the CLI (installed, version, signed in
  /// as whom), the accounts, git's helper, the default, who would answer.
  codeHostHealth: (kind: CodeHostKind, s?: AbortSignal) => get<CodeHostHealth>(`/codehost/${kind}/health`, s),
  /// How to sign in to one kind: the CLI's browser login, the way to install
  /// the CLI, or a token from the host's page.
  codeHostLogin: (kind: CodeHostKind, s?: AbortSignal) => get<LoginPlan>(`/codehost/${kind}/login`, s),
  /// Ask the host whose a login's credential is, and which organizations it sees.
  checkCodeHostAccount: (kind: CodeHostKind, login: string, s?: AbortSignal) =>
    post<CodeHostConnection>(`/codehost/${kind}/accounts/${encodeURIComponent(login)}/check`, {}, s),
  /// Add an account: the token is verified with the host first and stored
  /// under the login the host answers — a refused token is a 401 and nothing
  /// is kept. Bitbucket's token is checked beside its `login`. Sent once;
  /// nothing reads it back.
  addCodeHostAccount: (kind: CodeHostKind, token: string, login: string | null, s?: AbortSignal) =>
    put<{ login: string; connection: CodeHostConnection }>(`/codehost/${kind}/accounts`, { token, login: login ?? undefined }, s),
  forgetCodeHostAccount: (kind: CodeHostKind, login: string, s?: AbortSignal) =>
    del<{ login: string; forgotten: boolean }>(`/codehost/${kind}/accounts/${encodeURIComponent(login)}`, undefined, s),
  /// The kind's default account — the global `codehost.<kind>.account` — or `null` to clear it.
  setDefaultCodeHostAccount: (kind: CodeHostKind, login: string | null, s?: AbortSignal) =>
    put<{ default: string | null }>(`/codehost/${kind}/default`, { login }, s),
  /// What a remote is before a project exists — a URL about to be cloned, or
  /// a folder the person picked. Offline.
  inspectRemote: (body: Inspect, s?: AbortSignal) => post<RemoteInspection>(`/codehost/inspect`, body, s),

  // --- connectors (03 §Connectors): definitions, this machine's accounts — never a secret value
  /// Every connector installed here, catalog and custom, with its operations and how many accounts this machine holds.
  connectors: (tag?: string | null, s?: AbortSignal) => get<{ connectors: ConnectorRow[] }>(`/connectors${tag ? `?tag=${encodeURIComponent(tag)}` : ""}`, s),
  /// One connector whole — the definition and its accounts (which fields are set, never a value).
  connector: (id: string, s?: AbortSignal) => get<ConnectorDetail>(`/connectors/${id}`, s),
  /// A custom definition. Validated first: a 400 carries the problems and nothing is written.
  createConnector: (def: ConnectorDefinition, s?: AbortSignal) => post<{ connector: ConnectorRow }>(`/connectors`, def, s),
  /// Replace a custom definition. A catalog one is 409.
  updateConnector: (id: string, def: ConnectorDefinition, s?: AbortSignal) => put<{ connector: ConnectorRow }>(`/connectors/${id}`, def, s),
  /// Remove a definition. 409 while an account or a workflow step still names it.
  deleteConnector: (id: string, s?: AbortSignal) => del<{ connector: string; removed: boolean }>(`/connectors/${id}`, undefined, s),
  /// Every problem a definition has, without saving it.
  validateConnector: (def: ConnectorDefinition, s?: AbortSignal) => post<ConnectorValidation>(`/connectors/validate`, def, s),
  /// Add an account (no `id`) or edit one (`id`). Secrets are written once, each field replaced, and never read back.
  putConnectorAccount: (id: string, body: NewConnectorAccount, s?: AbortSignal) => put<ConnectorAccountRow>(`/connectors/${id}/accounts`, body, s),
  /// Forget an account and every secret it held.
  deleteConnectorAccount: (id: string, aid: string, s?: AbortSignal) => del<{ account: string; forgotten: boolean }>(`/connectors/${id}/accounts/${aid}`, undefined, s),
  /// One request to the platform as this account — the connector's `check` operation — within `timeout_secs` (the node's 20 unsaid). The answer is kept as the row's `health`.
  checkConnectorAccount: (id: string, aid: string, timeout_secs?: number, s?: AbortSignal) =>
    post<AccountCheck>(`/connectors/${id}/accounts/${aid}/check`, timeout_secs ? { timeout_secs } : {}, s),
  /// Make this account the one a step runs as when it names none.
  setDefaultConnectorAccount: (id: string, aid: string, s?: AbortSignal) => put<{ default: string }>(`/connectors/${id}/accounts/${aid}/default`, {}, s),
  /// Begin an OAuth2 flow: the URL to open in the browser; the node listens on the callback port meanwhile.
  startConnectorOauth: (id: string, aid: string, s?: AbortSignal) => post<OAuthStart>(`/connectors/${id}/accounts/${aid}/oauth/start`, {}, s),
  /// Finish an OAuth2 flow with a code the person pasted — for a platform that cannot send the browser back.
  completeConnectorOauth: (id: string, aid: string, code: string, s?: AbortSignal) =>
    post<{ account: string; connected: boolean }>(`/connectors/${id}/accounts/${aid}/oauth/complete`, { code }, s),

  // --- profiles by organization (ide/04) ----------------------------------
  gitProfiles: (s?: AbortSignal) => get<GitProfilesView>(`/git/profiles`, s),
  putGitProfile: (slug: string, spec: ProfileSpec, s?: AbortSignal) =>
    put<GitProfileView>(`/git/profiles/${encodeURIComponent(slug)}`, spec, s),
  deleteGitProfile: (slug: string, s?: AbortSignal) => del<void>(`/git/profiles/${encodeURIComponent(slug)}`, undefined, s),

  // --- SSH for git hosts (ide/04) — public material only ----------------
  sshOverview: (s?: AbortSignal) => get<SshOverview>(`/git/ssh`, s),
  /// Generate an ed25519 key pair — no passphrase from the platform.
  sshGenerate: (key: SshNewKey, s?: AbortSignal) => post<SshPublicKey>(`/git/ssh/keys`, key, s),
  sshLoad: (name: string, s?: AbortSignal) => post<void>(`/git/ssh/keys/${encodeURIComponent(name)}/load`, {}, s),
  /// What ssh would do for a host — offline.
  sshResolve: (host: string, key?: string | null, s?: AbortSignal) =>
    get<SshResolved>(`/git/ssh/resolve?host=${encodeURIComponent(host)}${key ? `&key=${encodeURIComponent(key)}` : ""}`, s),
  /// One handshake with a git host; nothing is written on either side.
  sshTest: (host: string, user?: string | null, key?: string | null, s?: AbortSignal) =>
    post<HostGreeting>(`/git/ssh/test`, { host, user: user ?? undefined, key: key ?? undefined }, s),

  // --- what a checkout uses to reach its remote (ide/04 §The repository under About › Settings)
  gitConnection: (wid: string, s?: AbortSignal) => get<RepoConnection>(`/workstreams/${wid}/git/connection`, s),
  /// The three read-only probes: the code host, the SSH handshake, `git ls-remote`.
  checkGitConnection: (wid: string, s?: AbortSignal) => post<ConnectionCheck>(`/workstreams/${wid}/git/connection/check`, {}, s),
  /// Pin the repository to one account (its local `codehost.account`), or unpin it.
  setWorkstreamAccount: (wid: string, login: string | null, s?: AbortSignal) =>
    put<RepoConnection>(`/workstreams/${wid}/git/account`, { login }, s),
  /// The two whole texts of one file's change, for a comparison drawn side
  /// by side or inline (ide/04): the index and the working tree, or HEAD
  /// and the index. A side git does not hold is null; binary says so.
  gitSides: (wid: string, path: string, staged: boolean, s?: AbortSignal) =>
    get<FileSides>(`/workstreams/${wid}/git/sides?path=${encodeURIComponent(path)}${staged ? "&staged=true" : ""}`, s),
  /// A conflicted path whole (ide/04 §Conflicts, continued): its kind, its
  /// three sides in git's words, and the file as git wrote it with its
  /// markers, with the hash the save that follows carries.
  gitConflict: (wid: string, path: string, s?: AbortSignal) =>
    get<GitConflict>(`/workstreams/${wid}/git/conflict?path=${encodeURIComponent(path)}`, s),
  /// The operation git has left half-done, as facts read from its
  /// directory — the sides named by branch and commit, the step — or null.
  gitOperation: (wid: string, s?: AbortSignal) => get<GitOperationFacts | null>(`/workstreams/${wid}/git/operation`, s),
  /// What merging `source` into HEAD would do, before anything moves:
  /// clean, or the paths that would conflict; `supported: false` on a git
  /// before 2.38.
  gitMergePreview: (wid: string, source: string, s?: AbortSignal) =>
    get<GitMergePreview>(`/workstreams/${wid}/git/merge-preview?source=${encodeURIComponent(source)}`, s),
  /// Settle a conflicted path: with `take`, a side taken whole and staged,
  /// or the path removed (consented, a recovery ref first); without, *Mark
  /// resolved* — the merged text the person saved is staged, index only.
  gitResolve: (wid: string, path: string, take?: GitResolution | null, s?: AbortSignal) =>
    post<WorkstreamStaged | GitDone>(`/workstreams/${wid}/git/resolve`, { path, take: take ?? null }, s),

  // --- attachments --------------------------------------------------------

  /// **The one call in this module that is not JSON.**
  ///
  /// Everything else goes through `req`, which stringifies a body and parses a
  /// response. A file goes up as raw bytes with its name and type in the query,
  /// because base64 in JSON would cost a third more bytes and a parse of the
  /// whole thing at both ends.
  ///
  /// Uploading is separate from posting on purpose: a failed post does not
  /// re-send the file, the composer has something to show progress against, and
  /// attaching the same file twice costs one transfer because the store is
  /// content-addressed.
  uploadAttachment: async (file: File, s?: AbortSignal): Promise<AttachmentRef> => {
    const base = await resolveApiBase();
    const q = new URLSearchParams({
      name: file.name,
      mime: file.type || "application/octet-stream",
    });
    await resolveApiToken();
    const res = await fetch(`${base}/attachments?${q}`, {
      method: "POST",
      headers: authHeaders(),
      body: file,
      signal: s,
    });
    if (!res.ok) {
      const detail = await res.json().catch(() => null);
      throw new ApiError(
        detail?.error ?? tr("app-api-upload-failed", { status: res.status }),
        res.status,
        "/attachments",
        detail ?? undefined,
      );
    }
    return (await res.json()) as AttachmentRef;
  },

  /// Where the bytes are. `image` asks for inline rendering, which the node
  /// grants only if the file's magic number agrees — so a mislabelled file
  /// comes back as a download rather than as something the webview will run.
  attachmentUrl: (sha256: string, opts?: { image?: boolean }) =>
    withToken(`${apiBaseSync()}/attachments/${sha256}${opts?.image ? "?as=image" : ""}`),

  /// Ask a peer for bytes this machine does not have.
  fetchAttachment: (sha256: string, s?: AbortSignal) =>
    post<{ status: string }>(`/attachments/${encodeURIComponent(sha256)}/fetch`, {}, s),

  /// The bytes themselves, fetched with the bearer header — what every
  /// artifact viewer draws from (ide/12). Never a URL a frame is pointed at:
  /// the node serves nothing an agent wrote as a page.
  attachmentBytes: async (sha256: string, s?: AbortSignal): Promise<Uint8Array> => {
    const base = await resolveApiBase();
    await resolveApiToken();
    const res = await fetch(`${base}/attachments/${encodeURIComponent(sha256)}`, {
      headers: authHeaders(),
      signal: s,
    });
    if (!res.ok) {
      const detail = await res.json().catch(() => null);
      throw new ApiError(
        detail?.error ?? tr("app-api-fetch-failed", { status: res.status }),
        res.status,
        `/attachments/${sha256}`,
        detail ?? undefined,
      );
    }
    return new Uint8Array(await res.arrayBuffer());
  },

  /// The blob under a real name, made on demand inside the store — the file
  /// the file manager reveals and the default application opens.
  attachmentFile: (sha256: string, name: string, s?: AbortSignal) =>
    post<{ path: string }>(`/attachments/${encodeURIComponent(sha256)}/file`, { name }, s),

  // --- artifacts (ide/12) -------------------------------------------------

  /// One message wherever it lives, with its artifacts and their presence.
  message: (id: string, s?: AbortSignal) =>
    get<{ message: MessageRow }>(`/messages/${encodeURIComponent(id)}`, s),

  /// A conversation's artifacts, newest first — its gallery.
  artifactsOf: (scope: string, limit = 50, s?: AbortSignal) =>
    get<{ scope: string; artifacts: ArtifactListRow[] }>(
      `/artifacts/${encodeURIComponent(scope)}?limit=${limit}`,
      s,
    ),

  // --- work items, addressed by id alone ----------------------------------

  /// Every work item in the workspace, each with the home it is filed at —
  /// its goal, or the run of the workspace that made it — and its label.
  ///
  /// The workspace-wide list, for the same reason `allProjects` exists: a
  /// flat list built by asking every goal and run is N requests to draw one
  /// list, and the node already has the data.
  allWorkItems: (s?: AbortSignal) =>
    get<{ work_items: WorkItemRef[] }>("/work-items", s),

  /// One work item from its id alone: its home and label, the item, its
  /// latest result and whether a captured patch exists. A workbench URL
  /// carries only the item id, so this is the lookup that lets it render.
  workItem: (item: string, s?: AbortSignal) => get<WorkItemDetail>(`/work-items/${encodeURIComponent(item)}`, s),
  /// The captured patch of an item, as `text/x-patch`.
  resultUrl: (item: string) => withToken(`${apiBaseSync()}/work-items/${encodeURIComponent(item)}/result`),

  /// Every workstream in the workspace, with the project each belongs to.
  allWorkstreams: (s?: AbortSignal) =>
    get<{ workstreams: WorkstreamRef[] }>("/workstreams", s),

  // --- readiness ----------------------------------------------------------
  /// What the platform needs before it can work (16 — The setup gate): five
  /// checks with their state, words, official install hint and one-click fixes.
  readiness: (s?: AbortSignal) => get<Readiness>("/readiness", s),

  // --- harnesses ----------------------------------------------------------
  harnesses: (s?: AbortSignal) => get<{ harnesses: HarnessRow[] }>("/harnesses", s),
  /// What a harness lists: its models, each with the effort levels it takes,
  /// and the levels the harness takes for a model it does not list — an id
  /// typed by hand. An empty list of levels means it lists none.
  models: (harness: string, s?: AbortSignal) =>
    get<{ harness: string; efforts: Effort[]; models: ModelInfo[] }>(`/harnesses/${harness}/models`, s),
  /// What a harness's account has left — its usage windows from its own
  /// source, cached by the node; `refresh` asks the source again. Never a credential.
  harnessUsage: (harness: string, refresh = false, s?: AbortSignal) =>
    get<HarnessUsage>(`/harnesses/${harness}/usage${refresh ? "?refresh=true" : ""}`, s),
  /// The engine's model-health ledger, for cooldown badges. Live runtime
  /// state: an empty list means "nothing to report", never "no models".
  modelHealth: (s?: AbortSignal) =>
    get<{ models: ModelHealthRow[] }>("/models/health", s),

  // --- admin --------------------------------------------------------------
  governance: (s?: AbortSignal) => get<{ governance: Governance }>("/governance", s),
  setGovernance: (body: Governance, s?: AbortSignal) =>
    put<{ governance: Governance }>("/governance", body, s),

  // --- the IDE's files (ide/03) -----------------------------------------------
  /// A file for the editor: text, its hash (the `base_hash` a save carries)
  /// and whether it is within the editable size. 413 above the refusal cap.
  ideFile: (scope: FileScope, id: string, path: string, s?: AbortSignal) =>
    get<IdeFile>(`/ide/file/${scope}/${id}?path=${encodeURIComponent(path)}`, s),

  /// A file's bytes for a renderer (ide/03 §Rendered documents), fetched
  /// with the bearer header — never a URL an `<img>` or a `<video>` is
  /// pointed at. A 413 is an `ApiError` whose body carries `size` and `limit`.
  ideRaw: async (scope: FileScope, id: string, path: string, s?: AbortSignal): Promise<Uint8Array> => {
    const base = await resolveApiBase();
    await resolveApiToken();
    const route = `/ide/raw/${scope}/${id}?path=${encodeURIComponent(path)}`;
    const res = await fetch(`${base}${route}`, { headers: authHeaders(), signal: s });
    if (!res.ok) {
      const detail = await res.json().catch(() => null);
      throw new ApiError(detail?.error ?? tr("app-api-fetch-failed", { status: res.status }), res.status, route, detail ?? undefined);
    }
    return new Uint8Array(await res.arrayBuffer());
  },
  /// Compare-and-swap save; no `baseHash` creates. A 409 carries
  /// `current_hash` and `current_text` in its error body.
  ideWriteFile: (
    scope: FileScope,
    id: string,
    path: string,
    text: string,
    baseHash?: string | null,
    s?: AbortSignal,
  ) =>
    put<{ path: string; hash: string; created: boolean }>(
      `/ide/file/${scope}/${id}?path=${encodeURIComponent(path)}`,
      baseHash ? { text, base_hash: baseHash } : { text },
      s,
    ),
  ideCreate: (scope: FileScope, id: string, path: string, kind: "file" | "dir", s?: AbortSignal) =>
    post<{ path: string; kind: string }>(`/ide/files/${scope}/${id}`, { path, kind }, s),
  ideMove: (scope: FileScope, id: string, from: string, to: string, s?: AbortSignal) =>
    post<{ from: string; path: string }>(`/ide/files/${scope}/${id}/move`, { from, to }, s),
  /// Delete — to the OS trash or unlinked, as `editor.delete.trash` says for the root; the answer's `disposal` says which. One entry or many, always as one act — one move to the Trash, one sound — each entry with its own `recursive`; the answer lists every path that went and, when it halted, the first that did not. (`DELETE …?path=` stays the API's one-entry form, the CLI's.)
  ideDeleteMany: (scope: FileScope, id: string, entries: { path: string; recursive: boolean }[], s?: AbortSignal) =>
    post<{ ok: boolean; deleted: { path: string; disposal: Disposal }[]; failed: { path: string; reason: string } | null; disposal: Disposal }>(
      `/ide/files/${scope}/${id}/delete`,
      { entries },
      s,
    ),
  /// How a delete under this root goes right now, so a confirmation can say so first.
  ideDisposal: (scope: FileScope, id: string, s?: AbortSignal) =>
    get<{ disposal: Disposal }>(`/ide/files/${scope}/${id}/disposal`, s),
  /// Duplicate a file or a folder within the root; an existing target is refused.
  ideCopy: (scope: FileScope, id: string, from: string, to: string, s?: AbortSignal) =>
    post<{ from: string; path: string }>(`/ide/files/${scope}/${id}/copy`, { from, to }, s),
  /// Content search over a root (ide/03), streamed: `onHit` per matching line
  /// as ripgrep finds it, the summary when the walk ends. Aborting the signal
  /// closes the stream; the promise then resolves with what arrived so far.
  ideSearch: (scope: FileScope, id: string, params: SearchParams, onHit: (hit: SearchHit) => void, s?: AbortSignal) => {
    const q = new URLSearchParams({ q: params.q });
    if (params.regex) q.set("regex", "true");
    if (params.case) q.set("case", params.case);
    if (params.word) q.set("word", "true");
    if (params.include?.length) q.set("include", params.include.join(","));
    if (params.exclude?.length) q.set("exclude", params.exclude.join(","));
    if (params.limit) q.set("limit", String(params.limit));
    return sse<{ type: "hit"; hit: SearchHit } | ({ type: "done" } & SearchSummary) | { type: "error"; error: string }, SearchSummary>(
      `/ide/search/${scope}/${id}?${q}`,
      (frame, finish, fail) => {
        if (frame.type === "hit") onHit(frame.hit);
        else if (frame.type === "done") finish({ matches: frame.matches, files_with_matches: frame.files_with_matches, files_scanned: frame.files_scanned, truncated: frame.truncated });
        else fail(new Error(frame.error));
      },
      { matches: 0, files_with_matches: 0, files_scanned: 0, truncated: false },
      s,
    );
  },
  /// Every file path under a root, for quick open. Cached by the client and patched by `file_changed`.
  ideIndex: (scope: FileScope, id: string, s?: AbortSignal) =>
    get<{ paths: string[]; truncated: boolean }>(`/ide/index/${scope}/${id}`, s),
  /// The workbench layout saved for a root — window furniture, local to this node.
  ideLayout: (scope: FileScope, id: string, s?: AbortSignal) =>
    get<{ layout: unknown }>(`/ide/layout/${scope}/${id}`, s),
  ideSaveLayout: (scope: FileScope, id: string, layout: unknown, s?: AbortSignal) =>
    put<{ ok: boolean }>(`/ide/layout/${scope}/${id}`, layout, s),
  /// Watch a root; `file_changed` frames follow on the engine stream. Re-post to keep the lease.
  ideWatch: (scope: FileScope, id: string, s?: AbortSignal) =>
    post<{ watching: boolean; root: string; idle_secs: number }>(`/ide/watch/${scope}/${id}`, {}, s),

  // --- settings --------------------------------------------------
  /// Every setting the registry declares — what a panel is generated from.
  settingsRegistry: (s?: AbortSignal) => get<{ settings: SettingDef[] }>("/settings/registry", s),
  /// The raw values one scope holds — for an editor of a list that merges across scopes.
  settingsLayer: (scope: SettingScope, project?: string | null, s?: AbortSignal) =>
    get<{ scope: SettingScope; values: Record<string, unknown> }>(
      `/settings/${scope}${project ? `?project=${encodeURIComponent(project)}` : ""}`,
      s,
    ),
  /// Every key with its value and the scope it came from; `project` resolves
  /// for that project's layer as well.
  settingsResolved: (project?: string | null, s?: AbortSignal) =>
    get<{ project: string | null; settings: ResolvedSetting[] }>(
      `/settings/resolved${project ? `?project=${encodeURIComponent(project)}` : ""}`,
      s,
    ),
  /// Write keys at one scope. 400 names a key's allowed scopes or a value out of range.
  setSettings: (
    scope: SettingScope,
    values: Record<string, unknown>,
    project?: string | null,
    s?: AbortSignal,
  ) =>
    put<{ scope: SettingScope; settings: ResolvedSetting[] }>(
      `/settings/${scope}${project ? `?project=${encodeURIComponent(project)}` : ""}`,
      { values },
      s,
    ),
  /// Remove one key from one scope; the value falls back to the next layer.
  unsetSetting: (scope: SettingScope, key: string, project?: string | null, s?: AbortSignal) =>
    del<{ scope: SettingScope; setting: ResolvedSetting }>(
      `/settings/${scope}/${encodeURIComponent(key)}${project ? `?project=${encodeURIComponent(project)}` : ""}`,
      undefined,
      s,
    ),

  search: (q: string, s?: AbortSignal) =>
    get<{ results: SearchHit[] }>(`/search?q=${encodeURIComponent(q)}`, s),

  /// `GET /pause` — whether the engine is paused, read rather than guessed.
  paused: (s?: AbortSignal) => get<{ paused: boolean }>("/pause", s),
  pause: (s?: AbortSignal) => post<{ paused: boolean }>("/pause", {}, s),
  resume: (s?: AbortSignal) => post<{ paused: boolean }>("/resume", {}, s),

  // --- cache ---------------------------------------------------
  cacheStats: (s?: AbortSignal) => get<CacheStatsDto[]>("/cache/stats", s),
  /// `GET /logs` — the diagnostic log on this machine: the folder, each
  /// process family's files, the crash reports and the newest of them.
  logs: (s?: AbortSignal) => get<LogsView>("/logs", s),
  /// `GET /logs/crashes/{name}` — one crash report whole.
  crashReport: (name: string, s?: AbortSignal) => get<CrashReportView>(`/logs/crashes/${encodeURIComponent(name)}`, s),

  // --- network -------------------------------------------------
  /// `GET /network` — the `network.*` settings as the node holds them, what is in force, and what keeps it from being.
  network: (s?: AbortSignal) => get<NetworkStatus>("/network", s),
  /// `POST /network/check` — one request to `url` through the node's outbound client; 400 for a URL that is not `http(s)` or is this machine.
  networkCheck: (url: string, s?: AbortSignal) => post<NetworkCheck>("/network/check", { url }, s),
  clearCache: (s?: AbortSignal) => post<{ cleared: boolean }>("/cache/clear", {}, s),
};

export type { BudgetSpent, CatalogEntry, CatalogKind, Installed, McpServerView, SkillDef, TagFacet };
