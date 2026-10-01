/**
 * The right panel's rail: every occupant as an icon on a vertical strip at
 * the panel's right edge — Files · Git · Workstreams · Agent · About in
 * the model's order (`RAIL_GROUPS`), every tab at the same
 * spacing, the showing one marked on the rail's outer edge; the bar's
 * right-panel toggle is a square the rail's size, centred over the rail's
 * tabs. Icon only; the name and the chord are the tooltip, and the
 * accessible name. The rail is on screen whether or not the panel's column
 * is, and it never loses a tab: an occupant this root cannot show — Git on
 * a goal's folder, Agent while the centre already is the conversation — is
 * drawn muted, and pressing it asks the workbench through the same door
 * every other press uses (`showRightPanel`), so the workbench answers as it
 * would for a chord: the caret goes to the centre's composer, or the panel
 * settles on what it can show. Files is never more than one click away.
 *
 * Which icons exist and how they group is the model's (`railGroups`); the
 * press rule is the store's (`pressOccupant`: open · switch · close). The
 * strip itself is the one every rail wears (`ui/IconRail`); this file only
 * names the tabs.
 */
import { useKeymap } from "../../shell/useKeymap";
import { chordFor } from "../../shell/keymapModel.mjs";
import { Dot, IconRail, WorkingDot } from "../../ui";
import { railAnchor } from "../../ui/iconRailModel.mjs";
import { OCCUPANT_ICON } from "./occupantIcons";
import { badgeTooltip } from "./railBadgesModel.mjs";
import type { RailBadge } from "./railBadgesModel.mjs";
import { OCCUPANT_COMMAND, OCCUPANT_LABEL, railGroups } from "./rightPanelModel.mjs";
import type { Occupant } from "./rightPanelModel.mjs";
import { showRightPanel } from "./rightPanelStore";
import { t } from "../../i18n/l10n.mjs";

/** What a muted tab says: why this root cannot show it. */
function mutedWords(occupant: Occupant): string {
  return occupant === "agents" ? t("workbench-occupant-rail-conversation-centre") : t("workbench-occupant-rail-not-root");
}

export function OccupantRail({
  available,
  shown,
  open,
  onPress,
  badges = {},
}: {
  /** What this root can show; the rest is drawn muted. */
  available: readonly Occupant[];
  /** The occupant the column shows, or would show when opened. */
  shown: Occupant;
  /** Whether the column is showing: the rail marks an occupant only then. */
  open: boolean;
  onPress: (o: Occupant) => void;
  /** The marks per occupant (`railBadgesModel.mjs`): something to commit, a lifecycle step moving. */
  badges?: Partial<Record<Occupant, RailBadge | null>>;
}) {
  const keymap = useKeymap();
  // The groups are the model's order; the rail draws every tab at one
  // spacing — no gap and no rule between the groups.
  const tabs = railGroups().flat();
  const items = tabs.map((o) => {
    const isAvailable = available.includes(o);
    const showing = open && shown === o;
    const badge = badges[o] ?? null;
    const label = OCCUPANT_LABEL[o];
    const chord = chordFor(keymap, OCCUPANT_COMMAND[o]);
    const name = chord ? `${label} · ${chord}` : label;
    return {
      id: o,
      icon: OCCUPANT_ICON[o],
      label,
      tooltip: isAvailable ? badgeTooltip(name, showing, badge) : `${name} — ${mutedWords(o)}`,
      showing,
      muted: !isAvailable,
      badge:
        badge && isAvailable ? (
          <span aria-label={badge.title} className="inline-flex">
            {badge.tone === "working" ? <WorkingDot title={badge.title} /> : <Dot tone={badge.tone} title={badge.title} />}
          </span>
        ) : undefined,
    };
  });
  // The anchor is the showing occupant while the column is open and this
  // root can show it, else the first — a closed column must not make the
  // whole rail unreachable by keyboard.
  const anchor = railAnchor(tabs, { open: open && available.includes(shown), tab: shown });
  return (
    <IconRail
      label={t("workbench-occupant-rail-details")}
      items={items}
      anchor={anchor}
      onPress={(o) => (available.includes(o) ? onPress(o) : showRightPanel(o))}
    />
  );
}
