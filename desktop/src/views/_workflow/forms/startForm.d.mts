/** Types for `startForm.mjs`. */
import type { InputDef, StartOn, Step } from "../../../types";

type Start = Extract<Step, { kind: "start" }>;
/** Anything with steps and inputs: a draft, a stored workflow, a run's frozen copy. */
type Shape = { steps?: readonly Step[]; inputs?: readonly InputDef[] } | null | undefined;

export interface MappingRow {
  input: string;
  label: string;
  /** Required with no default: a run needs it, from the event or from the listening inputs. */
  required: boolean;
  /** What the start maps onto it; `null` for nothing. */
  template: string | null;
}

export interface GuardForm {
  overlap: "queue" | "skip" | "parallel";
  max: number;
  debounce: number;
}

export declare const MANUAL: "manual";
export declare const DEFAULT_EVERY_SECS: number;
export declare const DEFAULT_POLL_SECS: number;
export declare const DEFAULT_CRON: string;
export declare const PAYLOAD_FIELDS: Readonly<Record<StartOn["event"], readonly string[]>>;
export declare function eventOf(step: Step | null | undefined): StartOn["event"];
export declare function blankStartOn(event: StartOn["event"]): StartOn;
export declare function setStartEvent<S extends Start>(step: S, event: StartOn["event"]): S;
export declare function cadenceOf(on: { cron?: unknown } | null | undefined): "every" | "cron";
export declare function setCadence<O extends object>(on: O, cadence: "every" | "cron"): O;
export declare function payloadTemplate(field: string): string;
export declare function mappingSuggestions(event: StartOn["event"]): string[];
export declare function mappingRows(step: Step | null | undefined, inputs: readonly InputDef[] | null | undefined): MappingRow[];
export declare function setMapping<S extends Start>(step: S, input: string, template: string | null): S;
export declare function strayMappings(step: Step | null | undefined, inputs: readonly InputDef[] | null | undefined): string[];
export declare function guardOf(step: Step | null | undefined): GuardForm;
export declare function setGuard<S extends Start>(step: S, guard: GuardForm): S;
export declare function guardWords(step: Step | null | undefined): string;
export declare function eventPhrase(on: StartOn | null | undefined): string;
export declare function localHookPath(host: { workflow?: string | null; goal?: string | null } | null | undefined, step: string): string;
export declare function samplePayload(on: StartOn | null | undefined, now?: number): Record<string, unknown> | null;
export declare function inputPlaceholders(tmpl: string | null | undefined): string[];
export declare function startInputRefs(on: StartOn | null | undefined): string[];
export declare function startTemplates(on: StartOn | null | undefined): string[];
export declare function startSteps(wf: Shape): Step[];
export declare function manualEntry(wf: Shape): Step | null;
export declare function eventStarts(wf: Shape): Step[];
export declare function listens(wf: Shape): boolean;
export declare function listeningNeeds(wf: Shape): string[];
/** The inputs a host is asked when it begins listening: what `needs` names and no default fills, each required. */
export declare function listeningInputs(wf: { inputs?: InputDef[] } | null | undefined, needs?: readonly string[]): InputDef[];
/** The inputs a start asks: every input for a run by hand, what listening needs when it begins by listening. */
export declare function startInputs(wf: { inputs?: InputDef[] } | null | undefined, listen: boolean): InputDef[];
export declare function validSignalName(name: string | null | undefined): boolean;
