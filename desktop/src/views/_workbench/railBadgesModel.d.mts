/**
 * Types for `railBadgesModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export type RailBadgeTone = "accent" | "danger" | "working";

/** A mark on a rail tab: its tone and the sentence that is its meaning. */
export interface RailBadge {
  tone: RailBadgeTone;
  title: string;
}

export interface GitBadgeStatus {
  git?: boolean;
  exists?: boolean;
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
}

export interface WorkstreamsBadgeFacts {
  kind: string | null | undefined;
  pr: { number: number } | null | undefined;
  busy: string | null | undefined;
  run: { run: { kind: string; agent: string }; starting: boolean; live: boolean; done: boolean } | null | undefined;
}

export declare function gitBadge(status: GitBadgeStatus | null | undefined): RailBadge | null;
export declare function workstreamsBadge(facts: WorkstreamsBadgeFacts): RailBadge | null;
export declare function badgeTooltip(name: string, showing: boolean, badge: RailBadge | null | undefined): string;
