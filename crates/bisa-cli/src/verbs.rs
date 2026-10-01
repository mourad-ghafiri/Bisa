//! Every verb of the command line is run by a test that names it.
//!
//! The tree is clap's own, so a verb added is a verb this guard asks a test
//! for. The tests are read as sources, under `tests/`: a verb is named where
//! its words stand one after another in an argument list
//! (`&["workstream", "open", …]`, `.arg("node")`). A verb a test reaches by
//! another road is excused by name, with the road it takes — and an excuse
//! nothing needs any more fails, so the list cannot outlive its reasons.

// Test code whole, and it says so at its head: its words are a developer's,
// and the language ratchet reads none of them.
#![cfg(test)]

use clap::{Command, CommandFactory as _};
use std::path::Path;

/// Verbs no argument list names, and the road a test takes to each.
const BY_ANOTHER_ROAD: &[(&str, &str)] = &[
    (
        "session report",
        "a harness's hooks are command lines the node writes: \
         `e2e/a_harness_in_a_terminal.rs` runs each one through `sh` as the harness does",
    ),
    (
        "session guard",
        "the second `PreToolUse` line of the same hooks, run through `sh` by \
         `e2e/a_harness_in_a_terminal.rs`, its verdict read off its output",
    ),
    (
        "git ssh keys",
        "the binary's SSH is the real programs over the person's SSH directory, which no test \
         reads: the engine's `gitsetup::ssh_keys_are_listed_generated_loaded_and_tested_through_the_fake…` \
         and the node's `ssh::the_overview_lists_the_pairs_on_disk_and_what_the_agent_holds`, on a fake",
    ),
    (
        "git ssh keygen",
        "as `git ssh keys`: the node's `ssh::a_pair_is_generated_as_files_a_taken_name_is_refused_before_any_spawn_and_a_key_is_loaded`",
    ),
    (
        "git ssh load",
        "as `git ssh keys`: the same node test, and `gitsetup::tests` here for the route a name becomes",
    ),
    (
        "git ssh resolve",
        "as `git ssh keys`: the node's `ssh::ssh_g_is_read_for_a_host_and_a_greeting_is_one_handshake_in_batch_mode`, \
         and `gitsetup::tests` here for the query a host and a key become",
    ),
    (
        "git ssh test",
        "a handshake with a git host, which no test makes: the same node test, on a fake",
    ),
];

/// Every leaf of the tree, as the words a person types after `bisa`.
fn leaves(cmd: &Command, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    let mut has_verbs = false;
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        has_verbs = true;
        path.push(sub.get_name().to_string());
        leaves(sub, path, out);
        path.pop();
    }
    if !has_verbs && !path.is_empty() {
        out.push(path.clone());
    }
}

/// Every Rust source under `dir`, its whitespace taken out, so a list laid
/// out over several lines reads as one laid out on one.
fn sources(dir: &Path, out: &mut String) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            out.extend(text.chars().filter(|c| !c.is_whitespace()));
            out.push('\n');
        }
    }
}

/// What stands before a verb once the flags written ahead of it are passed
/// over: `["--json","status"` opens as `["status"` does.
fn before_flags(mut before: &str) -> &str {
    while let Some(rest) = before.strip_suffix("\",") {
        let Some(opening) = rest.rfind('"') else {
            break;
        };
        if !rest[opening + 1..].starts_with("--") {
            break;
        }
        before = &rest[..opening];
    }
    before
}

/// Whether the sources name the verb: its words one after another, quoted,
/// at the head of an argument list — or, for a verb of several words,
/// anywhere in one.
fn named(sources: &str, verb: &[String]) -> bool {
    let needle = verb
        .iter()
        .map(|word| format!("\"{word}\""))
        .collect::<Vec<_>>()
        .join(",");
    sources.match_indices(&needle).any(|(at, _)| {
        let before = before_flags(&sources[..at]);
        let opens = before.ends_with('[')
            || before.ends_with("arg(")
            || (verb.len() > 1 && before.ends_with(','));
        let closes = matches!(
            sources[at + needle.len()..].chars().next(),
            Some(',' | ']' | ')')
        );
        opens && closes
    })
}

fn words(verb: &str) -> Vec<String> {
    verb.split(' ').map(str::to_string).collect()
}

#[test]
fn a_verb_is_named_where_its_words_head_an_argument_list() {
    let text: String = r#"
        ws.json(&["project", "new", slug]);
        ws.ok(&[
            "workstream",
            "open",
            &project,
        ]);
        bisa(&dir, &["--json", "status"]);
        Command::new(bin).arg("--data-dir").arg(dir).arg("node");
        json!({ "kind": "new", "slug": "lathe" });
        let turn = ["say", "stop"];
    "#
    .chars()
    .filter(|c| !c.is_whitespace())
    .collect();
    for verb in ["project new", "workstream open", "status", "node"] {
        assert!(named(&text, &words(verb)), "{verb}");
    }
    // A word that only stands somewhere in a list, or as a value, names no
    // verb of one word; and a verb's group with another word is another verb.
    for verb in ["new", "open", "stop", "workstream close"] {
        assert!(!named(&text, &words(verb)), "{verb}");
    }
    assert!(named(&text, &words("say")), "the head of a list is a verb");
}

#[test]
fn every_verb_is_run_by_a_test_that_names_it() {
    let mut all = Vec::new();
    leaves(&crate::Cli::command(), &mut Vec::new(), &mut all);
    assert!(
        all.len() > 100,
        "the walk found {} verbs; it is broken",
        all.len()
    );
    let mut text = String::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut text,
    );

    let excused = |verb: &str| BY_ANOTHER_ROAD.iter().any(|(v, _)| *v == verb);
    let unnamed: Vec<String> = all
        .iter()
        .filter(|verb| !named(&text, verb))
        .map(|verb| verb.join(" "))
        .filter(|verb| !excused(verb))
        .collect();
    assert!(
        unnamed.is_empty(),
        "{} of {} verbs are run by no test — write one, or say the road a test takes to it:\n{}",
        unnamed.len(),
        all.len(),
        unnamed.join("\n")
    );

    let idle: Vec<&str> = BY_ANOTHER_ROAD
        .iter()
        .map(|(verb, _)| *verb)
        .filter(|verb| !all.contains(&words(verb)) || named(&text, &words(verb)))
        .collect();
    assert!(
        idle.is_empty(),
        "excused, and either no verb any more or named by a test after all: {idle:?}"
    );
    for (verb, road) in BY_ANOTHER_ROAD {
        assert!(!road.trim().is_empty(), "{verb} is excused with no road");
    }
}
