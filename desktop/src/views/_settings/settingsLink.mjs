/**
 * The Settings tab vocabulary, and the deep link other screens use to reach
 * one.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `ui/fileTreeModel.mjs`:
 * `node --test` imports it with no build step, and TypeScript reads the
 * declarations. It is a module rather than a constant inside `Settings.tsx`
 * because half a dozen screens outside Settings link into a panel — an empty
 * skill library points at `settings?tab=catalog-skill` — and importing the
 * lazily-loaded Settings screen to spell a link would pull the whole screen
 * into every caller's bundle.
 *
 * It holds the rail itself too — the groups, their panels and the words of
 * each (`SETTINGS_GROUPS`) — because where a panel is (`settingsPath`) is
 * said by those same callers, and a path spelt in a sentence must be the
 * path the rail shows.
 *
 * The ids are the `?tab=` values: each catalog kind is its own panel
 * (`catalog-agent` … `catalog-workflow`).
 */

import { t } from "../../i18n/l10n.mjs";

const panel = (id, label, blurb, needsWorkspace = false) => Object.freeze({ id, label, blurb, needsWorkspace });
const group = (id, label, panels) => Object.freeze({ id, label, panels: Object.freeze(panels) });

/**
 * The rail: its groups in order, and under each its panels — the id (the
 * `?tab=` value), the label the rail draws, the one line above the panel, and
 * whether the panel cannot draw without the workspace read. The groups run in
 * the order somebody sets a workspace up: themselves, then the people, then
 * where staff and procedures come from, then what the agents may use, what
 * guards them, who judges, the repositories, the IDE, what runs unattended,
 * and the engine underneath all of it.
 *
 * This is the one place the rail's words are: `Settings.tsx` draws from it
 * (and adds a glyph per panel), and `settingsPath` says where a panel is from
 * it — so a sentence that sends a person to a panel and the rail they then
 * read cannot disagree. Two panels may share a label under different groups
 * (*You › Identity*, *Git & code hosts › Identity*); the group is what tells
 * them apart, which is why a path always names it.
 */
export const SETTINGS_GROUPS = Object.freeze([
  group("you", t("screens-settings-words"), [
    panel("identity", t("screens-settings-identity"), t("screens-settings-keypair-account-there-no-server-knows"), true),
    panel("appearance", t("screens-settings-appearance"), t("screens-settings-six-independent-dials-theme-accent-density")),
    panel("pet", t("screens-settings-pet"), t("screens-settings-companion-floats-over-app-shows-what")),
    panel("notes", t("screens-settings-notes"), t("screens-settings-how-much-scratchpad-screen-how-behaves")),
    panel("draw", t("screens-settings-draw"), t("screens-settings-draw-blurb")),
  ]),
  group("workspace", t("screens-settings-workspace"), [
    panel("people", t("screens-channels-people"), t("screens-settings-people-other-nodes-workspace-hosts-each"), true),
    panel("sync", t("screens-settings-relays-sync"), t("screens-settings-relays-transport-never-authority-they-see"), true),
    panel("governance", t("screens-settings-governance"), t("screens-settings-which-gates-need-human-who-may")),
  ]),
  // Library sits above Capabilities on purpose: the catalog is where the
  // skills, agents, teams and channels the groups below list come from.
  group("library", t("screens-settings-library"), [
    panel("catalog-agent", t("screens-settings-agents"), t("screens-settings-agents-workspace-can-install-roster-comes")),
    panel("catalog-skill", t("screens-agents-skills"), t("screens-settings-skills-agent-can-given-installed-skills")),
    panel("catalog-team", t("screens-settings-teams"), t("screens-settings-teams-workspace-can-install-installing-one")),
    panel("catalog-channel", t("screens-settings-channels"), t("screens-settings-channels-workspace-can-install-agents-people")),
    panel("catalog-connector", t("screens-settings-connectors"), t("screens-settings-connectors-workspace-can-install-one-outside")),
    panel("catalog-workflow", t("screens-settings-workflows"), t("screens-settings-workflow-templates-goal-can-run-installing")),
    panel("addons", t("screens-settings-addons"), t("screens-settings-addons-blurb")),
  ]),
  group("capabilities", t("screens-settings-capabilities"), [
    panel("skills", t("screens-agents-skills"), t("screens-settings-procedures-agents-follow-held-once-referenced")),
    panel("mcp", t("screens-agents-mcp-servers"), t("screens-settings-tools-agent-may-given-registered-machine")),
    panel("connectors", t("screens-settings-connectors"), t("screens-settings-outside-platforms-workflow-s-connector-step")),
    panel("harnesses", t("screens-settings-harnesses"), t("screens-settings-coding-agents-machine-can-actually-run")),
    panel("system", t("screens-settings-system"), t("screens-settings-what-mac-lets-desktop-app-do")),
    panel("desktop", t("screens-settings-desktop"), t("screens-settings-desktop-app-s-own-behaviour-machine")),
    panel("network", t("screens-settings-network"), t("screens-settings-what-mac-s-network-whether-internet")),
    panel("browser", t("screens-settings-browser"), t("screens-settings-browser-built-into-desktop-app-tab")),
    panel("mobile-development", t("screens-settings-mobile-development"), t("screens-settings-flutter-apps-ios-simulator-android-emulator")),
  ]),
  // What never reaches an agent, what an agent may not run, and who reads
  // the doubtful cases: three panels over one `security.*` group.
  group("security", t("screens-settings-security"), [
    panel("security-redactor", t("screens-settings-redactor"), t("screens-settings-keys-tokens-values-rules-recognise-never")),
    panel("security-guard", t("screens-settings-guard"), t("screens-settings-tool-commands-guard-every-command-path")),
    panel("security-classifier", t("screens-settings-classifier"), t("screens-settings-classifier-blurb")),
  ]),
  // The Decision-Making Agent: it holds no record of its own, so who answers
  // for it, and whether it is on, is set here.
  group("decisions", t("screens-settings-decision-settings"), [
    panel("decision-making", t("screens-settings-decision-making"), t("screens-settings-decision-making-agent-judges-place-place")),
  ]),
  group("git", t("screens-settings-git-code-hosts"), [
    panel("git", t("screens-settings-identity"), t("screens-settings-global-git-config-profile-per-organization")),
    panel("git-ssh", t("screens-settings-ssh-keys"), t("screens-settings-key-each-organization-generate-here-put")),
    panel("github", t("screens-settings-github"), t("screens-settings-whether-mac-signed-github-github-cli")),
    panel("gitlab", t("screens-settings-gitlab"), t("screens-settings-whether-mac-signed-gitlab-gitlab-cli")),
    panel("bitbucket", t("screens-settings-bitbucket"), t("screens-settings-whether-mac-signed-bitbucket-accounts-platform")),
  ]),
  // Generated from the settings registry, one key prefix a panel: a key added
  // in `bisa-core` grows a control there with no change here.
  group("ide", t("screens-settings-project-ide"), [
    panel("ide", t("screens-settings-ide"), t("screens-settings-which-centre-workstream-opens-documents-terminals")),
    panel("editor", t("screens-settings-editor"), t("screens-settings-fonts-sizes-machine-s-tab-size")),
    panel("terminal", t("screens-settings-terminal"), t("screens-settings-shell-font-scrollback-which-harness-place")),
    panel("workstreams", t("screens-settings-workstreams"), t("screens-settings-how-checkout-backed-what-follows-merged")),
    panel("board", t("screens-settings-board"), t("screens-settings-every-workstream-card-five-columns-off")),
    panel("diagrams", t("screens-settings-diagrams"), t("screens-settings-mermaid-s-theme-how-diagram-exported")),
    panel("artifacts", t("screens-settings-artifacts"), t("screens-settings-what-page-agent-made-may-reach")),
    panel("agents", t("screens-settings-agents"), t("screens-settings-which-agent-project-reaches-default-how")),
    panel("keymap", t("screens-settings-keymap"), t("screens-settings-preset-overrides-keymap-two-settings-so")),
    panel("lsp", t("screens-settings-language-servers"), t("screens-settings-whether-servers-run-which-ones-machine")),
  ]),
  group("automation", t("screens-settings-automation"), [
    panel("events", t("screens-settings-events"), t("screens-settings-events-blurb")),
    panel("goals", t("screens-settings-goals"), t("screens-settings-how-new-goal-moves-auto-guided")),
    panel("workflow", t("screens-settings-workflows"), t("screens-settings-how-designer-behaves-machine-snapping-grid")),
    panel("budgets", t("screens-settings-budgets"), t("screens-settings-budgets-blurb")),
  ]),
  group("performance", t("screens-settings-performance"), [panel("cache", t("screens-settings-cache"), t("screens-settings-process-caches-how-long-each-reused"))]),
  group("node", t("screens-settings-node"), [
    panel("node", t("screens-settings-node"), t("screens-settings-engine-process-hosting-version-whether-anything")),
    panel("logging", t("screens-settings-logging"), t("screens-settings-what-node-desktop-write-about-themselves")),
  ]),
]);

/**
 * Every panel id, in rail order — the rail's own, read off its groups, so a
 * panel cannot be linked at and not drawn, nor drawn and not linkable.
 */
export const SETTINGS_TABS = Object.freeze(SETTINGS_GROUPS.flatMap((g) => g.panels.map((p) => p.id)));

/**
 * Where a panel is, as the rail shows it — *Settings › Capabilities ›
 * Browser*: the screen, the group, the panel. The one spelling a sentence, a
 * link or a hint uses to send a person there; said from inside Settings the
 * screen's own name is left off (`within`). An id that is no panel resolves
 * as a link to it would — to the first panel.
 *
 * @param {string} tab
 * @param {{ within?: boolean }} [opts]
 * @returns {string}
 */
export function settingsPath(tab, opts) {
  const id = settingsTab(tab);
  const g = SETTINGS_GROUPS.find((one) => one.panels.some((p) => p.id === id)) ?? SETTINGS_GROUPS[0];
  const p = g.panels.find((one) => one.id === id) ?? g.panels[0];
  const words = { group: g.label, panel: p.label };
  return opts?.within ? t("screens-settings-path-within", words) : t("screens-settings-path", words);
}

/**
 * Which panel a `?tab=` value selects.
 *
 * An unknown or absent value is not an error worth showing anybody: the hash
 * is user-editable and links outlive renames, so it resolves to the first
 * panel the way an unmatched route resolves to the Inbox.
 */
export function settingsTab(param) {
  return SETTINGS_TABS.includes(param ?? "") ? param : SETTINGS_TABS[0];
}

/**
 * The search patch for a link into one panel, composed with whatever extra
 * keys that panel reads — the catalog's own `?kind=` is the only one today.
 *
 * Empty and absent extras are dropped rather than written as `key=`, because
 * `settings?tab=catalog&kind=` is a URL a reader has to decide about and it
 * means exactly what no `kind` at all means.
 */
export function settingsSearch(tab, extra) {
  const patch = { tab };
  for (const [k, v] of Object.entries(extra ?? {})) {
    if (v === undefined || v === null || v === "") continue;
    patch[k] = v;
  }
  return patch;
}
