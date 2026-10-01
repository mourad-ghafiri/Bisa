/**
 * The desktop's view of the wire contract.
 *
 * Two sources, one import site: `types.gen.ts` is generated from the Rust
 * types by `just gen-types` (never edit it), and `types.hand.ts` mirrors the
 * few crates that don't derive `schemars::JsonSchema` yet. Everything else in
 * the app imports from here so the split stays an implementation detail.
 */

export * from "./types.gen";
export * from "./types.hand";
