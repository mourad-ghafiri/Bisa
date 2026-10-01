/**
 * Types for `settingsLink.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { SearchPatch } from "../../router";

/** A `?tab=` value: one of the panels the Settings rail draws. */
export type SettingsTab =
  | "identity"
  | "appearance"
  | "pet"
  | "notes"
  | "draw"
  | "people"
  | "sync"
  | "governance"
  | "catalog-agent"
  | "catalog-skill"
  | "catalog-team"
  | "catalog-channel"
  | "catalog-connector"
  | "catalog-workflow"
  | "addons"
  | "skills"
  | "mcp"
  | "connectors"
  | "harnesses"
  | "system"
  | "desktop"
  | "network"
  | "browser"
  | "mobile-development"
  | "security-redactor"
  | "security-guard"
  | "security-classifier"
  | "decision-making"
  | "git"
  | "git-ssh"
  | "github"
  | "gitlab"
  | "bitbucket"
  | "ide"
  | "editor"
  | "terminal"
  | "workstreams"
  | "board"
  | "diagrams"
  | "artifacts"
  | "agents"
  | "keymap"
  | "lsp"
  | "events"
  | "goals"
  | "workflow"
  | "budgets"
  | "cache"
  | "node"
  | "logging";

/** One panel of the rail: its `?tab=` id, its label, the line above it. */
export interface SettingsPanel {
  id: SettingsTab;
  label: string;
  /** What the panel is for, in the one line above it. */
  blurb: string;
  /** True where the panel cannot draw without `WorkspaceInfo`. */
  needsWorkspace: boolean;
}

/** One group of the rail, in rail order. */
export interface SettingsGroup {
  id: string;
  label: string;
  panels: readonly SettingsPanel[];
}

/** The rail: its groups, and under each its panels. */
export declare const SETTINGS_GROUPS: readonly SettingsGroup[];

export declare const SETTINGS_TABS: readonly SettingsTab[];

/** *Settings › Group › Panel* as the rail shows it (*Group › Panel* from `within` Settings). */
export declare function settingsPath(tab: SettingsTab, opts?: { within?: boolean }): string;

/** Which panel a `?tab=` value selects; anything unknown gets the first. */
export declare function settingsTab(param: string | null | undefined): SettingsTab;

/** The search patch for a link into one panel, plus that panel's own keys. */
export declare function settingsSearch(
  tab: SettingsTab,
  extra?: Record<string, string | number | undefined | null>,
): SearchPatch;
