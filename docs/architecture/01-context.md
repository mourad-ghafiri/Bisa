# 01 — System context and containers

C4 levels 1 and 2. Level 3 (components) is covered per subsystem in documents 02–09; level 4 is the
code.

---

## Level 1 — System context

```mermaid
graph TB
    owner["<b>Owner</b><br/>a person with a keypair<br/>answers questions, signs gates"]
    peer["<b>Collaborator</b><br/>another Bisa node<br/>a member of this workspace"]

    subgraph sys["Bisa"]
        core["A workspace on one machine<br/>goals · projects · channels · agents"]
    end

    harness["<b>Coding harnesses</b><br/>Claude Code · Codex · pi<br/>oh-my-pi · OpenCode<br/>Copilot CLI · Grok Build · ACP"]
    codehost["<b>Git code host</b><br/>gh: pull requests, repos"]
    relays["<b>Nostr relays</b><br/>dumb transport, never authority"]
    ext["<b>The outside world</b><br/>the clock · hook calls · repositories<br/>platforms polled · checks"]
    mcpsrv["<b>MCP servers</b><br/>the owner's own tools"]

    owner -->|"CLI · desktop app"| core
    core -->|"questions, gates, activity"| owner

    core -->|"launches, steers, reads results"| harness
    harness -->|"MCP tools: results, questions, signals"| core

    core -->|"argv only, time-boxed"| codehost
    core <-->|"gift-wrapped GEP events"| relays
    relays <-->|"encrypted, wrapped to each person alone"| peer
    core <-->|"direct QUIC, relay-less"| peer
    ext -->|"events, written down as durable signals"| core
    core -->|"stdio / http"| mcpsrv

    style sys fill:#f6f8fa,stroke:#57606a
    style core fill:#ddf4ff,stroke:#0969da
```

**What the picture asserts, and what it deliberately does not:**

- **No server sits between the owner and their workspace.** There is no account, no tenancy, no
  control plane the vendor operates. A relay carries ciphertext and cannot read it.
- **Harnesses are orchestrated, not implemented.** Bisa never contains a model client. It
  launches the tools the owner already has and speaks their protocols at one boundary.
- **The outside world can only *make an event*.** An event never acts; it is written down as a
  durable signal, and the signal starts a run of the workflow that listens for it — under the same
  gates, budgets and concurrency caps as work the owner started
  ([03 — Workflows](03-workflows.md#events)).

---

## Level 2 — Containers

```mermaid
graph TB
    subgraph machine["One machine"]
        direction TB

        subgraph desktop["Desktop app (Tauri 2 + React 19)"]
            webview["Webview<br/>views · terminal · overlays"]
            shellproc["Rust shell<br/>PTY host · sidecar supervisor"]
        end

        cli["<b>bisa</b> (CLI)<br/>one-shot, embedded engine<br/>or a client of the node"]
        node["<b>bisa node</b> (daemon)<br/>axum control plane<br/>unix socket + optional TCP"]
        mcpp["<b>bisa mcp</b> (stdio)<br/>injected into every session"]

        engine["<b>Engine</b><br/>run effects · scheduler · gates<br/>events · waits · failover"]
        store["<b>Store</b><br/>truth files + rebuildable index"]

        fs[("<b>Workspace directory</b><br/>~/.bisa<br/>journals · snapshots · projects")]
        sqlite[("index.sqlite<br/><i>cache — deletable</i>")]
        keyring[("Key store<br/><i>identity/ files, 0600 · the OS keyring by choice</i>")]
    end

    webview <-->|"Tauri IPC"| shellproc
    webview -->|"HTTP + SSE over a unix socket"| node
    shellproc -->|"spawns as a sidecar"| node

    cli -->|"unix socket, when a daemon answers"| node
    cli -.->|"otherwise: embeds"| engine
    node --> engine
    mcpp -->|"JSONL over a unix socket"| engine

    engine --> store
    store --> fs
    store --> sqlite
    store --> keyring

    engine -->|"launch · steer · dispose"| harnesses["Coding harnesses"]
    engine -->|"argv, time-boxed"| git["git / gh"]
    node <-->|"sync"| net["Nostr + iroh"]

    style fs fill:#dafbe1,stroke:#1a7f37
    style sqlite fill:#fff8c5,stroke:#9a6700
    style keyring fill:#ffebe9,stroke:#cf222e
```

### One binary, three personalities

`bisa` is a multicall binary. Which personality runs is decided by argv, and all three share
the same engine and store code:

| Invocation | Role |
|---|---|
| `bisa …` | CLI. Talks to a running daemon over its unix socket when one answers; otherwise embeds the engine for a one-shot. |
| `bisa node` | Daemon. An axum control plane over a live engine, with an SSE event stream. |
| `bisa mcp` | The MCP server injected into every harness session, speaking to the engine over a unix socket. |

The desktop app ships the same binary as a sidecar and drives it over HTTP.

### What each container owns

| Container | Owns | Never does |
|---|---|---|
| **Webview** | rendering, navigation, local window state — where the person was and what every screen kept of itself, in its own storage | reach the filesystem; know a path; name a program; send what it remembers to the node |
| **Rust shell** | the PTY, the sidecar's lifetime, the login `PATH`, the window's size and place (a file of its own under the app's config folder) | domain logic |
| **Node** | the HTTP/SSE surface, the hook receiver, A2A | write to the store directly — every write goes through the engine |
| **Engine** | the run machine's effects, scheduling, gates, listening for events, waits, failover, and **every filesystem effect** | transport |
| **Store** | truth files, the rebuildable index, identity, and the typed API above them | create a directory for work, run `git`, launch a process |
| **Core** | the domain: types, the state machine, invariants | any I/O at all |

### Where the security boundaries actually are

Three, and they are not where a reader usually guesses:

1. **The key files / the keyring.** The account is a keypair. There is no reset link. Keys are
   files under `identity/` by default, written with `O_CREAT|O_EXCL` at mode `0600` — never created
   wide and narrowed afterwards; the OS keyring holds them only with `BISA_KEYSTORE=keyring`.
2. **The unix socket** at `<data-dir>/run/node.sock`, mode `0600`, **and the bearer token**
   (`run/token`, mode `0600`). Every route on both listeners wants the token; the socket's file
   permissions are a second, independent check. `--listen` exposes the same plane on TCP, where
   the token stands alone, and a non-loopback address is refused unless `--insecure-allow-remote`
   is passed. `POST /hooks/{host}/{step}` is the one public route with a secret of its own (the listener's, as a
   token or an HMAC-SHA256 over the body), and it answers only while this machine allows public
   hooks (`events.public_hooks`, off by default).
3. **`resolve_within`.** Any caller-supplied path is resolved by canonicalization, not by a string
   prefix test — because `..`, an absolute path and a symlink out of the tree are three ways to ask
   one question and only canonicalization answers all three.

The embedded terminal is deliberately outside all of this: it is a Tauri command in the desktop
shell, not a node route, because a PTY behind the control plane is a shell for anything that can
read the workspace's token.
