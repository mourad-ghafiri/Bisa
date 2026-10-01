# bisa-ssh

`ssh`, `ssh-keygen` and `ssh-add` for **git hosts** as typed, time-boxed, argv-only subprocess
calls — a leaf crate like `bisa-vcs`, with no dependency on anything of ours. It answers the
questions a person asks before a push: which public keys are on this machine and what their
fingerprints are, which of them ssh-agent holds, which identity `ssh` would offer a host or an alias
(`ssh -G`, offline), what a git host says when greeted (`ssh -T`, one handshake, nothing written),
and it makes a new ed25519 key pair or loads one into ssh-agent when asked. The one rule it exists
to keep: **a private key is never opened** — by this crate, by a test, by anything above it. The
SSH directory is a parameter (`~/.ssh` in production, a `tempfile` directory in every test), the
process launcher is a port with a fake, and no test of any crate spawns `ssh`, `ssh-keygen` or
`ssh-add`.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the module list and re-exports; `SshError { Unavailable, Refused, Timeout, Failed }`; `KNOWN_GIT_HOSTS` (`github.com`, `gitlab.com`, `bitbucket.org`, `codeberg.org`, `ssh.dev.azure.com`); the crate's discipline in prose; the guard test `processes_are_spawned_in_exec_alone_and_only_public_files_are_read`, which greps the shipped source for `std::process` outside `exec.rs` and for any file read whose name does not end in `.pub` or is not `config` |
| `exec.rs` | the one process launcher: `Program::{Ssh, SshKeygen, SshAdd}`, `Timeouts` (15 s for a local call, 30 s for a handshake), `Output { code, stdout, stderr }`, the `SshRunner: Send + Sync + Debug` port and `Cli`, its production implementation — argv only, never a shell; stdin null; `BatchMode=yes` on every `ssh`; `SSH_ASKPASS=""`, `SSH_ASKPASS_REQUIRE=never` and `DISPLAY` unset, so nothing can prompt; time-boxed and **terminated** on expiry; each pipe drained on its own thread, and a read that fails is `Failed`, never a truncated answer read as whole |
| `fake.rs` | `FakeSsh` — the scripted runner every test hands in: `answer(program, words, code, stdout, stderr)` matches an argv by the words it must contain, `writing(path, text)` lays a file down the way `ssh-keygen` would, `calls()` returns every `Call` recorded (`Call::line()` is its argv as one line for an assertion) |
| `config.rs` | `parse_ssh_config(text)` → `HostBlock { patterns, options }` (a `Host` block and the options under it; `Match` blocks skipped), `git_host_blocks(blocks, hosts)` (the blocks that name a git host, an alias whose `HostName` is one, or apply to every host), `host_block_text(alias, hostname, user, key)` — the text *Copy a Host block* offers, which a person pastes into `~/.ssh/config` themselves; the file is **read, never written** |
| `keys.rs` | `PublicKey { name, path, algorithm, comment, fingerprint, public_line }`, `parse_public_key(line)` (total: what is not a public key line is `None` — the first word one OpenSSH writes, `ssh-*`, `ecdsa-*` and the hardware-backed `sk-ssh-*@…` and `sk-ecdsa-*@…`, and a blob that opens with that same word, so a line whose halves disagree is a damaged file and not a key), `fingerprint_sha256(blob)` — OpenSSH's `SHA256:` form computed from the key blob with `sha2` + `base64`, no subprocess |
| `agent.rs` | `AgentState { available, keys: Vec<AgentKey> }`, `parse_ssh_add_list(code, stdout)` — exit 0 lists keys, 1 is *no identities*, 2 is *no agent* |
| `resolve.rs` | `Resolved { hostname, user, port, identity_files, identities_only, identity_agent }`, `parse_ssh_g(text, home)` — `ssh -G`'s answer with `~` expanded against the home handed in, so an alias like `github-acme` reads as the host it stands for |
| `greeting.rs` | `HostGreeting::{Authenticated { login }, Refused { reason }, HostKeyUnknown { reason }, Unreachable { reason } }`, `parse_greeting(host, code, stderr)` — GitHub's *Hi X! You've successfully authenticated* on exit **1**, GitLab's *Welcome to GitLab, @X!* on 0, Bitbucket's sentence, *Permission denied* → `Refused`, *Host key verification failed* → `HostKeyUnknown`, *Could not resolve* or a timeout → `Unreachable`. The crate's one prose boundary, like `bisa-vcs::classify` |
| `ssh.rs` | `Ssh::new(runner, dir, home)` and the verbs: `keys()` (every `*.pub` under the directory, parsed), `agent()`, `config()`, `overview() -> SshOverview { dir, keys: Vec<KeyRow>, agent, hosts, known_git_hosts }` (`KeyRow` is a `PublicKey` plus `loaded`, joined on the fingerprint), `generate(NewKey { name, comment })` (the directory created 0700 when absent; a taken name or a comment starting with `-` refused **before any spawn**; `ssh-keygen -q -t ed25519 -f <dir>/<name> -C <comment> -N ""` — no passphrase from the platform, because one on argv is visible to every process; the person adds one with `ssh-keygen -p` in a terminal), `agent_add(name)` (`ssh-add`, `--apple-use-keychain` on macOS; a missing key refused before spawn; a key that wants a passphrase cannot be loaded without a prompt and the refusal names the terminal command), `resolve(host, identity)` (`ssh -G`, with `-i <key> -o IdentitiesOnly=yes` when a profile names a key), `test(host, user, identity)` (`ssh -T user@host`, the same pair, read by `parse_greeting`) |

---

## Entry points

`Ssh::new(Arc<dyn SshRunner>, dir, home)`. Production hands in `Cli::default()`, `~/.ssh` and the
home directory (`bisa_engine::ssh::default_ssh()`); a test hands in a `FakeSsh` and a
`tempfile` directory. The engine holds one `Ssh` in `EngineConfig.ssh` — `None` by default, so a
fixture that says nothing about SSH spawns nothing and every SSH route answers **503**.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| Processes are spawned in `exec.rs` alone, and the only files the crate reads end in `.pub` or are named `config` — a private key is never opened | `lib.rs::processes_are_spawned_in_exec_alone_and_only_public_files_are_read` |
| A public key line reads into its parts and its fingerprint is OpenSSH's `SHA256:` form; what is not a public key line is `None` | `keys.rs::a_public_line_reads_into_its_parts_and_the_fingerprint_is_openssh_shaped`, `what_is_not_a_public_key_line_is_none`, `every_openssh_key_type_is_read_and_a_line_whose_halves_disagree_is_not` |
| `ssh-add -l`'s three exit codes read as keys, none and no agent | `agent.rs::the_three_exit_codes_read_as_keys_none_and_no_agent` |
| `Host` blocks are read with their options and `Match` blocks skipped; the git hosts' blocks are the ones that name a git host or apply to all; a Host block's text is what a person pastes | `config.rs::host_blocks_are_read_with_their_options_and_match_blocks_are_skipped`, `the_git_hosts_blocks_are_the_ones_that_name_a_git_host_or_apply_to_all`, `a_host_block_text_is_what_a_person_pastes` |
| An alias resolves to its hostname and the identity files expand `~`; an unconfigured host reads as itself with the default identities | `resolve.rs::an_alias_resolves_to_its_hostname_and_the_identity_files_expand_home`, `an_unconfigured_host_reads_as_itself_with_the_default_identities` |
| Each host's greeting names the login and its exit code is not a failure; refusals, unknown host keys and unreachable hosts are told apart | `greeting.rs::each_hosts_greeting_names_the_login_and_the_exit_code_is_not_a_failure`, `refusals_unknown_host_keys_and_unreachable_hosts_are_told_apart` |
| Keys are listed from their `.pub` files and marked loaded from ssh-agent's answer | `ssh.rs::keys_are_listed_from_their_pub_files_and_marked_loaded_from_the_agents_answer` |
| `generate` refuses a taken name and an option-shaped comment before any spawn, and reads the new `.pub` file back | `ssh.rs::generate_refuses_a_taken_name_and_a_bad_comment_before_any_spawn_and_reads_the_new_pub_file` |
| `agent_add` refuses a missing key before any spawn and names the terminal command when a passphrase is wanted | `ssh.rs::agent_add_refuses_a_missing_key_and_names_the_terminal_command_when_a_passphrase_is_wanted` |
| `resolve` and `test` carry the profile's key as `-i … -o IdentitiesOnly=yes`, every `ssh` argv carries `BatchMode=yes`, and no call goes through a shell | `ssh.rs::resolve_and_test_carry_the_profiles_key_and_batch_mode_and_never_a_shell` |
| The crate depends on nothing of ours and only the engine depends on it | `crates/bisa-core/tests/it/layering.rs::allowed_edges` |

---

## Errors

`SshError`: `Unavailable(what)` — the program is not on `PATH` or ssh-agent is not running;
`Refused(sentence)` — the crate refused before spawning (a taken key name, an option-shaped
argument, a missing key) or the program refused with a reason; `Timeout { what, secs }`; `Failed {
what, code, stderr }`. The engine wraps it as `EngineError::Ssh` and adds `SshUnavailable` for a
node started without SSH; the node renders `Refused` as **400**, `Unavailable` and
`SshUnavailable` as **503**, `Timeout` as **504** and `Failed` as **502**, each with the sentence.

---

## Extension points

| To add… | Touch, in order | The gate that catches a miss |
|---|---|---|
| a git host's greeting | its sentence in `greeting.rs::parse_greeting` and a fixture in its tests; the host in `KNOWN_GIT_HOSTS` | `greeting.rs` tests |
| an SSH verb | a function on `Ssh` in `ssh.rs` that validates before it spawns, over `SshRunner` → its engine wrapper in `crates/bisa-engine/src/ssh.rs` → its route in `crates/bisa-node/src/ssh.rs` → the CLI verb | `lib.rs`'s guard (a spawn outside `exec.rs`, a read of a private file); `crates/bisa-node/tests/it/routes.rs` |

---

## Tests

Unit tests beside each module, on fixtures — banner text, `ssh -G` output, `ssh-add -l` output, a
synthetic public key line whose blob is built in the test. `ssh.rs`'s tests run `Ssh` over a
`FakeSsh` on a `tempfile` directory; `FakeSsh::writing` stands in for `ssh-keygen` laying a `.pub`
file down. No test reads `~/.ssh`, asks ssh-agent, or connects anywhere.

---

## What this crate refuses to do

- depend on anything of ours;
- open, read, copy or print a private key — `.pub` files and `config` are the only files it reads;
- take a passphrase, on argv or otherwise;
- write `~/.ssh/config` or `known_hosts` — a Host block is offered as text to paste, an unknown
  host key is reported as such;
- prompt, run through a shell, or spawn outside `exec.rs`;
- touch a host that is not a git host — server keys and `scp` are not its business.
