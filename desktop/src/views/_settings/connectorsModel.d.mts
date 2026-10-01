import type { AccountCheck, AuthScheme, Connector, ConnectorAccountRow, ConnectorDefinition, ConnectorRow, ConnectorValidation, NewConnectorAccount, Origin, ResolvedSetting, SecretField } from "../../types";

export type Tone = "ok" | "warn" | "danger" | "quiet";
export type SchemeWord = "none" | "api_key" | "bearer" | "basic" | "oauth2" | "jwt";

export declare const SECRET_FIELDS_BY_SCHEME: Readonly<Record<SchemeWord, readonly SecretField[]>>;
export declare const SECRET_FIELD_LABEL: Readonly<Record<SecretField, string>>;
export declare function schemeWord(auth: string | AuthScheme | null | undefined): string;
export declare function secretFields(auth: string | AuthScheme | null | undefined): SecretField[];
export declare function isMultilineSecret(field: string): boolean;
export declare function needsAccount(auth: string | AuthScheme | null | undefined): boolean;
export declare function connectApplies(auth: string | AuthScheme | null | undefined): boolean;
export declare function redirectUri(port: number | string): string;
/** Whether a connector is the person's own definition (`"local"`), not the catalog's. */
export declare function isYours(origin: Origin | null | undefined): boolean;
export declare function originWords(origin: Origin | null | undefined): string;
export declare function connectorLine(row: ConnectorRow | null | undefined): string;
export declare function accountRows(accounts: ConnectorAccountRow[] | null | undefined): ConnectorAccountRow[];
export declare function secretsLine(row: ConnectorAccountRow | null | undefined, auth: string): { tone: "ok" | "warn" | "quiet"; text: string };
export declare function oauthLine(row: ConnectorAccountRow | null | undefined, now: number): string | null;
export declare function checkLine(check: AccountCheck | null | undefined, name?: string): { tone: Tone; text: string };
export declare function connectWords(port: number | string): string;
/** What the account form holds while it is open. */
export interface AccountDraft {
  label: string;
  values: Record<string, string>;
  secrets: Partial<Record<SecretField, string>>;
}
export declare function accountDraft(account: ConnectorAccountRow | null | undefined): { label: string; values: Record<string, string> };
/** `PUT /connectors/{cid}/accounts`, by name; a parameter held as a number or a switch and not retyped goes back as it was held. */
export declare function accountBody(draft: AccountDraft, account: ConnectorAccountRow | null | undefined): NewConnectorAccount;
export declare function typedSecrets(secrets: Partial<Record<SecretField, string>> | null | undefined): Partial<Record<SecretField, string>>;
export declare function maySaveAccount(state: { label: string; busy: boolean; definitionRead: boolean }): boolean;
export declare function withoutCheck<T>(checks: Record<string, T>, account: string): Record<string, T>;
export declare const OAUTH_PORT_KEY: "connectors.oauth.port";
/** The callback port as resolved, or null until the settings are read. */
export declare function oauthPort(resolved: readonly ResolvedSetting[] | null | undefined): number | null;
export declare function problemLines(validation: ConnectorValidation | null | undefined): string[];
export declare function parseDefinition(text: string): { ok: true; value: Record<string, unknown> } | { ok: false; error: string };
export declare function emptyDefinition(): Record<string, unknown>;
export declare const DEFINITION_KEYS: readonly (keyof ConnectorDefinition)[];
/** The definition of a connector installed here: the record's fields by name, without what the store stamps. */
export declare function definitionOf(connector: Connector): ConnectorDefinition;
export declare function addedWords(label: string): string;
export declare function defaultWords(label: string): string;
export declare function connectedWords(label: string): string;
export declare function secretsSetWords(fields: readonly string[]): string;
export declare function forgottenWords(label: string): string;
export declare function openedWords(): string;
export declare function saveRefusedWords(count: number): string;
