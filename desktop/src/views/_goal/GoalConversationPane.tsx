/**
 * The goal's Conversation tab (13 — Conversations): the conversations about
 * the goal on the one surface every owner has (`ConversationSurface`) — the
 * goal's glyph and title in the bar, the **Conversations** door, the list,
 * *New conversation* in one click — with the goal's **own thread** standing
 * where nothing is picked (`fallback`), and as the list's first row, *The
 * goal's thread*, to come back to. The pick is the address's (`?conversation=`,
 * `?conversations=1`), so a link lands on a conversation and Back leaves it.
 *
 * One component wherever the goal's thread is drawn, so it is one
 * conversation with one draft (`bisa:draft:<goal>`), and a rule cannot hold
 * on one surface and not another. A path said here is looked up in the
 * attached projects' primary checkouts (ide/17).
 */

import { useMemo } from "react";
import { useWorkspace } from "../../shell/useWorkspaceData";
import type { Goal } from "../../types";
import { ICON, LinkRoots, type LinkRootRef } from "../../ui";
import { Conversation } from "../_studio/Conversation";
import { ConversationSurface } from "../_studio/ConversationSurface";
import { useConversationSurface } from "../_studio/useConversationSurface";
import { boardLabel } from "../_work/types";
import { goalLinkRoots } from "./goalLinks.mjs";
import { designs, modeOf } from "./goalMode.mjs";
import { t } from "../../i18n/l10n.mjs";

export function GoalConversationPane({
  goal,
  emptyHint,
}: {
  goal: Goal;
  /** The hint under an empty thread; the tab's own words when absent. */
  emptyHint?: string;
}) {
  const ws = useWorkspace();
  const linkRoots = useMemo<LinkRootRef[]>(() => goalLinkRoots(ws.projects, goal.id), [ws.projects, goal.id]);
  const hint =
    emptyHint ??
    (designs(modeOf(goal))
      ? t("goal-goal-conversation-pane-workflow-agent-posts-here-designs-answer")
      : t("goal-goal-conversation-pane-add-context-address-agent-get-started"));
  const c = useConversationSurface({ owner: { kind: "goal", id: goal.id } }, "route", { fallback: true });
  return (
    <LinkRoots roots={linkRoots}>
      <ConversationSurface
        surface={c}
        icon={<ICON.goal size={14} aria-hidden />}
        subject={boardLabel(goal)}
        fallback={{
          node: <Conversation scope={goal.id} kind="goal" placeholder={t("goal-goal-conversation-pane-talk-about-goal-address-agent-put")} emptyTitle={t("goal-goal-conversation-pane-nothing-said-yet")} emptyHint={hint} />,
          label: t("goal-goal-conversation-pane-own-thread"),
        }}
      />
    </LinkRoots>
  );
}
