export declare const PRODUCT: string;
export interface AboutRow {
  label: string;
  value: string;
  copy: boolean;
}
export declare function versionLine(app: string, node: string | null | undefined): string;
export declare function versionCaution(app: string, node: string | null | undefined): string | null;
export declare function aboutRows(facts: { app: string; node: string | null | undefined; dataDir: string | null | undefined; npub: string | null | undefined }): AboutRow[];

export interface AboutLink {
  id: "website" | "source";
  label: string;
  url: string;
}
export declare function aboutLinks(homepage: string | undefined, repository: string | undefined): AboutLink[];
export declare function marksWords(): string;
export interface LicenceFiles {
  licence?: string | null;
  notices?: string | null;
}
export interface AboutFile {
  id: "licence" | "notices";
  label: string;
  path: string;
}
export declare function aboutFiles(files: LicenceFiles | null | undefined): AboutFile[];
