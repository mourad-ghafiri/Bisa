//! The CLI's help in the person's language ([17 — Internationalisation](../../../docs/architecture/17-internationalisation.md)).
//!
//! The derive keeps its doc comments — they are the rustdoc, and clap's
//! default `about` and `help` — and the catalog carries the same words as
//! messages: `cli-cmd-<path>` with `.about` for every command (and
//! `.after-help` where a command says more under its help — the root names
//! the website and the source), and
//! `cli-arg-<path>-<id>` with `.help` for every argument that has help, the
//! path being the command names below the root joined with `-` (`agent-add`),
//! the root itself `bisa`. [`localize`] walks the built `Command` and sets
//! each word from the catalog for the chosen language; a message the catalog
//! lacks leaves clap's own. The tests hold every command and argument to its
//! message, and the message to the doc comment, so the two never drift.
//!
//! The language is decided before clap parses — `--lang` read from the raw
//! arguments ([`locale_from_args`]), else the environment — since the help
//! clap prints is already in it.

use bisa_core::Text;
use bisa_i18n::Locale;
use clap::Command;

/// `--lang <tag>` or `--lang=<tag>` in the raw arguments, negotiated; else the environment's.
pub fn locale_from_args<S: AsRef<str>>(args: &[S]) -> Locale {
    let mut it = args.iter().map(|a| a.as_ref());
    while let Some(a) = it.next() {
        if a == "--lang" {
            if let Some(tag) = it.next() {
                return Locale::negotiate(&[tag]);
            }
        } else if let Some(tag) = a.strip_prefix("--lang=") {
            return Locale::negotiate(&[tag]);
        }
    }
    Locale::from_env()
}

/// The message id of a command at `path`.
pub fn command_id(path: &str) -> String {
    format!("cli-cmd-{path}")
}

/// The message id of an argument `id` of the command at `path`.
pub fn arg_id(path: &str, id: &str) -> String {
    format!("cli-arg-{path}-{id}")
}

/// The path of a subcommand `name` under `parent` (`bisa` being the root).
pub fn child_path(parent: &str, name: &str) -> String {
    if parent == "bisa" {
        name.to_string()
    } else {
        format!("{parent}-{name}")
    }
}

fn attribute(locale: &Locale, id: &str, attr: &str) -> Option<String> {
    bisa_i18n::attribute(locale, id, attr, &Text::new(id.to_string()))
}

/// The command tree with every `about` and `help` said in `locale`, where
/// the catalog has the words; `path` is the command's own (`bisa` at the root).
pub fn localize(cmd: Command, locale: &Locale, path: &str) -> Command {
    let mut cmd = cmd;
    if let Some(about) = attribute(locale, &command_id(path), "about") {
        cmd = cmd.about(about);
    }
    if let Some(after) = attribute(locale, &command_id(path), "after-help") {
        cmd = cmd.after_help(after);
    }
    // Every argument where it stands: a positional's place is its order
    // among the arguments, and `mut_arg` takes the one it changes out and
    // puts it back last — `<id> <step>` would be read as `<step> <id>` the
    // day only one of the two had words in the catalog.
    cmd = cmd.mut_args(|arg| {
        if arg.is_hide_set() {
            return arg;
        }
        let id = arg.get_id().to_string();
        match attribute(locale, &arg_id(path, &id), "help") {
            Some(help) => arg.help(help),
            None => arg,
        }
    });
    let subs: Vec<String> = cmd
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect();
    for name in subs {
        let child = child_path(path, &name);
        let locale = locale.clone();
        cmd = cmd.mut_subcommand(&name, move |c| localize(c, &locale, &child));
    }
    cmd
}

/// A failure said to the person: a `Text` at the top — a refusal the CLI
/// made — rendered in the run's language with its causes after it; anything
/// else as anyhow prints it.
pub fn said(out: &crate::output::Out, e: &anyhow::Error) -> String {
    match e.downcast_ref::<Text>() {
        Some(text) => {
            let mut s = out.text(text);
            for cause in e.chain().skip(1) {
                s.push_str(": ");
                s.push_str(&cause.to_string());
            }
            s
        }
        None => format!("{e:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn plain(s: Option<&clap::builder::StyledStr>) -> String {
        s.map(|v| v.to_string().trim().trim_end_matches('.').to_string())
            .unwrap_or_default()
    }

    /// Every command and every argument with help, against the English catalog.
    fn check(cmd: &Command, path: &str, faults: &mut Vec<String>) {
        let en = Locale::english();
        let cid = command_id(path);
        match attribute(&en, &cid, "about") {
            // A command with no doc comment owes no words.
            None if plain(cmd.get_about()).is_empty() => {}
            None => faults.push(format!(
                "{cid}: no `.about` in locales/en/cli.ftl; the doc comment {:?}",
                plain(cmd.get_about())
            )),
            Some(about) => {
                let derive = plain(cmd.get_about());
                if !derive.is_empty() && about.trim_end_matches('.') != derive {
                    faults.push(format!(
                        "{cid}: the catalog says {about:?}, the doc comment {derive:?}"
                    ));
                }
            }
        }
        for arg in cmd.get_arguments() {
            if arg.is_hide_set() || arg.get_help().is_none() {
                continue;
            }
            let id = arg.get_id().to_string();
            if id == "help" || id == "version" {
                continue;
            }
            let aid = arg_id(path, &id);
            match attribute(&en, &aid, "help") {
                None => faults.push(format!(
                    "{aid}: no `.help` in locales/en/cli.ftl; the doc comment {:?}",
                    plain(arg.get_help())
                )),
                Some(help) => {
                    let derive = plain(arg.get_help());
                    if help.trim_end_matches('.') != derive {
                        faults.push(format!(
                            "{aid}: the catalog says {help:?}, the doc comment {derive:?}"
                        ));
                    }
                }
            }
        }
        for sub in cmd.get_subcommands() {
            if sub.is_hide_set() || sub.get_name() == "help" {
                continue;
            }
            check(sub, &child_path(path, sub.get_name()), faults);
        }
    }

    #[test]
    fn every_command_and_argument_has_its_words_and_they_are_the_doc_comment_s() {
        let mut faults = Vec::new();
        check(&crate::Cli::command(), "bisa", &mut faults);
        assert!(
            faults.is_empty(),
            "the help and the catalog disagree:\n{}",
            faults.join("\n")
        );
    }

    #[test]
    fn the_localized_tree_keeps_its_shape_and_the_english_words() {
        let cmd = localize(crate::Cli::command(), &Locale::english(), "bisa");
        assert_eq!(cmd.get_name(), "bisa");
        let new = cmd.find_subcommand("new").expect("`new` is a verb");
        assert!(plain(new.get_about()).starts_with("Capture a new goal"));
        assert!(cmd.find_subcommand("agent").is_some());
    }

    /// What a person types after a command, in the order it is read, for the
    /// command at `path` and every one under it.
    fn positionals(cmd: &Command, path: &str, out: &mut Vec<(String, Vec<String>)>) {
        out.push((
            path.to_string(),
            cmd.get_positionals()
                .map(|a| a.get_id().to_string())
                .collect(),
        ));
        for sub in cmd.get_subcommands() {
            positionals(sub, &child_path(path, sub.get_name()), out);
        }
    }

    /// Saying the help in a language moves no argument: every command reads
    /// what is typed after it in the order its verb declares.
    #[test]
    fn the_localized_tree_reads_its_arguments_in_the_order_they_are_declared() {
        let (mut declared, mut localized) = (Vec::new(), Vec::new());
        positionals(&crate::Cli::command(), "bisa", &mut declared);
        positionals(
            &localize(crate::Cli::command(), &Locale::english(), "bisa"),
            "bisa",
            &mut localized,
        );
        assert!(
            declared.iter().any(|(_, ids)| ids.len() > 1),
            "no command takes two positionals: the test would hold nothing"
        );
        let moved: Vec<_> = declared
            .iter()
            .zip(&localized)
            .filter(|(before, after)| before != after)
            .collect();
        assert!(moved.is_empty(), "arguments changed places: {moved:#?}");

        // The one a person met: a step is answered by `<goal or run> <step>`.
        let answer = localize(crate::Cli::command(), &Locale::english(), "bisa")
            .try_get_matches_from([
                "bisa",
                "step",
                "answer",
                "a-goal-or-a-run",
                "ask",
                "-o",
                "yes",
            ])
            .expect("the verb parses");
        let (_, step) = answer.subcommand().expect("step");
        let (_, answer) = step.subcommand().expect("answer");
        assert_eq!(
            answer.get_one::<String>("id").map(String::as_str),
            Some("a-goal-or-a-run")
        );
        assert_eq!(
            answer.get_one::<String>("step").map(String::as_str),
            Some("ask")
        );
    }

    /// Every name a person may type and every word of help under `cmd`.
    fn words(cmd: &Command, out: &mut Vec<String>) {
        out.push(cmd.get_name().to_string());
        out.extend(cmd.get_all_aliases().map(str::to_string));
        out.push(plain(cmd.get_about()));
        for arg in cmd.get_arguments() {
            out.push(arg.get_id().to_string());
            out.push(plain(arg.get_help()));
        }
        for sub in cmd.get_subcommands() {
            words(sub, out);
        }
    }

    /// The verbs of listening are in the tree with their words, and the
    /// feature they replaced left nothing behind: no verb, no alias — hidden
    /// or not — and no word of help.
    #[test]
    fn the_listening_verbs_are_said_and_the_retired_feature_left_no_word() {
        let cmd = localize(crate::Cli::command(), &Locale::english(), "bisa");
        for (noun, verbs) in [
            (
                "workflow",
                &["on", "off", "listeners", "hook-secret", "run"][..],
            ),
            ("signal", &["emit", "list", "release"][..]),
        ] {
            let parent = cmd
                .find_subcommand(noun)
                .unwrap_or_else(|| panic!("`{noun}` is a verb"));
            for verb in verbs {
                let sub = parent
                    .find_subcommand(verb)
                    .unwrap_or_else(|| panic!("`{noun} {verb}` is a verb"));
                assert!(
                    !plain(sub.get_about()).is_empty(),
                    "`{noun} {verb}` says what it does"
                );
            }
        }
        // Spelt in two halves, so this source does not carry the word either.
        let retired = ["trig", "ger"].concat();
        let mut said = Vec::new();
        words(&cmd, &mut said);
        let left: Vec<&String> = said
            .iter()
            .filter(|word| word.to_lowercase().contains(&retired))
            .collect();
        assert!(left.is_empty(), "the retired word is still said: {left:?}");
    }

    #[test]
    fn the_language_is_read_from_the_raw_arguments_before_clap_parses() {
        assert_eq!(
            locale_from_args(&["bisa", "--lang", "en", "goals"]).tag(),
            "en"
        );
        assert_eq!(
            locale_from_args(&["bisa", "--lang=fr", "goals"]).tag(),
            "en",
            "a language not shipped is English"
        );
        assert_eq!(
            locale_from_args(&["bisa", "goals"]).tag(),
            Locale::from_env().tag()
        );
    }
}
