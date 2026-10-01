# bisa-decision

The Decision-Making Agent's providers, behind one port. The contract is the domain's
(`bisa-core`'s [`decision`](../15-decision-making-agent.md#the-contract) module): typed questions against a
state in, `{ model, answers, usage }` out. This crate owns what stands between a decision point and a
model that answers it — a calibrated model over its own wire, a generative model held to the same
shape, and the three decorators every answer passes through before a caller sees it. It depends on
`bisa-core` for the contract and on `bisa-connectors` for the outbound HTTP port, the host rules and
the un-printable `Secret` — a decision endpoint is an outside platform's API like any other. Nothing
here reads a setting, a keystore or a clock of its own; the engine hands in a `DecisionSettings` and
a `Ports` for as long as one judgement takes.

---

## Where things live

| Module | Owns |
|---|---|
| `provider.rs` | the port — `DecisionProvider { descriptor() -> ProviderDescriptor { kind, model }, decide(&request, deadline) }` — and `ProviderError { Misconfigured, Refused { status, message }, Busy { status, retry_after }, Unreachable, Unreadable, Contract(#[from] DecisionContractError), TimedOut }` with `is_transient()` (`Busy`, `Unreachable`, `Unreadable`, `Contract` — asking again may get an answer; `Misconfigured`, `Refused`, `TimedOut` never do) and `retry_after()`; `ProviderDescriptor::calibrated()` |
| `system_one.rs` | `SystemOneProvider` — a model trained for calibrated decisions, over its own wire: `POST <endpoint>/v1/systemone` with `{ state, model, questions }`, a bearer key when one is given; `JEV_ENDPOINT` (`https://api.typesafe.ai`) and `SYSTEM_ONE_PATH`; `SystemOneProvider::new` refuses — before any request — an endpoint that is no URL, names no model, or is a host the node's `HostJudge` will not reach (the same `https`-or-loopback rule a connector's host passes); a 2xx body is parsed strictly against the contract; 408, 429 and 5xx read as `Busy` (`Retry-After` honoured); anything else is `Refused` with the key scrubbed from the message |
| `prompted.rs` | `PromptedProvider` — a generative model held to the same shape, asked through the engine's [`Asker`] port (`AskTarget::{Harness { harness, model }, Agent(id)}`; `model_name()` for the record when the reply names none): one fixed prompt (`prompt(&request)` — the state, every question with its options or levels, and the exact JSON the answer must be), the reply read strictly (`read_answers`, `first_object` — the first balanced `{…}` of the text, braces inside strings not counted); `reply_schema()` is `{answers}` over `bisa_core::answers_schema()`, handed to a harness that can hold a model to a shape. `usage` is always zero; the descriptor's `kind` (`Harness` or `Agent`) is never calibrated |
| `resilient.rs` | the three decorators every provider is wrapped in, each a `DecisionProvider` over another so the stack is built once: `Checked` (the request validated before it is sent, the response checked against it after — a broken response is `Contract`, transient, since a generative model asked again may hold to the shape), `RetryBudget` (`new(retries)`: 0.5 s base doubled per attempt, capped at 5 s, `wait(attempt, retry_after, jitter)` — the service's own `Retry-After` when it sent one, else the doubled base less up to a quarter of jitter, both under the cap) and `Retrying` (asks again after a transient failure inside the deadline it was given; a wait that would outlive the deadline is not taken and the last error is the answer), `Bounded` (one `tokio::time::timeout` over the whole call, every attempt and every wait) |
| `factory.rs` | the one place a provider kind is matched — `build(&DecisionSettings, &Ports) -> Result<Box<dyn DecisionProvider>, ProviderError>`: `harness` and `agent` build a `PromptedProvider` over the engine's `Asker`; `jev` and `rlcd` build a `SystemOneProvider`, the key read through `KeySource` and refused by name when none is stored; every kind returns wrapped in `Bounded(Retrying(Checked(...)))`. `DecisionSettings` (`provider`, `harness`, `harness_model`, `agent`, `jev_model`, `rlcd_endpoint`, `rlcd_model`, `rlcd_auth`, `retries`; `Default` is `harness` / `claude-code` / `claude-sonnet-5-5[1m]`; the effort the judge's session runs at is the engine's to read, `decisions.harness.effort`, and is no field here), `RlcdAuth { None, Bearer }` (`FromStr`), `KeySource` (`key(provider) -> Option<Secret>`) + `NoKeys`, `Ports<'a> { transport, hosts, keys, asker, clock, entropy }` — everything a provider needs for one judgement, on loan |
| `scripted.rs` | `ScriptedProvider` — the fake every test above this crate answers from: a queue of turns (`Scripted = Result<BTreeMap<String, DecisionAnswer>, ProviderError>`), `answers(id, answer)`, `choice(option, options, confidence)` (a lead option with the rest spread evenly), `of_kind(kind)`, `asked()` — every request it was put, in order, so a test reads what a caller sent as well as what it got back |
| `lib.rs` | the re-exports |
| `error_text.rs` | `Localize for ProviderError` — how a provider's failure is said to a person (`error-decision-…`) |

---

## Entry points

The engine builds one `Ports` per judgement (`crates/bisa-engine/src/decider.rs::ask`) — a
`ReqwestTransport` over its shared HTTP clients, a `PolicyHostJudge` scoped to the one host a
provider's endpoint declares, its keystore as `KeySource`, its one-shot session as `Asker`,
`SystemClock` and `OsEntropy` — reads the `decisions.*` settings into a `DecisionSettings`, and calls
`build(&settings, &ports)`. The provider it gets back is `decide`d once, within the deadline
`decisions.deadline_secs` names. A test substitutes `ScriptedProvider` in the engine's `DeciderState`
stand-in, or a `ScriptedTransport` under a real `SystemOneProvider`; nothing in either path reaches a
platform.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A request that breaks the contract is never sent, and a response that breaks it is a transient error, never a guess a caller could mistake for an answer | `tests/it/factory.rs` (`Checked` unit tests in `resilient.rs`) |
| A judgement is asked again only for a transient failure — `Busy`, `Unreachable`, `Unreadable`, a broken contract — never for `Refused` or `Misconfigured`, and never past the deadline it was given | `resilient.rs` unit tests, `tests/it/resilient.rs` |
| The wait between attempts doubles from a half second under a five-second cap, obeys a short `Retry-After` and caps a long one | `resilient.rs` — `the_wait_doubles_under_the_cap_and_obeys_the_service_under_it_too` |
| A System One endpoint is refused before any request when it names no URL, no model, or a host the node will not reach; a refusal's message never contains the key | `tests/it/system_one.rs` |
| A generative provider's reply is read as the first balanced JSON object in the text, whatever surrounds it, and a reply with no object is `Unreadable` | `prompted.rs` unit tests |
| Switching `decisions.provider` is the only thing that moves: the same `Ports` and the same `DecisionSettings` build a different provider for `harness`, `agent`, `jev` and `rlcd`, each already wrapped in the same three decorators | `tests/it/factory.rs` |
| No test here reaches a platform, a keychain or a network beyond a loopback stub | `tests/it/support.rs` (the loopback stub, `ScriptedTransport`) |

---

## Errors

`ProviderError` as above; the engine reads `is_transient()` to decide whether `Retrying` asks again,
and folds every outcome into a `Judgement` — `applied`, `unsure` or `failed` — never propagating the
error past `crates/bisa-engine/src/decider.rs::judge`, whose caller always gets an answer to act on or
a reason to run its own rule instead.

---

## Tests

`tests/it/` is one binary: `support.rs` (a loopback stub, `ScriptedTransport`, fixed clocks and
entropy), then `main.rs`, `system_one.rs`, `prompted.rs`, `resilient.rs`, `factory.rs`; unit tests
beside `provider.rs`, `prompted.rs` and `resilient.rs`. The retry-timing tests run under tokio's
paused clock (`tokio::time`, `test-util`).

---

## What this crate refuses to do

- depend on any crate of ours beyond `bisa-core` and `bisa-connectors`;
- read a setting, a keystore or a clock of its own — everything arrives through `DecisionSettings`
  and `Ports`;
- redact a request — an HTTP provider is an outside service, and the engine redacts before a request
  reaches this crate;
- turn a broken contract into a guess: a response that does not hold to the shape is an error, never
  a partial answer;
- decide anything for a caller — `build` names a provider; whether an answer is acted on is
  [the engine's](../15-decision-making-agent.md#the-order-of-one-judgement).
