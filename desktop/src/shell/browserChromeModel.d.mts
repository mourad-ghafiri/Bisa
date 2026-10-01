/** Types for `browserChromeModel.mjs`, plain JavaScript so `node --test` reads it. */

export type LoadVerb = "reload" | "stop";

/** A verb of the bar: free, or held with the reason its tooltip wears. */
export interface BarVerb {
  enabled: boolean;
  why: string | null;
}

export interface BrowserChrome {
  back: BarVerb;
  forward: BarVerb;
  load: BarVerb & { verb: LoadVerb };
  address: true;
  wand: BarVerb;
  camera: BarVerb;
  openOutside: BarVerb;
  openInIde: boolean;
}

export declare const HELD: Readonly<{ blank: string; noBack: string; noForward: string; artifact: string }>;
export declare function chromeOf(facts: { blank: boolean; loading: boolean; canBack: boolean; canForward: boolean; inIde: boolean; atWorkbenchHome: boolean; annotatable: boolean; whyNot?: string | null }): BrowserChrome;
export declare function loadWords(verb: LoadVerb): string;
export declare function wandWords(wand: BarVerb, inspecting: boolean): string;
