/**
 * The One Band Rule (DESIGN.md › Layout): every index screen opens with the
 * kit's band — its tabs standing on the band's hairline, the primary create
 * action last — and the side columns give way before the work does.
 * Source-text facts across the files. Run with
 * `node --test desktop/src/scenarios/screenBar.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (rel) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

const INDEX_SCREENS = ["views/Inbox.tsx", "views/Agents.tsx", "views/Teams.tsx", "views/Workflows.tsx", "views/Goals.tsx", "views/Pulse.tsx", "views/Channels.tsx", "views/Messages.tsx"];

test("every index screen opens with the kit's band, and a tab strip in it stands on the band's line", () => {
  for (const file of INDEX_SCREENS) {
    const src = read(file);
    assert.ok(src.includes("<ScreenBar"), `${file}: the band`);
    // A view switch in the band is the kit's strip with `bare`: one hairline, the band's.
    const tabs = src.match(/<Tabs\b[^>]*>/g) ?? [];
    for (const tag of tabs) assert.ok(src.slice(src.indexOf(tag), src.indexOf(tag) + 40).includes("bare"), `${file}: ${tag.slice(0, 40)} stands bare in the band`);
    // The hand-built bars it replaced are gone.
    assert.ok(!src.includes('border-b border-hairline px-6 py-2') && !src.includes('border-b border-border px-6 py-2') && !src.includes('px-6 pt-1'), `${file}: no hand-built band`);
  }
  const bar = read("ui/ScreenBar.tsx");
  assert.ok(bar.includes("min-h-11") && bar.includes('"@container shrink-0 border-b border-hairline"') && bar.includes("px-6"), "44px, the hairline, the index gutter");
  assert.ok(bar.includes('stack && "@max-5xl:order-last @max-5xl:basis-full"') && read("views/Workflows.tsx").includes("<ScreenBar\n        stack"), "a screen with much to narrow gives its tabs the base line on a narrow band");
  assert.ok(bar.indexOf("{children &&") < bar.indexOf("{end &&") && bar.includes("ml-auto"), "what narrows, then the right edge's count, tools and primary");
  assert.ok(bar.includes("h-11 min-w-0 items-stretch self-end") && bar.includes("h-11 shrink-0 items-center gap-2 self-start"), "wrapped, the tabs keep the base and the primary the first line");
  const strip = read("ui/Tabs.tsx");
  assert.ok(strip.includes('bare ? "min-w-0 flex-1 items-stretch" : "items-center border-b border-hairline"'), "a bare strip draws no line of its own");
});

test("the primary create action is the band's last control, at its right edge", () => {
  for (const [file, primary] of [
    ["views/Agents.tsx", 'screens-agents-new-agent'],
    ["views/Teams.tsx", 'screens-teams-new-team'],
    ["views/Workflows.tsx", 'screens-workflows-new-workflow'],
    ["views/Goals.tsx", 'screens-goals-new-goal'],
    ["views/Channels.tsx", 'screens-channels-new-channel'],
    ["views/Messages.tsx", 'screens-messages-new-message'],
  ]) {
    const src = read(file);
    const band = src.slice(src.indexOf("<ScreenBar"), src.indexOf("</ScreenBar>") > 0 ? src.indexOf("</ScreenBar>") : src.indexOf("/>", src.indexOf("end={")));
    const end = band.slice(band.indexOf("end={"));
    assert.ok(end.includes(primary), `${file}: the primary is in the band's right edge`);
    const after = end.slice(end.lastIndexOf(primary));
    assert.ok(!/<Button\b/.test(after), `${file}: nothing follows the primary`);
  }
});

test("side columns give way before the work: the Details pane leaves the screen its least, the designer folds its palette", () => {
  const aux = read("shell/auxPaneModel.mjs");
  assert.ok(aux.includes("Math.min(share, Math.floor(room) - SCREEN_MIN_WIDTH)"), "the pane's bound leaves the screen beside it its least");
  const designer = read("views/WorkflowDesigner.tsx");
  assert.ok(designer.includes("fitColumns({ total: rowWidth, fixed: 40, rail: PALETTE_WIDTH") && designer.includes("compact={paletteCompact}") && designer.includes("width: fit.right ?? panelWidth"), "the designer's row is fitted as the IDE's is");
  const tab = read("views/_workflow/GoalWorkflowTab.tsx");
  assert.ok(tab.includes("fitColumns({ total: rowWidth, fixed: 0, rail: PALETTE_WIDTH") && tab.includes("compact={paletteCompact}") && tab.includes("width: fit.right ?? INSPECTOR_WIDTH"), "a goal's Workflow tab is fitted the same way");
  const palette = read("views/_workflow/Palette.tsx");
  assert.ok(palette.includes('aria-label={compact ? k.label : undefined}') && palette.includes("workflow-palette-named-drag-onto-canvas"), "a folded kind keeps its name for the ear and the tooltip");
  const step = read("views/_goal/StepRow.tsx");
  assert.ok(step.includes("grid-cols-[auto_minmax(0,1fr)_auto]") && step.includes("@md:grid-cols-[auto_minmax(0,1fr)_auto_auto_auto]") && read("views/_goal/RunSteps.tsx").includes('<ul className="@container'), "a narrow step list keeps the step's name");
});

test("what a screen repeats stays quiet: pending is words, a tag bar folds, an empty state's door is raised", () => {
  const chip = read("ui/Chip.tsx");
  assert.ok(chip.includes('if (name === "pending") return <span className="text-2xs whitespace-nowrap text-text-dim">'), "a pending step is dim words, never a chip");
  const tags = read("ui/Tags.tsx");
  assert.ok(tags.includes("const FACETS_SHOWN = 6;") && tags.includes("facets.length > FACETS_SHOWN + 1") && tags.includes("value.selected.includes(tag)") && tags.includes('tr("ui-tags-more-suggestions", { count: hidden })'), "six tags, then N more from two hidden up; a tag that is on always shows");
  const empty = read("ui/EmptyState.tsx");
  assert.ok(empty.includes("[&_[data-variant=ghost]]:border-border") && read("ui/Button.tsx").includes('data-variant={variant ?? "default"}'), "an empty state draws a ghost door raised");
  for (const file of ["views/Agents.tsx", "views/Teams.tsx"]) assert.ok(read(file).includes("@2xl:sticky @2xl:top-0 @2xl:max-h-[calc(100vh-11rem)] @2xl:self-start @2xl:overflow-y-auto"), `${file}: the detail stays in view beside the roster`);
  for (const file of ["views/Workflows.tsx", "views/_workflow/TemplateGallery.tsx"]) assert.ok(read(file).includes('<h2 className="mb-2 text-sm font-semibold text-text capitalize">'), `${file}: a domain is an h2 under the chrome's h1`);
});

test("Agents and Teams reach their catalog from the band, and a team's page offers no goal of its own", () => {
  for (const [file, kind] of [["views/Agents.tsx", "catalog-agent"], ["views/Teams.tsx", "catalog-team"]]) {
    const src = read(file);
    const band = src.slice(src.indexOf("<ScreenBar"), src.indexOf("</ScreenBar>"));
    assert.ok(band.includes(`settingsSearch("${kind}")`) && band.includes("<ICON.catalog size={12} aria-hidden />"), `${file}: Catalog in the band, to its own kind`);
    assert.ok(band.indexOf(`settingsSearch("${kind}")`) < band.lastIndexOf('variant="primary"'), `${file}: before the screen's one primary`);
  }
  const teams = read("views/Teams.tsx");
  assert.ok(!teams.includes("screens-teams-new-goal-team") && !teams.includes("screens-teams-carrying") && !teams.includes("onNewGoal"), "no New goal for this team, no Carrying section");
  assert.ok(!read("views/_work/NewGoalDialog.tsx").includes("setPendingTeam"), "and no team handed to the New goal dialog");
});
