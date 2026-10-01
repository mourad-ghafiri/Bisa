# bisa-log

The platform's diagnostic log: one `tracing` subscriber per process, a JSON-lines file per process
family under the workspace's `logs/` folder, a flight recorder in memory, a crash report for every
abnormal end, a run marker for every run, and the words the `logging.*` settings speak. A leaf — it
depends on nothing of ours — because three processes install it: the `bisa` binary in every
personality, and the desktop shell, a separate cargo workspace that reaches it by path. The one
rule it keeps: **a process writes about itself, on this machine, and nothing here sends anything
anywhere.**

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the module list and the contract in prose: three layers, the file synchronous, a line carries ids, kinds, statuses and a typed error's sentence — never a prompt, a body, a token or an environment value; a report carries those lines, a backtrace and a child's stderr tail |
| `config.rs` | the words: `LogLevel` (`error` · `warn` · `info` · `debug` · `trace`, `WORDS` in the order the setting offers them, `as_str`, `FromStr`, `filter()` → `LevelFilter`), `LogRotation` (`hourly` · `daily`, the appender's `Rotation`), `LogConfig { enabled, level, rotation, keep_files }` (`Default` — **errors only**, daily, fourteen — is what the registry's defaults resolve to; `filter()` is `OFF` when the switch is off), `ConfigError` for a word the crate does not speak |
| `files.rs` | the folder: `Process { Node, Cli, Mcp, Desktop }` (`prefix()` — the folder's name and the file name's first segment, one family per process kind; `dir(root)`; `from_prefix`), `crashes_dir(root)`, `runs_dir(root)`, `SUFFIX` (`jsonl`) and `REPORT_SUFFIX` (`json`), `LogFile { name, bytes, modified_at }`, `Family { process, dir, files }`, `Listing { families, crashes }` (`bytes()`, `files()`), `list(root)` (every family in the crate's order with its files newest first — a family with no folder yet has none — and the reports; a root not yet made lists nothing), `is_log_name`, `is_crash_name` (`<process>.<stamp>.<pid>.json`, checked before a route joins a path), `is_marker_name` |
| `recorder.rs` | the flight recorder: a `Layer` under `RECORDER_FLOOR` (`debug`) keeping the last `RECORDER_CAPACITY` (256) events in a ring — `Recorded { at, level, target, message, fields }`, each field as its words, bounded — written nowhere until a report takes a snapshot; `take_unwritten` — what was said before any file was open, for the replay, and `forget_unwritten` — the backlog dropped when the switch is off; `Recording` and its `WrittenFlag`, set by the handle when a file opens and while the log is off, since nothing said then is owed to a later file |
| `crash.rs` | the report: `CrashReport { process, version, pid, at, kind, message, location, thread, backtrace, child, recent }`, `CrashKind { Panic, AbruptEnd, ChildExit }` (`WORDS`), `ChildExit { process, pid, code, signal, stderr }`; `write(root, report, at)` — `crashes/<process>.<stamp>.<pid>.json`, a temp file and a rename, then the oldest past `KEEP_CRASHES` (50) removed; `read(root, name)`, `latest(root)`; the backtrace cut at 64 KiB on a character boundary |
| `lifecycle.rs` | the run markers: `Marker { process, version, pid, started_at }`, `mark(root, marker)` — `runs/<process>.<pid>.json`; `sweep(root, process, now)` — every marker of the family whose pid is no longer alive (`alive(pid)`: `kill(pid, 0)`, `EPERM` counts as alive, elsewhere always alive) becomes one `error` line naming the report and one `AbruptEnd` report, then goes; this process's own and another family's are left alone; an unreadable marker goes with a `warn` |
| `stamp.rs` | time as the files spell it: `rfc3339(at)` on a line and in a report, `name_stamp(at)` — `20260911T102233Z` — in a report's name, so the names sort by time |
| `install.rs` | the subscriber and the handle: `install(process, version) -> Handle` (once per process — a `Registry` with the file slot, the recorder and the stderr layer under `EnvFilter::from_default_env()`, so `RUST_LOG` keeps working; the panic hook, installed whether or not the subscriber was this crate's), `current()` (the handle, for a site with no context to carry one), `build(process, version) -> (Handle, Layers)` (the file slot and the recorder as one boxed layer, for a test's thread-local subscriber), `Handle::attach(root, config)` (makes the family's folder, opens the rolling appender — prefix, suffix, `max_log_files(keep_files)` — swaps it into the slot, sets the level; **the first open** replays the recorder's unwritten entries at or above the level into the file, each marked `replayed`, sweeps the family's stale markers, writes this run's, then says *log started* with the version, the pid, the file family and the reports' folder; a root whose parent does not exist yet is remembered and nothing is written — its words wait for the first open; the switch off closes the file and settles the backlog — off is off, nothing said then reaches a later file), `Handle::apply(config)` (the settings moved), `Handle::goodbye(reason)` (*log stopped* with the reason and the uptime, the marker removed; idempotent), `Handle::report_crash(report)` (stamped with the process, the version, the pid, the moment and the recorder's entries, written under `crashes/`; refused with no root or the switch off, and by the write when the root's parent is missing — a report never makes a workspace), `Handle::writing()`, `Handle::root()`, `Handle::config()`, `Handle::recent()`, `panic_message(payload)` — the one downcast of a panic's payload, shared with the node's panic layer and the engine's task guards — `env_dir()` and `LOG_DIR_ENV` (`BISA_LOG_DIR` — the root a parent names to a child it spawns) |

### Two slots, not one

`reload::Handle::reload` refuses a `Filtered` layer (tokio-rs/tracing#1629), so the level does not
sit inside the file layer's slot: the appender lives in `reload::Layer<Option<fmt::Layer<…>>>` and
is *reloaded* when the folder or the rotation changes; the level lives in
`reload::Layer<LevelFilter>` as that slot's per-layer filter and is *modified*. Both change on the
next event, with no restart. `enabled = false` is `OFF` and an empty slot — no file is opened, no
marker is written, no report is taken.

### The folder

```text
logs/
  node/     node.<period>.jsonl           one folder per process family; retention prunes per folder
  cli/      cli.<period>.jsonl
  mcp/      mcp.<period>.jsonl
  desktop/  desktop.<period>.jsonl
  crashes/  <process>.<stamp>.<pid>.json  one report per abnormal end, fifty kept
  runs/     <process>.<pid>.json          a run's marker, gone with its goodbye
```

The store names `logs/` (`bisa-store`'s `paths.rs`); this crate names everything inside it. A
stranger anywhere in the folder is never listed and never removed.

### The file

`<root>/<process>/<process>.<period>.jsonl`: `node/node.2026-09-08.jsonl` for a daily file,
`node/node.2026-09-08-14.jsonl` for an hourly one, the period in UTC. One JSON object per line —
`timestamp`, `level`, `target`, `message` and the event's fields flattened beside them — written
**synchronously**: one `write` per event, nothing buffered, so a line is on disk before the event
that wrote it moves on — a panic (contained by the node's layer and the engine's guards; the `dist`
profile unwinds) is written before anything unwinds. At the default — errors only — the file costs
nothing; at `debug` it is one system call per event, under the cost of the presence fold that emits
most of them. No worker thread, no lossy channel. A replayed line carries `replayed: true` and its
fields as their words.

### The recorder

The last 256 events at `debug` and above, in memory, whatever the file's level: the context an
`error` line at the default level would otherwise lack. It is read twice — into every crash report,
and once into the first file a process opens, for what was said before there was a file (the shell's
words about a node that never started; a command's before the workspace opened). It is not a buffer
between an event and its line: a line is written as it is said, and the ring is a copy.

### A death

| What | Who writes | The report |
|---|---|---|
| a panic | the hook, before the unwind: the `error` line (`target: "panic"`, `location`, `thread`, `crash` — the report's name) and the report with the message, the location, the thread, a `Backtrace::force_capture()` and the recorder's entries | `<process>.<stamp>.<pid>.json`, kind `panic` |
| a run that ended without a goodbye — a signal, an abort, a stack overflow, an allocation failure, a power cut | the **next start** of the same family, from the marker it left: the `error` line *the previous … run ended without a goodbye* and the report with the marker's pid, version and start | kind `abrupt_end` |
| a child a process supervises — the desktop's node — exited on its own | the parent, through `report_crash`: the exit code or signal and the child's last stderr lines | kind `child_exit` |

The release profiles keep the symbol table (`strip = "debuginfo"`), so a backtrace in a shipped
binary names its frames.

### Retention

`keep_files` is `tracing-appender`'s `max_log_files`: when a new period's file opens, the oldest
files of **that family** past the count are removed — the families sit in folders of their own, so
`node/` never touches `desktop/`. A report past `KEEP_CRASHES` goes when a new one is written; a
stale marker goes with its report. These are the things the platform removes on its own
([08 — Persistence](../08-persistence.md)).

---

## Entry points

`install(process, version)` from `main`; `handle.attach(root, LogConfig::default())` as soon as the
root is known; `handle.apply(config)` whenever the settings resolve or move; `handle.goodbye(reason)`
where the process means to end — the CLI's `main` on both paths and its ctrl-c arms, the daemon
after its stop signal, the shell on `RunEvent::Exit`; `handle.report_crash(report)` for a death
that is not a panic. `list(root)`, `latest_crash(root)` and `read_crash(root, name)` for a panel and
`bisa logs`. `env_dir()` for a child personality that was told where to write.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| Every level and rotation word round-trips in the order the setting offers; the default is errors only, daily, fourteen; off filters everything | `config.rs` unit tests |
| A log name is a process, a period and the suffix; a crash name a process, a stamp, a pid and `.json`; a marker name a process, a pid and `.json` — a stranger is neither listed nor pruned; the folders hang off the root | `files.rs` unit tests; `tests/it/log.rs::the_listing_groups_the_families_newest_first_and_ignores_a_stranger` |
| A line lands at or above the level and not below; the level moves in place; the *log started* line names the process, the version, the floor and the reports' folder; the fields ride flattened beside the message; a family writes under its own folder and the others list nothing | `tests/it/log.rs::a_line_lands_at_or_above_the_level_and_a_change_of_level_lets_more_through` |
| Off opens no file, writes no marker and refuses a report; on again writes to the same folder; an hourly name carries the hour | `tests/it/log.rs::off_writes_nothing_and_on_again_writes_to_the_same_folder` |
| A root whose parent is missing is remembered and nothing is made — `bisa init` leaves no `logs/` before the workspace — and a report is refused by the write, not for want of a root | `tests/it/log.rs::a_folder_whose_parent_is_missing_is_remembered_and_nothing_is_made` |
| The ring keeps the last entries under its floor, counts the unwritten and hands them over once; a message and a field are bounded, an error is its sentence | `recorder.rs` unit tests |
| A report is written, read back by name, the newest found, the oldest pruned past the count, a stranger beside them untouched; a backtrace is cut on a character boundary | `crash.rs` unit tests |
| This process is alive and a pid nobody has is not; a stale marker of the family becomes a report and goes, this process's and another family's stay, an unreadable one goes with a warning; a sweep finds nothing twice | `lifecycle.rs` unit tests |
| The first open replays what was said before at or above the level, marked and above what follows; sweeps the family's stale marker into an error line naming the report and the report; writes this run's marker; a second attach does none of it again; the goodbye says *log stopped* once with its reason and takes the marker | `tests/it/log.rs::the_first_open_sweeps_a_stale_marker_writes_this_runs_and_replays_what_was_said_before` |
| A crash report through the handle carries the process, the version, the pid, the moment, the child's exit and last words and the recorder's entries — the debug ones included, the trace ones not — and is refused with no root | `tests/it/log.rs::a_crash_report_carries_the_recorders_entries_and_the_childs_last_words` |
| The panic hook writes a report with the message, the location, the thread, a backtrace and the recorder's entries, and an `error` line naming the report | `tests/it/log.rs::the_panic_hook_writes_a_report_with_a_backtrace_and_a_line_naming_it` |
| The two spellings of a moment agree | `stamp.rs` unit test |
| The registry's `logging.level` and `logging.rotation` choices are this crate's words | `crates/bisa-engine/tests/it/logging.rs` |

---

## Errors

`ConfigError` — a word the crate does not speak; `AttachError` — the folder could not be made
(`Io`), the appender could not open (`Appender`), the slot refused the swap (`Reload`);
`ReportError` — no root attached (`NoFolder`), the switch off (`Off`), the write (`Io`). A caller
that cannot attach says so on the log it has — stderr, and the recorder — and runs on; the hook says
so on stderr and never panics itself.

---

## Extension points

| To add… | Touch, in order | The gate that catches a miss |
|---|---|---|
| a level or a rotation word | the variant, `ALL` and `WORDS` in `config.rs` → the `Choice` in `crates/bisa-core/src/settings.rs` → `just gen-settings-docs` | `crates/bisa-engine/tests/it/logging.rs`; `desktop/src/logModel.test.mjs` |
| a process kind | the `Process` variant and its prefix in `files.rs` → `process_of` in `crates/bisa-cli/src/main.rs` → `FAMILIES` in `desktop/src/views/_settings/loggingModel.mjs` | `loggingModel.test.mjs` reads the prefixes |
| a crash kind | the `CrashKind` variant and its word in `crash.rs` → `CRASH_KINDS` in `desktop/src/views/_settings/loggingModel.mjs` | `loggingModel.test.mjs` reads the words |
| a field on every line | never here — a line is what its site says; a process-wide fact rides the *log started* line | — |
| a field on every report | `CrashReport` in `crash.rs` → `CrashReportView` in `crates/bisa-node/src/dto.rs` → `just gen-types` | `crates/bisa-node/tests/it/logs.rs` |

---

## Tests

Unit tests beside `config.rs`, `files.rs`, `recorder.rs`, `crash.rs`, `lifecycle.rs` and
`stamp.rs`; `tests/it/log.rs` on a `tempfile` folder under a thread-local subscriber (`build` +
`set_default`), so no test writes outside its tempdir — but for the one test of the process-wide
hook, which installs the global subscriber (a thread-local one overrides it everywhere else) and
still attaches a tempdir.

---

## What this crate refuses to do

- depend on anything of ours, or read a setting — an engine or a shell resolves the settings and
  calls `apply`;
- know where the workspace is — the root is handed in (`paths.rs` names it; `bisa paths` says
  it to the shell);
- buffer, batch or drop a line, or send one anywhere — the recorder is a copy for a report and a
  first replay, never the path a line takes to its file;
- format a payload: a site hands over words, and the crate writes them.
