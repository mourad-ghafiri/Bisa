export declare const DEFAULT_ACCOUNT: "";
export type StepAccount = string | { input: string } | null | undefined;
export declare function accountValue(account: StepAccount): string;
export declare function accountFromValue(v: string): string | { input: string } | null;
export declare function fixedValue(id: string): string;
export declare function inputValue(name: string): string;
export declare function strayParams(params: Record<string, string> | null | undefined, declared: readonly { name: string }[]): string[];
export declare function withParam(params: Record<string, string> | null | undefined, name: string, value: string): Record<string, string>;
/** The call under another connector: its operation, parameters, account and `unattended` gone. */
export declare function withConnector<T extends { connector?: string | null }>(call: T, connector: string | null): T;
/** The call under another operation: its parameters and `unattended` gone. */
export declare function withOperation<T extends { operation?: string | null }>(call: T, operation: string | null): T;
export declare function strayConnector(connector: string | null | undefined, installed: readonly { id: string }[]): string | null;
export declare function strayOperation(operation: string | null | undefined, offered: readonly { id: string }[], all: readonly { id: string }[] | null): string | null;
export declare function strayAccount(
  account: StepAccount,
  accounts: readonly { id: string }[] | null,
  inputs: readonly { name: string; kind: string; connector?: string | null }[],
  connector: string | null | undefined,
): string | null;
export declare function connectorPickerHint(state: { count: number; error: string | null | undefined }): string;
export declare function definitionHint(error: string | null | undefined, words: string): string;
export declare function accountsNote(state: { loading: boolean; error: string | null | undefined; count: number; inputs: number }): string | null;
export declare function writeWords(unattended: boolean): string;
/** What an empty parameter field would send — the definition's `default` in words — or nothing. */
export declare function paramHint(param: { name?: unknown; default?: unknown } | null | undefined): string;
export declare const UNATTENDED_LABEL: string;
export declare const UNATTENDED_HINT: string;
