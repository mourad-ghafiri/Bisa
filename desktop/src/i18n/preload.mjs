/**
 * `node --test --import ./src/i18n/preload.mjs …`: the English catalog is
 * installed before any test runs, so a model that says its words through
 * `l10n.t` says them in English — the sentences the tests compare.
 */
import { installEnglish } from "./testing.mjs";

installEnglish();
