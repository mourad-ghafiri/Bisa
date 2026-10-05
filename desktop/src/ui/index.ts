/**
 * The kit, from one import site.
 *
 * Every view imports from `../ui` and nothing else — not from a component
 * file, and never from `lucide-react`, `@radix-ui/*` or `motion` directly.
 * That is what makes swapping an implementation (a hand-rolled menu for a
 * Radix one, a bespoke toaster for sonner) a change to one file instead of a
 * change to forty-three, and it is why this file is a wall of re-exports
 * rather than a barrel someone deletes for being redundant.
 */

export { cn } from "./cn";
export { ICON, GOAL_STATUS_ICON, RUN_STATUS_ICON, STEP_KIND_ICON, STEP_STATE_ICON, BOUNDARY_ON_ICON, BOUNDARY_ACT_ICON, WORK_ITEM_STATE_ICON, GATE_ICON, INBOX_KIND_ICON, FILE_KIND_ICON, fileIcon, stepKindIcon } from "./icons";
export { HARNESS_MARK, harnessMark } from "./harnessMarks";
export { PlatformMark } from "./PlatformMark";
export { GitMark } from "./GitMark";
export type { Mark, MarkProps } from "./harnessMarks";
export type { MarkId } from "./harnessMarkModel.mjs";
export type { IconName, LucideIcon } from "./icons";
export { EASE, LIST_ITEM_MOTION, durations, useMotionTiming } from "./motion";

export { Button } from "./Button";
export type { ButtonProps } from "./Button";
export { Chip, SessionStateChip, StepStateChip, WaitBadge, WorkItemStateChip } from "./Chip";
export type { Tone } from "./Chip";
export { copyImage, copyText } from "./clipboard";
export { Meter } from "./Meter";
export { StackedBar } from "./StackedBar";
export type { Band, LegendEntry } from "./StackedBar";
export type { MeterTone } from "./Meter";
export { modelWords, nameWithModel } from "./modelWords.mjs";
export type { ModelWords } from "./modelWords.mjs";
export { Card, ErrorNote, ReadLine, Section, Spinner } from "./Card";
export { failureReason, failureText, sayFailure } from "./failure";
export { EmptyState } from "./EmptyState";
export { Tile } from "./Tile";
export { Pending, Skeleton, SkeletonRows } from "./Skeleton";
export { ImmediateIndicators } from "./indicatorBeat";
export { PageHeader } from "./PageHeader";
export { CountBadge, Dot, WorkingDot } from "./Badge";
export { forgetPhotoThumb, usePhotoThumb } from "./photoThumbs";
export { scalePhoto } from "./photoScale";
export { PHOTO_PROFILES, photoOfPrincipal } from "./photoModel.mjs";
export type { PhotoProfile } from "./photoModel.mjs";
export { PhotoField } from "./PhotoField";
export { Flash } from "./Flash";
export { duration, durationPrecise } from "../i18n/format.mjs";
export { ErrorBoundary, OverlayBoundary, overlayClosedWords } from "./ErrorBoundary";
export { KeyHint, isMac, keyLabel } from "./KeyHint";
export { SectionHeader } from "./SectionHeader";
export { expandAll, isCollapsed, setCollapsed, toggleCollapsed, useCollapsed, useCollapsedUnder } from "./collapsedStore";
export { CHROME_FALLBACK, DOCK_SIZE, dockBox, dockStyle, placementFrom, placementOf, samePlacement, useDockDrag, useDockViewport } from "./Dock";
export { useDockClearance, useDockFootprint, useDockOverlap } from "./dockClearance";
export { useArrivals, useCleared } from "./useArrival";
export type { Box, DockDrag, Placement, Viewport } from "./Dock";
export { ResizeHandle, useStoredSize } from "./SplitPane";
export { IconRail } from "./IconRail";
export type { IconRailItem } from "./IconRail";
export { PaneDivider } from "./PaneDivider";
export type { DividerGeometry } from "./PaneDivider";
export { ChoiceDialog } from "./ChoiceDialog";
export type { Choice } from "./ChoiceDialog";
export { ConfirmDialog, Dialog, PromptDialog } from "./Dialog";
export { useWatchLease } from "./useWatchLease";
export { ToastProvider, toaster, useToast } from "./Toast";
export { Tabs } from "./Tabs";
export { ScreenBar } from "./ScreenBar";
export type { TabDef } from "./Tabs";
/** Closeable document/terminal tabs. `Tabs` is the fixed view switcher. */
export { TabStrip, StripControlButton } from "./TabStrip";
export * as sessionState from "./sessionState.mjs";
export { SessionMark, sessionGlyph } from "./SessionMark";
export type { SessionWord, SessionTone } from "./sessionState.mjs";
export type { StripTab } from "./TabStrip";
export { Menu } from "./Menu";
export type { MenuItem } from "./Menu";
export { ContextMenu } from "./ContextMenu";
export { ChoiceMenu } from "./ChoiceMenu";
export type { MenuChoice } from "./ChoiceMenu";
export { Popover } from "./Popover";
export { Tooltip, TooltipProvider } from "./Tooltip";
export { SegmentedControl } from "./SegmentedControl";
export { FoldedText } from "./FoldedText";
export { StreamCaret, ThinkingBlock } from "./ThinkingBlock";
export { StreamedMarkdown } from "./StreamedMarkdown";
export { SecretInput, SecretTextArea } from "./SecretInput";
export type { Segment } from "./SegmentedControl";
export { Slider } from "./Slider";
export { Switch } from "./Switch";
export { Separator } from "./Separator";
export { DayDivider, dayKey, dayLabel } from "./DayDivider";
export { ScrollArea } from "./ScrollArea";
export { Avatar, PrincipalTag, principalColor } from "./Avatar";
export { VirtualList } from "./VirtualList";
export { useThemeNonce, useTokenPx } from "./useTokenPx";
export { useEditorTypography, editorTypographyFrom, type EditorTypography } from "./useEditorTypography";
export { colorToHex, resolvedRoles } from "./cssColor";
/** Named `FileTreeView` because `FileTree` is the wire type; see the file. */
export { FileTreeView } from "./FileTree";
export { revealLabel } from "./fileTreeMutations.mjs";
// Drag and drop — the one door to `@dnd-kit`.
export { DragProvider, DropZone, SortableList, useActiveDrag, useDragGhost, useDragSource } from "./dnd";
export { cycle, docTabDrag, dragType, hunkDrag, isDragOf, moveIndex, navRowDrag, pathDrag, railRowDrag, terminalTabDrag, workstreamCardDrag } from "./dnd";
export type { DocTabDrag, DragData, DropEvent, HunkDrag, NavRowDrag, PathDrag, RailRowDrag, SortableHandle, TerminalTabDrag, WorkstreamCardDrag } from "./dnd";
// The tree primitive both the explorer and the project rail are drawn with.
export { TreeList, childrenOf, filesUnder, orderedChildren, parentsFromDepth, pathTree } from "./tree";
export type { DropPlan, PathChild, PathNode, TreeDrag, TreeKeyAction, TreeRowLike, TreeRowState } from "./tree";
export {
  closeExplorerSearch,
  isExplorerCommand,
  openExplorerSearch,
  registerExplorer,
  requestReveal,
  sendExplorerCommand,
  useExplorerSearch,
} from "./explorerStore";
export type { ExplorerCommand } from "./explorerStore";
export { useFileClipboard } from "./fileClipboardStore";
export { FileView } from "./FileView";
export { formatSize } from "./fileTreeModel.mjs";
export { STANDING_KINDS, foldStandings, standingHint, standingMark, standingTone } from "./fileStandingModel.mjs";
export type { FileStanding, Standing, StandingKind, StandingTone } from "./fileStandingModel.mjs";
export { AnimatedList } from "./AnimatedList";
export { Markdown } from "./Markdown";
export { LinkedText } from "./LinkedText";
export { ExternalLink } from "./ExternalLink";
export { LinkCard } from "./LinkCard";
export type { LinkCardVerb } from "./LinkCard";
export { LinkHandlerContext, LinkRoots, LinkRootsContext, delegateLinkClick, hitOfAnchor, useLinkHandler, useLinkRoots } from "./linkContext";
export type { LinkHandler, LinkHit, LinkPointer, LinkRootRef } from "./linkContext";
export { addressWords, confirmListing, findLinks, linkifyHtml, parseAddress, relativeUnder, resolveLink, urlWords } from "./linkModel.mjs";
export type { DocResolution, LinkResolution, LinkRoot, LinkSpan, PathSpan, UnlistedResolution, UrlSpan } from "./linkModel.mjs";
export { Checkbox, CopyText, DateInput, Field, NumberInput, Select, TextArea, TextInput } from "./Field";
export { Labelled } from "./Labelled";
export { RelativeTime, LiveDuration, absolute, relative } from "./RelativeTime";
export {
  NO_TAG_FILTER,
  parseTagFilter,
  TAG_VOCABULARY,
  TagChips,
  TagFilterBar,
  TagInput,
  normalizeTag,
  passesTagFilter,
} from "./Tags";
export type { TagFilterState, TagMatch } from "./Tags";
export { Composer, focusComposer, forgetEveryDraft } from "./Composer";
export { PendingFiles } from "./PendingFiles";
export { useUploads } from "./useUploads";
export { usePastedImages } from "./usePastedImages";
export type { PastedImages } from "./usePastedImages";
export type { PendingFile, Uploads } from "./useUploads";
export type { ComposerFiles, ComposerStop, Mentionable } from "./Composer";
export { AgentChip, AgentPicker, AgentRow } from "./AgentPicker";
export type { AgentCandidate } from "./AgentPicker";
export { MermaidView } from "./MermaidView";
export { ArtifactView } from "./artifact/ArtifactView";
export { ArtifactCard } from "./artifact/ArtifactCard";
export { ArtifactStage } from "./artifact/ArtifactStage";
export { useArtifactBytes, loadArtifactBytes, textOf } from "./artifact/artifactBytes";
export { KindView } from "./artifact/KindView";
export { MoreMenu } from "./MoreMenu";
// Find, and find-and-replace, as one fact and one bar for every surface that reads a document.
export { FindBar } from "./find/FindBar";
export type { FindBarProps } from "./find/FindBar";
export { useDomFind } from "./find/useDomFind";
export { endsSelection } from "./selectionModel.mjs";
export { useKeptScroll, yieldKeptScroll } from "./useKeptScroll";
export type { KeptScroll } from "./useKeptScroll";
export { compileFind, countWords, emptyFind, hasQuery, matchesOf, replaceAll, replaceOne, stepIndex } from "./find/findModel.mjs";
export type { Find, Match } from "./find/findModel.mjs";
export type { KindBytes } from "./artifact/KindView";
export { PageFrame } from "./artifact/PageFrame";
export { usePageInspector } from "./artifact/usePageInspector";
export { useInspectorTheme } from "./artifact/inspectorTokens";
export type { PageInspectorHandlers } from "./artifact/usePageInspector";
export type { InspectMode, InspectorMark, InspectorMessage, InspectorNote, InspectorPick, InspectorRect } from "./artifact/pageInspector.mjs";
export { useIdeFileBytes } from "./artifact/fileBytes";
export type { FileBytesState } from "./artifact/fileBytes";
export {
  ARTIFACT_KINDS,
  artifactFromFile,
  artifactKey,
  artifactVersions,
  bytesWords,
  kindOf,
  inlinePreview,
  isTextKind,
  kindWords,
  parseArtifactKey,
  versionLabel,
  versionWords,
} from "./artifact/artifactModel.mjs";
export type { VersionGroup } from "./artifact/artifactModel.mjs";
export { isMermaidPath, mermaidBlocks, offsetError } from "./mermaidModel.mjs";
export { CodeEditor } from "./CodeEditor";
export type { CodeEditorProps, EditorSelection, GutterEntry, EditorCaret } from "./CodeEditor";
export { DiffEditor } from "./DiffEditor";
export type { DiffAnnotation, DiffAnnotationAction, DiffEditorProps } from "./DiffEditor";
// The flow canvas is the kit's second entry, `./flow` — deliberately not
// re-exported here, so `@xyflow/react` stays out of the main chunk.
export { CURSOR_RING, FOCUS_RING, PANE_RING } from "./rings";
