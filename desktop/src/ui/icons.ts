/**
 * Domain concepts to glyphs, in one place.
 *
 * The rule: **no view imports from `lucide-react`.** A screen asks for
 * `ICON.signal`, not for `Zap`, and the reason is drift. The moment two
 * screens each pick their own glyph for a signal, a signal stops being one
 * thing the reader recognises and becomes two things they have to learn. That
 * failure is invisible in review — each file looks fine on its own — and it
 * only shows up when someone uses both screens in the same minute. Importing
 * the icon library directly is exactly the move this file exists to prevent,
 * so if a concept has no entry here, add one rather than reaching past it.
 *
 * The maps are split by namespace for the same reason `Chip` splits its tone
 * maps: a goal state and a work-item state share words like "blocked", and
 * one flat record lets them silently borrow each other's glyph.
 *
 * Icons are `LucideIcon` components, not elements, so the caller controls
 * size and colour. The kit's convention is 14px inside dense rows and buttons
 * and 16px in headers; `strokeWidth` is left at the library default because
 * a thinner stroke at 14px starts to disappear against `surface-2`.
 */

import {
  Smartphone,
  MonitorSmartphone,
  Flame,
  Power,
  Camera,
  Archive,
  Sparkles,
  Globe,
  Network,
  Table2,
  Presentation,
  Maximize2,
  Minimize2,
  Save,
  History,
  Mic,
  FolderLock,
  Laptop,
  Layers,
  Sparkle,
  Moon,
  Hand,
  RotateCcw,
  AudioLines,
  ChevronsDownUp,
  FileArchive,
  Image as ImageIcon,
  Video,
  Mail,
  MailOpen,
  Reply,
  Sticker,
  Wrench,
  Bell,
  Expand,
  Flag,
  GitBranch,
  Split,
  ListTree,
  Repeat,
  GitFork,
  Hourglass,
  LayoutTemplate,
  ListChecks,
  Magnet,
  Map as MapIcon,
  Redo2,
  Undo2,
  Cherry,
  GitCommitHorizontal,
  ArrowRightLeft,
  GitBranchPlus,
  TagPlus,
  Waypoints,
  Workflow as WorkflowIcon,
  CircleHelp, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  FileText,
  ScrollText,
  GitPullRequest,
  CopyPlus,
  FolderSearch,
  ToggleLeft,
  ToggleRight,
  HardDrive,
  Palette,
  PawPrint,
  Radio,
  Scale,
  Gavel,
  Terminal,
  Braces,
  SquareTerminal,
  Plug,
  UserCheck,
  UsersRound,
  ArrowLeft,
  ArrowRight,
  PanelLeftClose,
  PanelLeftOpen,
  PanelLeft,
  PanelRight,
  Gauge,
  MemoryStick,
  Router,
  Activity,
  AtSign,
  Ban,
  Shield,
  ShieldCheck,
  FileClock,
  GitMerge,
  IterationCw,
  Bookmark,
  Bot,
  Check,
  Bold,
  Code,
  Heading,
  Italic,
  Link2,
  List,
  Quote,
  ChevronDown,
  ChevronUp,
  ChevronRight,
  Circle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  CircleCheck, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  CircleMinus, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  CirclePlus, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  CircleSlash, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  CircleX, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  Clock,
  ClipboardList,
  Copy,
  Cpu,
  Database,
  Download,
  File,
  Folder,
  FolderOpen,
  FolderTree,
  Ellipsis,
  EllipsisVertical,
  ExternalLink,
  Eye,
  EyeOff,
  WandSparkles,
  MousePointerClick,
  FolderKanban,
  Hammer,
  Kanban,
  Hash,
  Inbox,
  LayoutGrid,
  Info,
  KeyRound,
  Building2,
  Fingerprint,
  Library,
  Lightbulb,
  ListOrdered,
  ListFilter,
  LoaderCircle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  MessageSquare,
  MessagesSquare,
  NotebookPen,
  Package,
  Pin,
  Paperclip,
  Play,
  Plus,
  Puzzle,
  Blocks,
  GripHorizontal,
  RefreshCw,
  Scissors,
  ClipboardPaste,
  FolderInput,
  Search,
  Send,
  Server,
  Settings,
  Signature,
  Square,
  SquarePen,
  Star,
  StickyNote,
  Tag,
  Trash2,
  TriangleAlert,
  Unlink,
  UserRound,
  Users,
  X,
  Zap,
  type LucideIcon,
  Columns2,
  Rows2,
  SquareX,
  GitCompare,
  CornerDownRight,
  PenTool,
  Shapes,
  CirclePlay, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  CloudDownload,
  DiamondPlus,
  Timer,
  RotateCw,
} from "lucide-react";
import { fileIconKind, type FileIconKind } from "./fileIcons.mjs";

import type { FileEntryKind, GoalStatus, RunStatus, StepKind, StepState } from "../types";

/**
 * A goal's status, read as a story: a draft being shaped, a run moving, a run
 * waiting on a person, and the three endings. `closed` is a ring rather than
 * a cross because a goal that was abandoned or superseded is not a failure —
 * it is a decision, and it says which in its `closed` record.
 *
 * Typed as a total `Record`, so a status added in `core/goal.rs` fails the
 * build here rather than drawing nothing.
 */
export const GOAL_STATUS_ICON: Record<GoalStatus, LucideIcon> = {
  draft: Lightbulb,
  running: Play,
  waiting: Clock,
  done: CircleCheck, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  failed: CircleX, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  closed: CircleSlash, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
};

/**
 * The eighteen step kinds, one glyph each — the palette's vocabulary and
 * the node's badge. A start is a play button in a ring — the round glyph
 * an event wears; a wait an hourglass, an emit the signal's bolt, an end a
 * flag. A decide is a fork, an if a split, a switch a tree of cases, a
 * judge the Decision-Making Agent's gavel, a parallel the gateway's diamond with
 * a plus. A for-each is a repeat, a while a turn. An agent step is the
 * agent, a human step the person, an approval a signature; a check is a
 * checklist, a connector a plug, a notify a bell, a spawn a branching-off.
 */
/** The glyph for a step kind this build does not know. */
const STEP_ICON_FALLBACK: LucideIcon = Square;

/** A step kind's glyph, the stand-in for one the table does not name — never `undefined` as an element type. */
export function stepKindIcon(kind: string): LucideIcon {
  return STEP_KIND_ICON[kind as StepKind["kind"]] ?? STEP_ICON_FALLBACK;
}

export const STEP_KIND_ICON: Record<StepKind["kind"], LucideIcon> = {
  start: CirclePlay, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  agent: Bot,
  human: UserRound,
  approval: Signature,
  check: ListChecks,
  decide: GitFork,
  if: Split,
  switch: ListTree,
  judge: Gavel,
  parallel: DiamondPlus,
  for_each: Repeat,
  while: IterationCw,
  connector: Plug,
  wait: Hourglass,
  emit: Zap,
  notify: Bell,
  spawn: GitBranch,
  end: Flag,
};

/**
 * A boundary event's chip, by what it listens for: a timeout the timer, a
 * reminder the clockwise turn, a message the envelope, a signal the bolt —
 * the chips on a card's lower edge.
 */
export const BOUNDARY_ON_ICON = {
  after: Timer,
  every: RotateCw,
  message: Mail,
  signal: Zap,
} as const satisfies Record<string, LucideIcon>;

/**
 * What a boundary that does not divert does beside its live step: a post is
 * the arrow turning off to one side, an emit the signal's bolt. A divert
 * needs no glyph of its own: its chip carries the handle its path leaves by.
 */
export const BOUNDARY_ACT_ICON = {
  notify: CornerDownRight,
  emit: Zap,
} as const satisfies Record<string, LucideIcon>;

/**
 * A run's status, as a total `Record` so a status added in `core/run.rs`
 * fails the build here: queued is a numbered list (a place in line),
 * running and waiting are the goal's own glyphs for them, the rest the
 * step-state glyphs of the same words.
 */
export const RUN_STATUS_ICON: Record<RunStatus, LucideIcon> = {
  queued: ListOrdered,
  running: Play,
  waiting: Clock,
  done: CircleCheck, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  failed: CircleX, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  cancelled: CircleSlash, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
};

/**
 * A step's state inside a run. `running` is the spinner, the one state that
 * means something is happening right now; `skipped` is a minus rather than a
 * cross because an untaken branch is not a failure; `diverted` is the arrow
 * turning off — a boundary event took the run down its own path.
 */
export const STEP_STATE_ICON: Record<StepState["state"], LucideIcon> = {
  pending: Circle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  running: LoaderCircle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  waiting: Clock,
  done: CircleCheck, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  skipped: CircleMinus, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  failed: CircleX, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  cancelled: CircleSlash, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  diverted: CornerDownRight,
};

/**
 * Work-item states. `in_progress` is the spinner glyph deliberately: it is
 * the one state that means somebody is doing something right now, and it is
 * the only place in the kit where a static icon and an animated one share a
 * shape.
 */
export const WORK_ITEM_STATE_ICON: Record<string, LucideIcon> = {
  open: Circle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  claimed: Bookmark,
  in_progress: LoaderCircle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  blocked: Ban,
  review: Eye,
  accepted: CircleCheck, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  rejected: CircleX, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
};

/**
 * The three human gates. `approval` is a signature — adopting a workflow, an
 * approval step, an amendment; `escalation` a question raised mid-run;
 * `publish` work leaving the machine.
 */
export const GATE_ICON = {
  approval: Signature,
  escalation: TriangleAlert,
  publish: Send,
} as const satisfies Record<string, LucideIcon>;

/**
 * Everything else the app names.
 *
 * A channel is a hash, and a DM is a speech bubble, because that pairing is
 * what every chat tool has taught people to read; borrowing it costs nothing
 * and saves the reader a lesson. A core agent is a chip rather than a robot
 * so the two kinds of agent are distinguishable at 14px, where a variant of
 * the same robot would not be.
 */
export const ICON = {
  // Places
  inbox: Inbox,
  /// Work in progress, as a grid of it. Named for the concept rather than for
  /// the screen: this was `board` until the Board became Goals, and a glyph
  /// keyed to a screen name is a glyph that gets renamed with the screen.
  work: LayoutGrid,
  pulse: Activity,
  catalog: Library,
  workstream: Hammer,
  /**
   * Two states of the code side by side — a workstream against its base. Git
   * as the *tool* is not a glyph here but the Git mark (`ui/GitMark.tsx`).
   */
  compare: GitCompare,
  project: FolderKanban,
  /** The Board — the centre's third mode: every workstream a card in its column. */
  board: Kanban,
  settings: Settings,
  /// The library of workflows, and the designer. Its own glyph rather than
  /// `goal`: a workflow is the shape a goal runs, not the goal.
  workflow: WorkflowIcon,
  /// One execution of a workflow on a goal.
  run: Waypoints,
  /// Stop a goal or a workflow: the live run cancelled, the queue withdrawn.
  stop: Square,
  /// Restart: the live run cancelled and a new run of the same shape started.
  restart: RefreshCw,
  /// Runs waiting their turn behind the live one.
  queued: ListOrdered,
  /// A catalog workflow template — the shape before it is anybody's.
  template: LayoutTemplate,

  // Settings sections. Each is its own entry rather than borrowed from Things:
  // `members` is not `team`, and `node` is not `mcpServer`, however close the
  // glyphs look — a rail that reuses another concept's symbol teaches the
  // reader the wrong association.
  identity: KeyRound,
  appearance: Palette,
  pet: PawPrint,
  /// An addon: an overlay widget from a folder (18 — Addons).
  addon: Blocks,
  eye: Eye,
  eyeOff: EyeOff,
  /// The grip a floating window is dragged by.
  grip: GripHorizontal,
  members: UsersRound,
  sync: Radio,
  governance: Scale,
  /// The Decision-Making Agent, the third core agent: it judges in place of a place's own rule.
  decisions: Gavel,
  harness: Terminal,
  /// A login shell, distinct from a harness so a row can show both.
  shell: SquareTerminal,
  /// A listening TCP port a shell or harness opened (the rail's port chip).
  port: Plug,
  connector: Plug,
  /// The sidecar node — a local endpoint speaking on a port — in the Settings
  /// menu and the footer's read-out. Not a disk: that glyph is `disk`.
  node: Router,
  /// The diagnostic log — Settings › Node › Logging: what the node and the
  /// desktop write about themselves, on this machine.
  logs: ScrollText,
  /// The footer's machine read-outs, glyph and value only: CPU load, memory
  /// used over total, the platform's disk footprint.
  cpu: Gauge,
  gpu: Cpu,
  /** A harness account's usage — what it has left. */
  usage: Gauge,
  memory: MemoryStick,
  disk: HardDrive,
  /// A git branch by name — the footer's *where you are* line.
  branch: GitBranch,
  /// Settings › Capabilities › System — what this Mac lets the desktop app
  /// do. The machine itself, not `node` (the sidecar) and not `guard` (the
  /// rules): the grants are the operating system's.
  system: Laptop,
  network: Network,
  /** The whole stack the platform runs — the desktop app, the node, every session — as one of the footer's dimensions. */
  platform: Layers,
  /// Full Disk Access: a folder with a lock, the fence macOS puts around
  /// Library, Documents and other users' folders.
  fullDiskAccess: FolderLock,
  /// The microphone grant — voice mode's door, off until asked for.
  microphone: Mic,

  // Things
  agent: Bot,
  coreAgent: Cpu,
  team: Users,
  /// Who a piece of work is *for* — which may be an agent, a person or a
  /// team, so it borrows none of their three glyphs. Its own entry because
  /// assignment is its own concept: `person` is a human being and this is a
  /// relationship, and a header wearing `team` would say the answer is always
  /// a team, which is the exact assumption the assignee list replaced.
  assignee: UserCheck,
  channel: Hash,
  dm: MessageSquare,
  /// A conversation with agents — two bubbles, apart from a direct channel's
  /// one, so a conversation about a goal never reads as a message from a person.
  conversation: MessagesSquare,
  person: UserRound,
  skill: Puzzle,
  mcpServer: Server,
  // The bolt is the signal's — an event a workflow starts on, waits for or
  // raises — wherever one is drawn (a tab, a row, a rail, a chip).
  signal: Zap,
  goal: Circle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  tag: Tag,
  key: KeyRound,
  /// A code host's owner — an organization or a user — on a profile or a remote.
  organization: Building2,
  /// A code host account: a login whose token the platform holds.
  account: UserRound,
  /// An SSH key's fingerprint, and the key itself in a list.
  fingerprint: Fingerprint,
  mention: AtSign,
  /// A question waiting on a person. Distinct from `dm` and `mention`, which
  /// are about who is being addressed rather than that an answer is owed.
  question: CircleHelp, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  /// The asker's own recommendation, on one option of a question.
  ///
  /// A star rather than `check`, which already means *you picked this*: a
  /// recommended-but-unpicked option wearing a tick would read as already
  /// chosen, and the reader would send an answer they never gave. It is not
  /// `ok` either — that says a thing succeeded, and nothing here has happened
  /// yet. The star is always drawn beside the word "recommended", because the
  /// selected state owns `accent-soft` and a colour cannot carry a second
  /// meaning on the same row.
  recommended: Star,
  /// A written document — a brief, a runbook. `catalog` is a place and `edit`
  /// is an act; neither is the thing itself.
  document: FileText,
  /// A code host pull request.
  pullRequest: GitPullRequest,
  /// A tool call a session made.
  tool: Wrench,
  /// One assignment given to an agent. Not `ICON.goal`, which is the whole
  /// thing being pursued, and not a work-item *state* — those live in
  /// `WORK_ITEM_STATE_ICON` and answer a different question.
  workItem: ClipboardList,
  /// Something a run produced and left behind: a patch, a report, a build.
  /// A box rather than `attach`, which is the act of attaching one.
  /** A work item's captured result — the patch a copy workstream left. */
  result: Package,
  /** What an agent made for a person to look at (ide/12). */
  artifact: Sparkles,
  /** The artifact kinds' own glyphs; the rest reuse `image`, `video`, `audio`, `document`, `code`, `file`. */
  page: Globe,
  /** The project's run command — the Terminal menu's *Run* item (ide/18). */
  play: Play,
  /** A screenshot of a browser tab (ide/18). */
  camera: Camera,
  sheet: Table2,
  slides: Presentation,
  expand: Maximize2,
  collapse: Minimize2,
  save: Save,
  versions: History,
  /// The append-only record of what happened. `document` is a thing somebody
  /// wrote and can rewrite; a journal is a thing that only grows.
  journal: NotebookPen,
  /// Somebody's own scratchpad. Neither of its neighbours: a `journal` only
  /// grows and nobody owns it, and a `document` is a thing the work produced.
  /// A note is a thing a person keeps, and the only one of the three they can
  /// delete without argument.
  note: StickyNote,
  /// A drawing on the canvas (19 — Drawings): the pen a person draws with,
  /// beside the note they write — not `Palette`, which reads as colour, and
  /// not `Shapes`, which is the library's mark below.
  draw: PenTool,
  /// The shape libraries the canvas offers.
  shapes: Shapes,
  /// Kept at the top of a list by choice. Not `star`, which this app spends
  /// on nothing yet and which reads as a rating rather than a position.
  pin: Pin,

  // Markdown marks. These name what the *text* becomes, not what the button
  // does, which is why `link` is here rather than reusing `attach`: attaching
  // a file and marking a span as a link are different acts on different
  // things.
  bold: Bold,
  italic: Italic,
  heading: Heading,
  bullet: List,
  quote: Quote,
  code: Code,
  link: Link2,
  /// The engine's own bookkeeping for a goal — checkpoints, cursors, the
  /// files it needs to resume. Named for what it is rather than `state`,
  /// which a work item, a workstream and a run's step all use for their own.
  engineState: Database,

  // Files. A folder gets two glyphs, not one rotated chevron: the chevron
  // says *this row discloses something* and the folder says *this row is a
  // directory*, and collapsing them loses the second half on a row whose
  // chevron is busy being a spinner.
  // By media type, for a file somebody sent. `FILE_KIND_ICON` below answers
  // the other question — what a thing *is* in the workspace — and has no glyph
  // for "a PDF a colleague attached".
  image: ImageIcon,
  audio: AudioLines,
  video: Video,
  archive: FileArchive,
  folder: Folder,
  folderOpen: FolderOpen,
  /** A duplicate of a file or folder. */
  duplicate: CopyPlus,
  /** Show a path in the OS file manager. */
  reveal: FolderSearch,
  file: File,
  /** Bring a folder from this machine into the workspace. */
  import: FolderInput,
  /** Clone a repository into the workspace. */
  clone: GitFork,

  // Actions
  add: Plus,
  edit: SquarePen,
  /// The designer's own verbs. Undo and redo are the curved arrows every
  /// editor uses; fit frames the whole graph; snap and grid and minimap are
  /// the canvas's three switches.
  undo: Undo2,
  redo: Redo2,
  fit: Expand,
  snap: Magnet,
  minimap: MapIcon,
  layout: LayoutGrid,
  delete: Trash2,
  install: Download,
  /** A newer release of the platform, waiting on GitHub (`shell/UpdateDialog.tsx`). */
  update: CloudDownload,
  attach: Paperclip,
  detach: Unlink,
  search: Search,
  filter: ListFilter,
  more: Ellipsis,
  moreVertical: EllipsisVertical,
  notification: Bell,
  copy: Copy,
  /** The explorer's clipboard pair — `copy` above is the third. */
  cut: Scissors,
  paste: ClipboardPaste,
  /// A running tool's tier on the rail's pulse line: reading,
  /// writing inside the checkout, or executing — the same three the
  /// permission ceilings name.
  toolRead: Eye,
  toolWrite: SquarePen,
  toolExec: Terminal,
  send: Send,
  refresh: RefreshCw,
  open: ExternalLink,
  close: X,
  /// Ending a session, a harness or a shell — *Terminate*, *Abort*, *Stop*: one
  /// stop mark for the three verbs, the verb itself in the tooltip.
  terminate: Square,
  /// The Redactor, the Tool & Commands Guard and the Classifier — one shield
  /// for the three, in Settings › Security, on a guarded session and on the
  /// Pulse line a guard decision makes.
  guard: Shield,
  /// Safety (ide/04): the recovery ref every consented act writes first, and
  /// the *Saved to Safety first* line under a control — a checked shield,
  /// apart from the guard's plain one.
  safety: ShieldCheck,
  /// A file's history — the commits that touched it (ide/04 §Blame and history).
  history: FileClock,
  reply: Reply,
  /** A sub-agent under its session, a child under its parent. */
  subagent: CornerDownRight,
  /// Adding a reaction. `Sticker` rather than a face, because the picker
  /// offers arbitrary emoji and a smiley would promise only the happy ones.
  react: Sticker,
  /// The mark-as-read pair. Two glyphs rather than one toggled state, so a
  /// row can say which way the action goes without the reader inferring it.
  read: MailOpen,
  unread: Mail,
  /// Putting a path into git's index, and taking it back out. Their own
  /// entries rather than borrowing `add` and `delete`: staging creates
  /// nothing and unstaging destroys nothing, and a `Trash2` on an unstage
  /// control would read as "delete this file" — the one thing it must never
  /// be mistaken for.
  stage: CirclePlus, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  unstage: CircleMinus, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  /// Park the working tree's changes as a stash entry, and the stashes
  /// themselves (ide/04 §Stash): a box with a lid — not `archive`, which is a
  /// file somebody sent, and not `artifact`, which is what a run produced.
  stash: Archive,
  /// Throw a working-tree change away — back to the index, or to HEAD for an
  /// unmerged path (ide/04). Its own glyph: `Trash2` is *delete this file*,
  /// the one thing a discard must never be mistaken for, and `Undo2` is the
  /// designer's. A counter-clockwise arrow says *back to where it was*.
  discard: RotateCcw,
  /// The commit graph.s actions (ide/05, `commitActionsModel.mjs`), one glyph
  /// each so a hover row reads without its labels: a cherry for *cherry-pick*,
  /// the designer.s undo arrow for *revert* (undo with a new commit), a commit
  /// node for *checkout* (detach HEAD there), a branch with a plus for *branch
  /// here*, a tag with a plus for *tag here*, two arrows for *switch to a
  /// branch*, and an eye for *inspect*.
  cherryPick: Cherry,
  revert: Undo2,
  checkout: GitCommitHorizontal,
  branchHere: GitBranchPlus,
  tagHere: TagPlus,
  switchBranch: ArrowRightLeft,
  /// The Branches view's two ways of bringing branches together (ide/04,
  /// `branchActionsModel.mjs`): a merge joins, a rebase replays — one turn.
  merge: GitMerge,
  rebase: IterationCw,
  inspect: Eye,
  /// A document's rendered view — the eye on the view control (ide/03
  /// §Rendered documents); the same glyph as `inspect`, two concepts kept
  /// apart on purpose. Its neighbours on the control are `code` (the source)
  /// and `splitRight` (both at once).
  rendered: Eye,
  /// Annotate a rendered page for an agent (ide/03 §Annotate): the wand on
  /// the page's toolbar; `pick` is the pointer that chooses an element.
  annotate: WandSparkles,
  pick: MousePointerClick,
  /// A session's state marks (`sessionState.mjs`'s `icon:*`, drawn by
  /// `SessionMark`): a raised hand while it waits on you, a moon while it is
  /// parked, a sparkle while it thinks. Running, done and failed reuse
  /// `working`, `ok` and `danger`; aborted reuses `terminate`; idle `agent`.
  permission: Hand,
  parked: Moon,
  thinking: Sparkle,
  /// The two halves of one switch, so a row reads its own state.
  enabled: ToggleRight,
  disabled: ToggleLeft,

  // States that are not a domain state
  waiting: Clock,
  working: LoaderCircle, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  ok: CircleCheck, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  warn: TriangleAlert,
  danger: CircleX, // terminology-lint-ignore: circle - lucide-react icon identifier, third-party API
  info: Info,

  // Structure
  expanded: ChevronDown,
  collapsed: ChevronRight,
  /** A step moved up or down a list — the rebase editor's reorder. */
  up: ChevronUp,
  down: ChevronDown,
  /** Fold every folder of a tree. */
  collapseAll: ChevronsDownUp,
  /** A listing as folders — the Changes view's tree layout. */
  tree: FolderTree,
  /** A listing flat — the same glyph as `bullet`: a bulleted paragraph and a flat list are one shape. */
  list: List,
  check: Check,
  // The sidebar toggle. It gets its own glyph rather than borrowing one from
  // Places: a control wearing a destination's icon teaches the reader that the
  // button means that destination.
  /// Splitting the terminal panel, and closing one of its panes. Columns and
  /// rows say which way the cut goes; the pane close is a boxed × so it does
  /// not read as "close this tab".
  splitRight: Columns2,
  splitDown: Rows2,
  closePane: SquareX,
  panelOpen: PanelLeftClose,
  panelClosed: PanelLeftOpen,
  /** A browser tab kept out of sight — an agent browses in it, and nobody sees it (ide/18). */
  hidden: EyeOff,
  /** A phone or a tablet: a device an app runs on (ide/19). */
  device: Smartphone,
  /** A simulator or an emulator: a phone drawn on this machine's screen. */
  simulator: MonitorSmartphone,
  /** Hot reload: the running app takes the change without starting again. */
  hotReload: Flame,
  /** Hot restart: the app starts again with the change. */
  hotRestart: RotateCcw,
  /** Boot a simulator, or start an emulator. */
  boot: Power,
  /// The workbench's two side toggles: the project rail on the left, the
  /// right panel — one glyph each, pressed while the side is showing.
  panelLeft: PanelLeft,
  panelRight: PanelRight,
  back: ArrowLeft,
  forward: ArrowRight,
} as const satisfies Record<string, LucideIcon>;

/**
 * What a file *is*, from `GET /tree`'s annotation.
 *
 * This is why the tree route exists at all: under `goal` scope the node
 * hands back `artifact` and `journal` and `workstream` instead of `dir` and
 * `file`, and a folder listing that renders every row with the same grey
 * rectangle throws that away — it becomes a directory dump again, which is
 * the thing a person could already get from a terminal.
 *
 * `project` and `workstream` borrow `ICON.project` and `ICON.workstream` rather
 * than getting file-flavoured variants, because a domain concept wears one
 * glyph everywhere: the workstream in this tree and the workstream in the
 * Projects screen are the same workstream, and two glyphs would say otherwise.
 *
 * Typed as a total `Record`, so adding a variant to `FileEntryKind` in
 * `types.gen.ts` fails the build here rather than falling back to a blank.
 */
export const FILE_KIND_ICON: Record<FileEntryKind, LucideIcon> = {
  work: ICON.workItem,
  note: ICON.note,
  result: ICON.result,
  journal: ICON.journal,
  state: ICON.engineState,
  document: ICON.document,
  dir: ICON.folder,
  file: ICON.file,
};

/**
 * A glyph per kind of file (`fileIcons.mjs` says which kind a name is), so a
 * listing reads at a glance and a tab wears the same mark as its row.
 */
const FILE_ICON: Record<FileIconKind, LucideIcon> = {
  code: ICON.code,
  data: Braces,
  text: FileText,
  image: ICON.image,
  // The kinds the IDE renders as documents (ide/03) wear the artifact kinds' glyphs.
  pdf: ICON.document,
  video: ICON.video,
  audio: ICON.audio,
  sheet: ICON.sheet,
  document: ICON.document,
  slides: ICON.slides,
  archive: ICON.archive,
  config: ICON.settings,
  file: ICON.file,
};

/** The glyph for a file name. */
export function fileIcon(name: string): LucideIcon {
  return FILE_ICON[fileIconKind(name)];
}

export type IconName = keyof typeof ICON;
export type { LucideIcon };

/**
 * The glyph an Inbox row wears for the kind of thing it is — the same six
 * glyphs the sidebar, the Goals screen and the rail use for those things, so
 * a mention in a channel and a mention in the Inbox are one symbol.
 */
export const INBOX_KIND_ICON = {
  goal: ICON.goal,
  channel: ICON.channel,
  dm: ICON.dm,
  conversation: ICON.conversation,
  workstream: ICON.workstream,
  project: ICON.project,
  workflow: ICON.workflow,
  people: ICON.members,
  session: ICON.harness,
} as const;
