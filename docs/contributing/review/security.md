# Security review

For any change that touches a guard, the redactor, the classifier, consent, keys, tokens, secrets,
the node's doors, what a harness may run, what leaves the machine, a dependency, or input that comes
from outside the machine. The model is [11 — Security](../../architecture/11-security.md) and the trust
boundary [IDE 01](../../architecture/ide/01-trust-boundary.md); a vulnerability is reported privately
([SECURITY.md](../../../SECURITY.md)), never in a pull request.

## Secrets and keys
- [ ] A key, token or secret never reaches a log, a message, a journal, an error body, a prompt, a
  commit, a screenshot or a response — the log contract holds (never a prompt, a body, a file's text,
  a token or an environment value).
- [ ] Secrets are written once and never read back to a person; they live where the store keeps them
  (owner-only files, or the keyring when chosen).
- [ ] A value the redactor should recognise is recognised: a new kind of credential gets its rule.
- [ ] The control-plane token never enters a child's environment.

## Doors and inputs
- [ ] A new route is behind the bearer token, or its exemption is argued and added to the one list
  that names exemptions.
- [ ] Every input from outside the machine — a public hook, a connector's item, a page an agent reads,
  a message from another node — passes the content screen before an agent sees it.
- [ ] Every path is resolved inside its root (`resolve_within`), and the change cannot reach a truth
  file, another workspace, or a folder Bisa did not create.
- [ ] A body or file a person writes refuses keys it does not know, by name.
- [ ] Nothing new listens beyond loopback; nothing new runs a program the person did not choose.

## What agents may do
- [ ] A new tool or command an agent can call is judged by the Tool & Commands Guard, at the right
  tier, before it runs — or the reason it cannot be is stated, with the harness's reach
  ([Harnesses](../areas/harnesses.md)).
- [ ] Nothing gives an agent the consented git tier, a recovery-ref deletion, or the person's keys.
- [ ] A gate is never signed by a judgement; a publish passes the publish gate.

## What leaves the machine
- [ ] A new outbound connection is to a service the person chose or switched on, is listed where
  [the security page](../../architecture/11-security.md) lists what leaves the machine, and has a way
  to turn it off.
- [ ] Data that crosses to other nodes is sealed for each recipient; nothing travels that the
  collaboration rules keep local.

## Dependencies and supply chain
- [ ] A new or upgraded dependency is maintained, comes from crates.io or npm (no git or path source
  outside the repository), and has no open advisory.
- [ ] A dependency that runs code at build time (a build script, a proc macro, an install script) is
  justified.

## The verdict
- [ ] The reviewer states, on the pull request, what the change exposes and why it is acceptable — or
  requests the change that closes it.
