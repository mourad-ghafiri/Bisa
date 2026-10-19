//! The code host from the command line, with a `gh` of the journey's own:
//! a shell script on the journey's path that answers what the platform asks
//! of the GitHub CLI and reaches nothing. The health, the sign-in plan, the
//! accounts and a check through the CLI; stored accounts, the default and
//! git's helper read as before; a profile with an alias, a key and an
//! account listed beside a person's own include; the pull request verbs —
//! opened, read, its checks, reviewed, merged — on a project whose `origin`
//! reads as a repository on a GitHub of its own while every push lands in a
//! bare repository on this disk; the same verbs asked first through the
//! gate and decided with `--yes`; and every verb again with no node running.
//!
//! The remote's host name resolves nowhere, so the one probe that would
//! reach it (`git ls-remote`) fails in words and nothing leaves the machine.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Output;

const COMMITTER: &str = "A Journey <journey@example.test>";

/// The `gh` of the journey: what each verb the platform runs answers. The
/// pull request's state lives in a file beside the script — opened again by
/// every create, merged by a merge — so a read after a merge says merged.
const GH: &str = r#"#!/bin/sh
# GH_JOURNEY_MODE bends one answer: unsigned (no version, signed in to
# nothing), refusing (the host rejects the credential), private (the
# repository is not found), nochecks (no check runs, as JSON), nochecks-real
# (no check runs as the real gh says it: exit 1, nothing on stdout), failing
# (one check failed), stubborn (a merge that does not land), merged-elsewhere
# (the pull request merged on the web, whatever this machine did).
here=$(dirname "$0")
state="$here/gh-state"
mode="${GH_JOURNEY_MODE:-}"
pr() {
  s=OPEN; mc=null; at=null
  if [ "$mode" = merged-elsewhere ] || { [ -f "$state" ] && [ "$(cat "$state")" = MERGED ]; }; then s=MERGED; mc='{"oid":"deadbeef"}'; at='"2026-01-01T00:00:00Z"'; fi
  printf '{"number":7,"url":"https://github.example.test/acme/web/pull/7","title":"Add x","state":"%s","isDraft":false,"mergeable":"MERGEABLE","headRefName":"feature/x","headRefOid":"abc","baseRefName":"main","author":{"login":"octocat"},"mergeCommit":%s,"mergedAt":%s,"mergeStateStatus":"CLEAN"}\n' "$s" "$mc" "$at"
}
case "$1 $2" in
  "--version ") [ "$mode" = unsigned ] || echo "gh version 2.63.2 (2024-12-05)";;
  "auth status")
    if [ "$mode" = unsigned ]; then echo "You are not logged into any GitHub hosts. To log in, run: gh auth login" >&2; exit 1; fi
    printf '%s\n  ✓ Logged in to %s account octocat (keyring)\n  - Active account: true\n  - Git operations protocol: https\n' "$4" "$4";;
  "auth token") [ "$mode" = unsigned ] && exit 1; echo "cli-word-for-the-journey";;
  "api user")
    if [ "$mode" = refusing ]; then echo "HTTP 401: Bad credentials (https://api.github.com/user)" >&2; exit 1; fi
    echo '{"login":"octocat","id":583231,"name":"The Octocat","email":null}';;
  "api user/orgs") echo '[{"login":"acme"}]';;
  "api repos/acme/web")
    if [ "$mode" = private ]; then echo "HTTP 404: Not Found (https://api.github.com/repos/acme/web)" >&2; exit 1; fi
    echo '{"permissions":{"push":true}}';;
  "api graphql") echo '{"data":{"repository":{"pullRequest":{"reviews":{"nodes":[]},"reviewThreads":{"nodes":[]}}}}}';;
  "api --method") echo '{}';;
  "pr create") echo OPEN > "$state"; echo "https://github.example.test/acme/web/pull/7";;
  "pr view") pr;;
  "pr list") echo '[]';;
  "pr checks")
    case "$mode" in
      nochecks) echo '[]';;
      nochecks-real) echo "no checks reported on the 'feature/x' branch" >&2; exit 1;;
      failing) echo '[{"name":"ci","state":"FAILURE","bucket":"fail","link":"","description":""}]'; exit 1;;
      *) echo '[{"name":"ci","state":"SUCCESS","bucket":"pass","link":"https://ci.example.test/1","description":"ok"}]';;
    esac;;
  "pr review") ;;
  "pr merge") [ "$mode" = stubborn ] || echo MERGED > "$state";;
  *) echo "unknown command \"$*\" for \"gh\"" >&2; exit 1;;
esac
"#;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A project that is a repository with one commit, publishing as `publish`
/// says, whose `origin` reads as `acme/web` on a GitHub of its own — the
/// kind named by `codehost.kind`, the host one that resolves nowhere —
/// while every push lands in a bare repository on this disk. Its id, its
/// tree and the bare repository.
fn an_enterprise_project(ws: &Sealed, slug: &str, publish: &str) -> (String, PathBuf, PathBuf) {
    let made = ws.json(&[
        "project",
        "new",
        slug,
        "--publish",
        publish,
        "--committer",
        COMMITTER,
    ]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let tree = PathBuf::from(made["path"].as_str().expect("its tree"));
    std::fs::write(tree.join("plan.md"), "measure twice\n").expect("the plan");
    ws.ok(&[
        "project", "commit", &project, "-m", "the plan", "--path", "plan.md",
    ]);
    let branch = git(&tree, &["symbolic-ref", "--short", "HEAD"]);
    let origins = ws.file("origins", "").parent().unwrap().to_path_buf();
    let bare = origins.join(format!("{slug}-origin.git"));
    std::fs::create_dir_all(&bare).unwrap();
    git(
        &bare,
        &["init", "--bare", "--quiet", "--initial-branch", &branch],
    );
    git(
        &tree,
        &[
            "remote",
            "add",
            "origin",
            "https://github.example.test/acme/web.git",
        ],
    );
    git(
        &tree,
        &["config", "remote.origin.pushurl", bare.to_str().unwrap()],
    );
    git(&tree, &["config", "codehost.kind", "github"]);
    git(
        &tree,
        &["push", "--quiet", "origin", &format!("HEAD:{branch}")],
    );
    (project, tree, bare)
}

/// A workstream on a new branch of `project` holding one commit.
fn a_branch(ws: &Sealed, project: &str, name: &str, goal: Option<&str>) -> String {
    let branch = format!("new:feature/{name}");
    let mut open = vec!["workstream", "open", project, "--from", branch.as_str()];
    if let Some(goal) = goal {
        open.extend(["--goal", goal]);
    }
    let opened = ws.json(&open);
    let workstream = opened["workstream"]["id"]
        .as_str()
        .expect("the workstream")
        .to_string();
    let checkout = PathBuf::from(opened["path"].as_str().expect("its checkout"));
    std::fs::write(checkout.join(format!("{name}.md")), "one more board\n").expect("a file");
    ws.ok(&["workstream", "commit", &workstream, "-m", name]);
    workstream
}

/// The verbs on an open pull request: read, its checks, reviewed, merged.
fn around_a_pull_request(ws: &Sealed, workstream: &str) {
    let viewed = ws.json(&["workstream", "pr-view", workstream]);
    assert_eq!(viewed["pr"]["number"], 7, "{viewed}");
    assert_eq!(viewed["pr"]["state"], "open", "{viewed}");
    let checks = ws.json(&["workstream", "pr-checks", workstream]);
    assert_eq!(checks["checks"][0]["name"], "ci", "{checks}");
    assert_eq!(checks["checks"][0]["conclusion"], "success", "{checks}");
    let own = ws.refused(&[
        "workstream",
        "pr-review",
        workstream,
        "--event",
        "approve",
        "--body",
        "fine",
    ]);
    assert!(
        own.contains("own") || own.contains("octocat"),
        "the account that opened it cannot approve it: {own}"
    );
    let shown = ws.ok(&["workstream", "show", workstream]);
    assert!(
        said(&shown).contains("pr #7"),
        "the state word: {}",
        said(&shown)
    );
    let reviewed = ws.json(&[
        "workstream",
        "pr-review",
        workstream,
        "--event",
        "comment",
        "--body",
        "one nit",
    ]);
    assert_eq!(reviewed["reviewed"], true, "{reviewed}");
    ws.refused(&["workstream", "pr-review", workstream, "--event", "nope"]);
    let own = ws.refused(&[
        "workstream",
        "pr-review",
        workstream,
        "--event",
        "request-changes",
        "--body",
        "no",
    ]);
    assert!(own.contains("own") || own.contains("octocat"), "{own}");
    ws.refused(&["workstream", "pr-merge", workstream, "--strategy", "nope"]);
    let merged = ws.json(&["workstream", "pr-merge", workstream]);
    assert_eq!(merged["merged"], true, "{merged}");
    assert_eq!(merged["sha"], "deadbeef", "{merged}");
    let viewed = ws.json(&["workstream", "pr-view", workstream]);
    assert_eq!(viewed["pr"]["state"], "merged", "read fresh: {viewed}");
    assert_eq!(
        ws.json(&["workstream", "show", workstream])["workstream"]["state"]["state"],
        "merged"
    );
}

/// The token store's two files for one login — the crate's own layout,
/// written here so the rows a stored account earns can be read without a
/// host to verify a token with.
fn a_stored_account(ws: &Sealed, login: &str) {
    let dir = ws.data().join("identity").join("codehost").join("github");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{login}.token")), "a-word-for-the-journey").unwrap();
    std::fs::write(dir.join("accounts"), format!("{login}\n")).unwrap();
}

#[test]
fn the_code_host_is_reached_through_a_gh_of_the_journeys_own() {
    let mut ws = Sealed::bare();
    ws.install_program("gh", GH);
    let helper = ws.install_program(
        "helper.sh",
        "#!/bin/sh\nprintf 'username=%s\\npassword=%s\\n' someone a-word-the-helper-holds\n",
    );
    // The machine's global git config, for the node and every verb: a file
    // of the journey's own holding a person's include and git's helper.
    let global = ws.file(
        "gitconfig",
        &format!(
            "[user]\n\tname = Ada Journey\n\temail = ada@journey.test\n[includeIf \"gitdir:/somewhere/\"]\n\tpath = /somewhere/own.gitconfig\n[credential]\n\thelper = !sh {0}\n\thelper = !sh {0}\n",
            helper.display()
        ),
    );
    ws.set_env("GIT_CONFIG_GLOBAL", &global.to_string_lossy());
    ws.start();

    // --- the health, the sign-in plan, the accounts, a check through the CLI ---
    let health = ws.json(&["git", "health"]);
    assert_eq!(health["cli"]["installed"], true, "{health}");
    assert_eq!(health["cli"]["version"], "2.63.2", "{health}");
    assert_eq!(health["cli_login"], "octocat", "{health}");
    assert_eq!(
        health["helpers"],
        json!(["sh", "sh"]),
        "two helpers: {health}"
    );
    assert_eq!(health["helper_username"], "someone", "{health}");
    assert_eq!(
        health["resolves"],
        json!({"login": "octocat", "source": "cli"}),
        "the CLI's account answers before git's helper: {health}"
    );
    let bitbucket = ws.json(&["git", "health", "--host", "bitbucket"]);
    assert!(
        bitbucket["cli"].is_null(),
        "Bitbucket has no CLI: {bitbucket}"
    );
    assert_eq!(
        bitbucket["resolves"]["source"], "git",
        "with nothing else, git's helper answers: {bitbucket}"
    );
    let plan = ws.json(&["git", "login"]);
    assert_eq!(plan["kind"], "cli", "{plan}");
    assert_eq!(plan["program"], "gh", "{plan}");
    let accounts = ws.json(&["git", "account", "list"]);
    assert_eq!(accounts["cli_login"], "octocat", "{accounts}");
    assert_eq!(accounts["helpers"], json!(["sh", "sh"]), "{accounts}");
    let checked = ws.json(&["git", "account", "check", "octocat"]);
    assert_eq!(checked["state"], "connected", "{checked}");
    assert_eq!(checked["organizations"], json!(["acme"]), "{checked}");

    // --- a stored account: listed, made the default, forgotten ---------------
    a_stored_account(&ws, "ada");
    let listed = ws.json(&["git", "account", "list"]);
    assert_eq!(listed["accounts"][0]["login"], "ada", "{listed}");
    let health = ws.json(&["git", "health"]);
    assert_eq!(health["accounts"][0]["source"], "file", "{health}");
    assert_eq!(
        health["resolves"]["login"], "ada",
        "a stored account outranks the CLI's: {health}"
    );
    assert_eq!(
        ws.json(&["git", "account", "default", "ada"])["default"],
        "ada"
    );
    let health = ws.json(&["git", "health"]);
    assert_eq!(health["default"], "ada", "{health}");
    let rows = ws.ok(&["git", "account", "list"]);
    assert!(said(&rows).contains("(default)"), "{}", said(&rows));
    ws.json(&["git", "account", "default", "--clear"]);
    ws.ok(&["git", "account", "rm", "ada"]);
    assert_eq!(ws.json(&["git", "account", "list"])["accounts"], json!([]));

    // --- a profile with an alias, a key and an account, beside a person's own include ---
    let saved = ws.json(&[
        "git",
        "profile",
        "set",
        "acme",
        "--label",
        "Acme",
        "--owner",
        "acme",
        "--alias",
        "github-acme",
        "--name",
        "Ada Journey",
        "--email",
        "ada@acme.test",
        "--ssh-key",
        "/keys/id_acme",
        "--account",
        "ada",
    ]);
    assert_eq!(saved["aliases"], json!(["github-acme"]), "{saved}");
    let listed = ws.bisa(&["git", "profile", "list"]);
    assert!(listed.status.success(), "{}", said(&listed));
    let words = said(&listed);
    assert!(
        words.contains("github-acme") && words.contains("/keys/id_acme") && words.contains("ada"),
        "the alias, the key and the account on the row: {words}"
    );
    assert!(
        words.contains("gitdir:/somewhere/"),
        "the person's own include, shown as not the platform's: {words}"
    );
    let as_json = ws.json(&["git", "profile", "list"]);
    assert_eq!(
        as_json["foreign_includes"][0]["condition"],
        "gitdir:/somewhere/"
    );
    ws.json(&["git", "profile", "rm", "acme"]);

    // --- a plain folder adopted, made a repository under the global identity ---
    let origins = ws.file("origins", "").parent().unwrap().to_path_buf();
    let plain = origins.join("plain-src");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::write(plain.join("note.txt"), "plain\n").unwrap();
    let adopted = ws.json(&[
        "project",
        "adopt",
        plain.to_str().unwrap(),
        "--slug",
        "plain",
    ]);
    let plain_id = adopted["project"]["id"].as_str().unwrap().to_string();
    let made = ws.ok(&["project", "git-init", &plain_id]);
    assert!(
        !said(&made).contains("commits as"),
        "nobody to name: the global identity commits: {}",
        said(&made)
    );
    let probed = ws.bisa(&["--json", "git", "connection", &plain_id, "--check"]);
    assert!(probed.status.success(), "{}", said(&probed));
    let value: Value = serde_json::from_slice(&probed.stdout).expect("json");
    assert!(
        value["check"]["ls_remote"].is_null(),
        "no remote, no probe: {value}"
    );

    // --- the pull request verbs, by itself ------------------------------------
    let (web, _, bare) = an_enterprise_project(&ws, "web", "auto");
    let primary = ws.json(&["workstream", "show", &web]);
    assert_eq!(primary["workstream"]["id"], web.as_str(), "{primary}");
    let connection = ws.json(&["git", "connection", &web]);
    assert_eq!(connection["code_host"], "github", "{connection}");
    assert_eq!(connection["remote"]["protocol"], "https", "{connection}");
    assert_eq!(connection["account"]["login"], "octocat", "{connection}");
    let x = a_branch(&ws, &web, "x", None);
    ws.refused(&["workstream", "pr", &x, "--title", "   "]);
    let opened = ws.json(&[
        "workstream",
        "pr",
        &x,
        "--title",
        "Add x",
        "--body",
        "one more",
    ]);
    assert_eq!(opened["opened"], true, "{opened}");
    assert_eq!(opened["pr"]["number"], 7, "{opened}");
    assert!(
        git(
            &bare,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads"]
        )
        .contains("feature/x"),
        "the branch went to the bare repository through pushurl"
    );
    around_a_pull_request(&ws, &x);

    // The probes: the host through the CLI, the repository's access, and a
    // remote whose name resolves nowhere, said in words.
    let probed = ws.bisa(&["--json", "git", "connection", &x, "--check"]);
    assert!(probed.status.success(), "{}", said(&probed));
    let value: Value = serde_json::from_slice(&probed.stdout).expect("json");
    assert_eq!(value["check"]["code_host"]["state"], "connected", "{value}");
    assert_eq!(value["check"]["access"]["push"], true, "{value}");
    assert_eq!(value["check"]["ls_remote"]["ok"], false, "{value}");
    assert!(value["check"]["ls_remote"]["detail"].is_string(), "{value}");

    // --- asked first, decided with the verb --------------------------------------
    let goal = ws.json(&["new", "ship the shelf", "--mode", "manual"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let (shelf, _, _) = an_enterprise_project(&ws, "shelf", "gated");
    ws.ok(&["project", "attach", &shelf, &goal]);
    let g = a_branch(&ws, &shelf, "g", Some(&goal));
    let opened = ws.json(&["workstream", "pr", &g, "--title", "Add g", "--yes"]);
    assert_eq!(opened["opened"], true, "{opened}");
    assert_eq!(
        opened["pr"]["number"], 7,
        "read off the record once the gate was decided: {opened}"
    );
    let merged = ws.json(&[
        "workstream",
        "pr-merge",
        &g,
        "--yes",
        "--keep-remote-branch",
    ]);
    assert_eq!(merged["merged"], true, "{merged}");
    assert_eq!(
        ws.json(&["workstream", "show", &g])["workstream"]["state"]["state"],
        "merged"
    );

    // --- every verb again with no node running --------------------------------
    ws.stop();
    let health = ws.json(&["git", "health"]);
    assert_eq!(health["cli_login"], "octocat", "{health}");
    // A global config with one helper, then one with nothing in it, and a gh
    // in each of its moods.
    let one = ws.file(
        "one-helper-gitconfig",
        &format!("[credential]\n\thelper = !sh {}\n", helper.display()),
    );
    ws.set_env("GIT_CONFIG_GLOBAL", &one.to_string_lossy());
    let words = ws.ok(&["git", "health"]);
    assert!(
        said(&words).contains("helper") && !said(&words).contains("helpers"),
        "one helper, singular: {}",
        said(&words)
    );
    let empty = ws.file("empty-gitconfig", "");
    ws.set_env("GIT_CONFIG_GLOBAL", &empty.to_string_lossy());
    ws.set_env("GH_JOURNEY_MODE", "unsigned");
    let health = ws.json(&["git", "health"]);
    assert_eq!(health["cli"]["installed"], true, "{health}");
    assert!(health["cli"]["version"].is_null(), "{health}");
    assert!(
        health["cli_login"].is_null() && health["resolves"].is_null(),
        "nobody answers: {health}"
    );
    let connection = ws.json(&["git", "connection", &web]);
    assert!(
        connection["cautions"][0]["id"] == "no_account",
        "nobody is signed in: {connection}"
    );
    assert!(
        connection["transport"]["helpers"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "{connection}"
    );
    ws.set_env("GH_JOURNEY_MODE", "refusing");
    assert_eq!(
        ws.json(&["git", "account", "check", "octocat"])["state"],
        "refused"
    );
    ws.set_env("GH_JOURNEY_MODE", "private");
    let probed = ws.bisa(&["--json", "git", "connection", &web, "--check"]);
    assert!(probed.status.success(), "{}", said(&probed));
    let value: Value = serde_json::from_slice(&probed.stdout).expect("json");
    assert_eq!(value["check"]["access"]["found"], false, "{value}");
    ws.set_env("GH_JOURNEY_MODE", "");
    let twin = ws.json(&[
        "project",
        "clone",
        bare.to_str().unwrap(),
        "--slug",
        "twin",
        "--committer",
        COMMITTER,
    ]);
    let twin = twin["project"]["id"].as_str().unwrap().to_string();
    let connection = ws.json(&["git", "connection", &twin]);
    assert_eq!(connection["remote"]["protocol"], "local", "{connection}");
    let probed = ws.bisa(&["--json", "git", "connection", &twin, "--check"]);
    assert!(probed.status.success(), "{}", said(&probed));
    let value: Value = serde_json::from_slice(&probed.stdout).expect("json");
    assert_eq!(value["check"]["ls_remote"]["ok"], true, "{value}");
    assert!(value["check"]["access"].is_null(), "{value}");
    assert_eq!(ws.json(&["git", "login"])["kind"], "cli");
    assert_eq!(ws.json(&["git", "account", "list"])["cli_login"], "octocat");
    assert_eq!(
        ws.json(&["git", "account", "check", "octocat"])["state"],
        "connected"
    );
    assert_eq!(
        ws.json(&["git", "account", "default", "octocat"])["default"],
        "octocat"
    );
    assert!(ws.json(&["git", "account", "default", "--clear"])["default"].is_null());
    ws.ok(&["git", "account", "rm", "nobody"]);
    let saved = ws.json(&[
        "git",
        "profile",
        "set",
        "widgets",
        "--label",
        "Widgets",
        "--owner",
        "widgets",
        "--name",
        "W",
        "--email",
        "w@widgets.test",
    ]);
    assert_eq!(saved["slug"], "widgets", "{saved}");
    assert_eq!(
        ws.json(&["git", "profile", "list"])["profiles"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    ws.json(&["git", "profile", "rm", "widgets"]);
    let connection = ws.json(&["git", "connection", &web]);
    assert_eq!(connection["code_host"], "github", "{connection}");
    let probed = ws.bisa(&["--json", "git", "connection", &web, "--check"]);
    assert!(probed.status.success(), "{}", said(&probed));
    let y = a_branch(&ws, &web, "y", None);
    let opened = ws.json(&["workstream", "pr", &y, "--title", "Add y"]);
    assert_eq!(opened["opened"], true, "{opened}");
    around_a_pull_request(&ws, &y);
    // A pull request with no check runs, and a merge the host does not land.
    let z = a_branch(&ws, &web, "z", None);
    ws.json(&["workstream", "pr", &z, "--title", "Add z"]);
    ws.set_env("GH_JOURNEY_MODE", "nochecks");
    let checks = ws.ok(&["workstream", "pr-checks", &z]);
    assert!(said(&checks).contains("no check"), "{}", said(&checks));
    ws.set_env("GH_JOURNEY_MODE", "stubborn");
    let merged = ws.json(&["workstream", "pr-merge", &z]);
    assert_eq!(merged["merged"], false, "{merged}");
    // A check that failed, as the host said it; no checks as the real gh
    // says it — exit 1 and nothing on stdout — is no checks, not a failure.
    ws.set_env("GH_JOURNEY_MODE", "failing");
    let checks = ws.json(&["workstream", "pr-checks", &z]);
    assert_eq!(checks["checks"][0]["conclusion"], "failure", "{checks}");
    ws.set_env("GH_JOURNEY_MODE", "nochecks-real");
    let checks = ws.ok(&["workstream", "pr-checks", &z]);
    assert!(said(&checks).contains("no check"), "{}", said(&checks));
    // Merged on the web, not here: the next read says so with the facts,
    // the record follows — the host's word is the second writer of merged —
    // and the workstream closes like any merged one.
    ws.set_env("GH_JOURNEY_MODE", "merged-elsewhere");
    let viewed = ws.json(&["workstream", "pr-view", &z]);
    assert_eq!(viewed["pr"]["state"], "merged", "{viewed}");
    assert_eq!(viewed["pr"]["merge_commit"], "deadbeef", "{viewed}");
    assert_eq!(viewed["pr"]["merge_state"], "clean", "{viewed}");
    assert_eq!(
        viewed["pr"]["merged_at"], "2026-01-01T00:00:00Z",
        "{viewed}"
    );
    assert_eq!(
        ws.json(&["workstream", "show", &z])["workstream"]["state"]["state"],
        "merged"
    );
    ws.set_env("GH_JOURNEY_MODE", "");
    let closed = ws.json(&["workstream", "close", &z]);
    assert_eq!(closed["workstream"]["state"]["state"], "closed", "{closed}");
}
