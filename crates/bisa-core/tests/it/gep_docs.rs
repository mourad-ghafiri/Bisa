//! `docs/reference/gep.md` is the table of kinds; `bisa_core::kind` is the
//! registry. The page is held to the code both ways: every number in its two
//! tables is a kind the registry has and every kind the registry has is a
//! row, a journal fact is never addressable, the retired holes are the four
//! the registry keeps out, the counts and the next free number are the
//! registry's, and the ephemeral kind is the one named. A kind added to the
//! code without a row — or a row the code lost — fails here.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use bisa_core::kind::{is_addressable, is_gep_kind, ADDRESSABLE_KINDS, EPHEMERAL_KINDS, GEP_KINDS};
use std::collections::BTreeSet;
use std::path::Path;

fn page() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/gep.md");
    std::fs::read_to_string(&path).expect("read docs/reference/gep.md")
}

/// A row of a kinds table: its number, and whether it is a retired hole.
struct Row {
    kind: u16,
    retired: bool,
}

/// The rows of the two tables in the page's order — the journal facts, then
/// the addressable snapshots, split at the sentence that opens the second —
/// read up to the envelopes section.
fn tables(page: &str) -> (Vec<Row>, Vec<Row>) {
    let mut journal = Vec::new();
    let mut addressable = Vec::new();
    let mut second = false;
    for line in page.lines() {
        if line.starts_with("Addressable snapshots") {
            second = true;
        }
        if line.starts_with("## Envelopes") {
            break;
        }
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let Some(end) = rest.find('`') else {
            continue;
        };
        let Ok(kind) = rest[..end].parse::<u16>() else {
            continue;
        };
        let row = Row {
            kind,
            retired: line.contains("*retired*"),
        };
        if second {
            addressable.push(row);
        } else {
            journal.push(row);
        }
    }
    (journal, addressable)
}

fn live(rows: &[Row]) -> BTreeSet<u16> {
    rows.iter().filter(|r| !r.retired).map(|r| r.kind).collect()
}

fn retired(rows: &[Row]) -> BTreeSet<u16> {
    rows.iter().filter(|r| r.retired).map(|r| r.kind).collect()
}

/// The paragraph that begins with `opening`: its lines up to the next blank
/// one, joined by a space.
fn paragraph(page: &str, opening: &str) -> String {
    let mut lines = page.lines().skip_while(|l| !l.starts_with(opening));
    let first = lines
        .next()
        .unwrap_or_else(|| panic!("a paragraph opening {opening:?}"));
    let mut out = first.to_string();
    for line in lines.take_while(|l| !l.trim().is_empty()) {
        out.push(' ');
        out.push_str(line);
    }
    out
}

/// `twenty-six` → 26, `fourteen` → 14: the number words a count is written in.
fn number_word(word: &str) -> usize {
    const UNITS: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [(&str, usize); 8] = [
        ("twenty", 20),
        ("thirty", 30),
        ("forty", 40),
        ("fifty", 50),
        ("sixty", 60),
        ("seventy", 70),
        ("eighty", 80),
        ("ninety", 90),
    ];
    let word = word.to_ascii_lowercase();
    let (tens, unit) = match word.split_once('-') {
        Some((t, u)) => (t, Some(u)),
        None => (word.as_str(), None),
    };
    if let Some(n) = UNITS.iter().position(|u| *u == tens) {
        assert!(unit.is_none(), "{word:?} is no number word");
        return n;
    }
    let (_, base) = TENS
        .iter()
        .find(|(t, _)| *t == tens)
        .unwrap_or_else(|| panic!("{word:?} is no number word"));
    base + unit.map_or(0, |u| {
        UNITS
            .iter()
            .position(|x| *x == u)
            .unwrap_or_else(|| panic!("{word:?} is no number word"))
    })
}

/// The word just before `phrase` in `text`.
fn word_before<'a>(text: &'a str, phrase: &str) -> &'a str {
    let at = text
        .find(phrase)
        .unwrap_or_else(|| panic!("the page says {phrase:?}"));
    text[..at]
        .split_whitespace()
        .last()
        .unwrap_or_else(|| panic!("a word before {phrase:?}"))
}

#[test]
fn the_two_tables_are_the_registry_and_nothing_else() {
    let page = page();
    let (journal, addressable) = tables(&page);
    assert!(
        journal.len() > 10 && addressable.len() > 10,
        "the parser found {} journal rows and {} addressable rows",
        journal.len(),
        addressable.len()
    );
    // The ephemeral kind has no row: it is named in the prose below the
    // tables ("Ephemeral `23400` observer frames are never wrapped"), and
    // the third test holds that sentence to `EPHEMERAL_KINDS`.
    let prose = paragraph(&page, "Four numbers are **retired as holes**");
    let ephemeral: BTreeSet<u16> = EPHEMERAL_KINDS
        .iter()
        .copied()
        .filter(|k| prose.contains(&format!("Ephemeral `{k}`")))
        .collect();
    for kind in &ephemeral {
        assert!(
            !live(&journal).contains(kind) && !live(&addressable).contains(kind),
            "the ephemeral kind {kind} is a row of a table: it is neither a journal fact nor a snapshot"
        );
    }
    let documented: BTreeSet<u16> = live(&journal)
        .union(&live(&addressable))
        .copied()
        .chain(ephemeral)
        .collect();
    let registry: BTreeSet<u16> = GEP_KINDS.iter().copied().collect();
    let undocumented: Vec<u16> = registry.difference(&documented).copied().collect();
    let phantom: Vec<u16> = documented.difference(&registry).copied().collect();
    assert!(
        undocumented.is_empty(),
        "kinds of the registry with no row on the page: {undocumented:?}"
    );
    assert!(
        phantom.is_empty(),
        "rows on the page the registry does not have: {phantom:?}"
    );
    let addressable_registry: BTreeSet<u16> = ADDRESSABLE_KINDS.iter().copied().collect();
    assert_eq!(
        live(&addressable),
        addressable_registry,
        "the snapshots table and ADDRESSABLE_KINDS differ"
    );
    for kind in live(&journal) {
        assert!(
            !is_addressable(kind),
            "{kind} is a journal fact on the page and addressable in the code"
        );
    }
}

#[test]
fn the_retired_holes_are_the_four_the_registry_keeps_out_and_each_is_explained() {
    let page = page();
    let (journal, addressable) = tables(&page);
    let holes: BTreeSet<u16> = retired(&journal)
        .union(&retired(&addressable))
        .copied()
        .collect();
    assert_eq!(
        holes,
        [3405, 3409, 33406, 33410]
            .into_iter()
            .collect::<BTreeSet<u16>>()
    );
    for hole in &holes {
        assert!(
            !is_gep_kind(*hole) && !is_addressable(*hole),
            "{hole} is retired on the page and live in the registry"
        );
    }
    let why = paragraph(&page, "Four numbers are **retired as holes**");
    for hole in &holes {
        assert!(
            why.contains(&format!("`{hole}`")),
            "the holes paragraph does not explain {hole}"
        );
    }
}

#[test]
fn the_counts_and_the_next_free_number_are_the_registrys() {
    let page = page();
    let prose = paragraph(&page, "Four numbers are **retired as holes**");
    assert_eq!(
        number_word(word_before(&prose, " kinds are GEP kinds")),
        GEP_KINDS.len(),
        "the page's count of GEP kinds"
    );
    assert_eq!(
        number_word(word_before(&prose, " are addressable")),
        ADDRESSABLE_KINDS.len(),
        "the page's count of addressable kinds"
    );
    let after = prose
        .split("the next free addressable number is `")
        .nth(1)
        .expect("the next free number");
    let next: u16 = after[..after.find('`').expect("the number's closing tick")]
        .parse()
        .expect("a number");
    let highest = ADDRESSABLE_KINDS.iter().max().copied().expect("a kind");
    assert_eq!(next, highest + 1, "the next free addressable number");
    assert!(!is_gep_kind(next), "{next} is already a kind");
    for kind in EPHEMERAL_KINDS {
        assert!(
            prose.contains(&format!("Ephemeral `{kind}`")),
            "the ephemeral kind {kind} is not the one the page names"
        );
    }
    // Number words, held to themselves at the edges the page may reach.
    assert_eq!(number_word("Twenty-six"), 26);
    assert_eq!(number_word("fourteen"), 14);
    assert_eq!(number_word("thirty"), 30);
}
