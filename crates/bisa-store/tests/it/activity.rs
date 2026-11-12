//! The activity feed's index: every seam produces a row, a page is exact by
//! keyset, a concept answers only its own, and a rebuild holds it all again.

use bisa_core::event::JournalPayload;
use bisa_core::{
    ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind, Home, MessageBody,
    RosterPolicy, RunScope, Tags,
};
use bisa_store::{ActivityCursor, MemoryKeyStore, NewGoal, NewProject, PostOrigin, Workspace};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn channel(ws: &Workspace, name: &str) -> String {
    ws.create_channel(
        name,
        None,
        RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![],
        },
        Tags::default(),
    )
    .unwrap()
    .id
    .as_str()
    .to_string()
}

fn engine_fact(
    at: u64,
    concept: ActivityConcept,
    kind: &str,
    source: ActivitySource,
) -> ActivityFact {
    ActivityFact {
        at,
        concept,
        kind: kind.into(),
        source,
        author: None,
        event: serde_json::json!({ "type": kind, "at": at }),
    }
}

#[test]
fn the_three_seams_each_produce_a_row_and_a_concept_answers_only_its_own() {
    let (_dir, ws) = ws();
    // A journal fact: through append.
    let goal = ws.create_goal(NewGoal::captured("Ship it")).unwrap().id;
    ws.append_journal(
        &Home::Goal { goal },
        JournalPayload::Note {
            text: "first".into(),
        },
        ws.owner_keys(),
        None,
    )
    .unwrap();
    // A message: through a post in a standing channel.
    let channel = channel(&ws, "watercooler");
    ws.post_message(
        &channel,
        MessageBody::Post {
            text: "hello everyone".into(),
            context: vec![],
            artifacts: vec![],
            thinking: None,
            said: None,
        },
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    // An engine fact: through the log.
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    ws.record_activity(
        &engine_fact(
            9,
            ActivityConcept::Projects,
            "project_created",
            ActivitySource::new(ActivitySourceKind::Project, project.id.to_string()),
        ),
        true,
    )
    .unwrap();

    let all = ws.activity_page(None, None, 50).unwrap();
    let kinds: Vec<&str> = all.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains(&"note"), "{kinds:?}");
    assert!(kinds.contains(&"message"), "{kinds:?}");
    assert!(kinds.contains(&"project_created"), "{kinds:?}");

    let goals = ws
        .activity_page(Some(ActivityConcept::Goals), None, 50)
        .unwrap();
    assert!(goals.iter().all(|r| r.concept == "goals"));
    // Two notes: the capture's own ("goal captured: …") and the one appended.
    assert_eq!(goals.iter().filter(|r| r.kind == "note").count(), 2);
    let note = goals
        .iter()
        .find(|r| r.kind == "note" && r.event.contains("\"first\""))
        .expect("the appended note is a row");
    assert_eq!(note.source_kind, "goal");
    assert_eq!(note.source_id, goal.to_string());
    assert!(note.author.is_some(), "a journal fact names who signed it");
    let event: serde_json::Value = serde_json::from_str(&note.event).unwrap();
    assert_eq!(event["text"], "first", "the payload rides verbatim");

    let channels = ws
        .activity_page(Some(ActivityConcept::Channels), None, 50)
        .unwrap();
    assert_eq!(channels.len(), 1);
    let m: serde_json::Value = serde_json::from_str(&channels[0].event).unwrap();
    assert_eq!(m["type"], "message");
    assert_eq!(m["snippet"], "hello everyone");
    assert_eq!(m["body_kind"], "post");
    assert_eq!(channels[0].source_kind, "channel");
    assert_eq!(channels[0].source_id, channel);

    let projects = ws
        .activity_page(Some(ActivityConcept::Projects), None, 50)
        .unwrap();
    assert_eq!(
        projects.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
        ["project_created"]
    );
    assert!(
        ws.activity_page(Some(ActivityConcept::Node), None, 50)
            .unwrap()
            .is_empty(),
        "nothing happened to the node"
    );
}

#[test]
fn a_page_is_exact_by_keyset_across_a_shared_second() {
    let (_dir, ws) = ws();
    // Five facts in one second, three in the next: the cursor is (at, seq),
    // so a page edge inside a second loses nothing and repeats nothing.
    for i in 0..5 {
        ws.record_activity(
            &engine_fact(
                100,
                ActivityConcept::Node,
                &format!("k{i}"),
                ActivitySource::node(),
            ),
            false,
        )
        .unwrap();
    }
    for i in 5..8 {
        ws.record_activity(
            &engine_fact(
                101,
                ActivityConcept::Node,
                &format!("k{i}"),
                ActivitySource::node(),
            ),
            false,
        )
        .unwrap();
    }
    let mut seen = Vec::new();
    let mut cursor: Option<ActivityCursor> = None;
    loop {
        let page = ws.activity_page(None, cursor, 3).unwrap();
        if page.is_empty() {
            break;
        }
        let last = page.last().unwrap();
        cursor = Some(ActivityCursor {
            at: last.at,
            seq: last.seq,
        });
        seen.extend(page.into_iter().map(|r| r.kind));
    }
    assert_eq!(
        seen,
        ["k7", "k6", "k5", "k4", "k3", "k2", "k1", "k0"],
        "newest first, every row once"
    );
}

#[test]
fn a_rebuilt_index_holds_every_row_again() {
    let (dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("Ship it")).unwrap().id;
    ws.append_journal(
        &Home::Goal { goal },
        JournalPayload::Note {
            text: "kept".into(),
        },
        ws.owner_keys(),
        None,
    )
    .unwrap();
    let channel = channel(&ws, "lounge");
    ws.post_message(
        &channel,
        MessageBody::Post {
            text: "still here".into(),
            context: vec![],
            artifacts: vec![],
            thinking: None,
            said: None,
        },
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    ws.record_activity(
        &engine_fact(7, ActivityConcept::Node, "paused", ActivitySource::node()),
        true,
    )
    .unwrap();
    // A fact recorded without durability is the index's alone: a rebuild forgets it, by design.
    ws.record_activity(
        &engine_fact(
            8,
            ActivityConcept::Node,
            "ephemeral",
            ActivitySource::node(),
        ),
        false,
    )
    .unwrap();
    let before: Vec<String> = ws
        .activity_page(None, None, 50)
        .unwrap()
        .into_iter()
        .map(|r| r.kind)
        .collect();
    assert!(before.contains(&"ephemeral".to_string()));
    for kind in ["note", "message", "paused"] {
        assert!(
            before.contains(&kind.to_string()),
            "{kind} before the rebuild: {before:?}"
        );
    }
    let mut durable_before: Vec<String> = before
        .iter()
        .filter(|k| *k != "ephemeral")
        .cloned()
        .collect();
    durable_before.sort();
    drop(ws);

    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    ws.rebuild_index().unwrap();
    let after: Vec<String> = ws
        .activity_page(None, None, 50)
        .unwrap()
        .into_iter()
        .map(|r| r.kind)
        .collect();
    // The rebuild reproduces the feed row for row — the capture note, the
    // kept note, the message, the durable fact — and nothing more.
    let mut after_sorted = after.clone();
    after_sorted.sort();
    assert_eq!(
        after_sorted,
        durable_before,
        "a rebuild holds every durable row once: {:?}",
        ws.activity_page(None, None, 50).unwrap()
    );
    assert!(
        !after.contains(&"ephemeral".to_string()),
        "what was never truth is not rebuilt"
    );
    assert!(
        dir.path()
            .join("activity")
            .read_dir()
            .unwrap()
            .next()
            .is_some(),
        "the log is a file under the workspace"
    );
}

/// The Inbox's one read: the newest rows under a set of kinds, nothing
/// outside them, whatever their concept.
#[test]
fn activity_by_kinds_answers_the_named_kinds_newest_first_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let goal = ws.create_goal(NewGoal::captured("read me")).unwrap().id;
    for (at, kind) in [
        (10, "run_started"),
        (20, "run_finished"),
        (30, "step_changed"),
        (40, "listener_failed"),
        (50, "run_finished"),
    ] {
        ws.record_activity(
            &engine_fact(
                at,
                ActivityConcept::Goals,
                kind,
                ActivitySource::new(ActivitySourceKind::Goal, goal.to_string()),
            ),
            true,
        )
        .unwrap();
    }
    let got = ws
        .activity_by_kinds(&["run_finished", "listener_failed"], 10)
        .unwrap();
    assert_eq!(
        got.iter()
            .map(|r| (r.at, r.kind.as_str()))
            .collect::<Vec<_>>(),
        [
            (50, "run_finished"),
            (40, "listener_failed"),
            (20, "run_finished")
        ],
        "newest first, only the named kinds"
    );
    assert_eq!(
        ws.activity_by_kinds(&["run_finished"], 1).unwrap().len(),
        1,
        "the limit is honoured"
    );
    assert!(
        ws.activity_by_kinds(&[], 10).unwrap().is_empty(),
        "no kinds, nothing"
    );
}

/// A run's own facts — started, a step's moves, finished — are journaled by
/// the store itself rather than appended by a caller, and they reach the feed
/// through the same seam every journal event does.
#[test]
fn a_runs_facts_are_rows_of_the_feed() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("Hold it")).unwrap().id;
    let step = |id: &str, kind: bisa_core::StepKind, then: &[&str]| bisa_core::Step {
        id: bisa_core::StepId::new(id).unwrap(),
        name: id.to_uppercase(),
        kind,
        then: then
            .iter()
            .map(|t| bisa_core::Flow::to(bisa_core::StepId::new(*t).unwrap()))
            .collect(),
        boundaries: vec![],
        join: bisa_core::Join::All,
        on_fail: bisa_core::OnFail::Fail,
        retries: 0,
        max_visits: bisa_core::DEFAULT_MAX_VISITS,
        position: None,
    };
    let wf = ws
        .create_workflow(
            bisa_store::NewWorkflow {
                name: "Held".into(),
                description: String::new(),
                inputs: vec![],
                tags: Default::default(),
                decision_making: false,
                steps: vec![
                    step(
                        "hold",
                        bisa_core::StepKind::Wait {
                            until: bisa_core::WaitFor::Release,
                        },
                        &["end"],
                    ),
                    step(
                        "end",
                        bisa_core::StepKind::End {
                            finish: bisa_core::Finish::Done,
                        },
                        &[],
                    ),
                ],
            },
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    let (run, _) = ws
        .create_run(
            RunScope::Goal { goal },
            wf.id,
            std::collections::BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();

    let goals = ws
        .activity_page(Some(ActivityConcept::Goals), None, 50)
        .unwrap();
    let mine: Vec<&str> = goals
        .iter()
        .filter(|r| r.source_id == goal.to_string())
        .map(|r| r.kind.as_str())
        .collect();
    assert!(mine.contains(&"run"), "the run's start is a row: {mine:?}");
    assert!(
        mine.contains(&"step"),
        "the held step's start is a row: {mine:?}"
    );
    let row = goals.iter().find(|r| r.kind == "run").unwrap();
    let event: serde_json::Value = serde_json::from_str(&row.event).unwrap();
    assert_eq!(
        event["run"],
        serde_json::json!(run.id.to_string()),
        "the payload rides verbatim"
    );
    assert!(
        row.author.is_some(),
        "the store signs its own facts as the owner"
    );
}

#[test]
fn a_feed_nothing_was_written_to_answers_empty_everywhere() {
    let (_dir, ws) = ws();
    for concept in ActivityConcept::ALL {
        assert!(
            ws.activity_page(Some(*concept), None, 50)
                .unwrap()
                .iter()
                .all(|r| r.concept == concept.as_str()),
            "{concept:?}"
        );
    }
    assert!(ws
        .activity_page(Some(ActivityConcept::Workflows), None, 50)
        .unwrap()
        .is_empty());
    assert!(ws
        .activity_by_kinds(&["listener_fired"], 50)
        .unwrap()
        .is_empty());
    assert!(
        ws.activity_by_kinds(&[], 50).unwrap().is_empty(),
        "no kind names no row"
    );
    // A cursor before anything is an empty page, not an error.
    let before = Some(ActivityCursor { at: 0, seq: 0 });
    assert!(ws.activity_page(None, before, 50).unwrap().is_empty());
}

#[test]
fn facts_appended_from_several_threads_each_land_once_in_the_index_and_in_the_log() {
    let (_dir, ws) = ws();
    let ws = std::sync::Arc::new(ws);
    let writers: Vec<_> = (0..4u64)
        .map(|t| {
            let ws = std::sync::Arc::clone(&ws);
            std::thread::spawn(move || {
                for i in 0..25u64 {
                    ws.record_activity(
                        &engine_fact(
                            // One second for all: the keyset's tie-break is what is under test.
                            1_700_000_000,
                            ActivityConcept::Node,
                            &format!("burst_{t}_{i}"),
                            ActivitySource::node(),
                        ),
                        true,
                    )
                    .unwrap();
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    let burst = |kinds: Vec<String>| -> Vec<String> {
        let mut k: Vec<String> = kinds
            .into_iter()
            .filter(|k| k.starts_with("burst_"))
            .collect();
        k.sort();
        k
    };
    let mut expected: Vec<String> = (0..4)
        .flat_map(|t| (0..25).map(move |i| format!("burst_{t}_{i}")))
        .collect();
    expected.sort();

    // Walked in pages of 7 across the one second: every fact once.
    let mut walked = Vec::new();
    let mut seqs = std::collections::BTreeSet::new();
    let mut cursor = None;
    loop {
        let page = ws
            .activity_page(Some(ActivityConcept::Node), cursor, 7)
            .unwrap();
        let Some(last) = page.last() else { break };
        cursor = Some(ActivityCursor {
            at: last.at,
            seq: last.seq,
        });
        for r in &page {
            assert!(seqs.insert(r.seq), "seq {} twice", r.seq);
        }
        walked.extend(page.into_iter().map(|r| r.kind));
    }
    assert_eq!(burst(walked), expected);

    // The log holds whole lines only: every fact reads back.
    let logged = bisa_store::activity_log::read_all(&ws.paths().activity_dir()).unwrap();
    assert_eq!(
        burst(logged.into_iter().map(|f| f.kind).collect()),
        expected
    );
}

#[test]
fn a_fact_the_log_holds_and_the_index_missed_is_a_row_again_after_a_rebuild() {
    let (dir, ws) = ws();
    // What a stop between the two writes of `record_activity` leaves: the
    // durable line, and no row.
    bisa_store::activity_log::append(
        &ws.paths().activity_dir(),
        &engine_fact(
            9,
            ActivityConcept::Node,
            "half_written",
            ActivitySource::node(),
        ),
    )
    .unwrap();
    let kinds = |ws: &Workspace| -> Vec<String> {
        ws.activity_page(Some(ActivityConcept::Node), None, 50)
            .unwrap()
            .into_iter()
            .map(|r| r.kind)
            .collect()
    };
    assert!(!kinds(&ws).contains(&"half_written".to_string()));
    drop(ws);

    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    ws.rebuild_index().unwrap();
    assert_eq!(
        kinds(&ws).iter().filter(|k| *k == "half_written").count(),
        1,
        "the log is the truth: the rebuild brings the fact back, once"
    );
    ws.rebuild_index().unwrap();
    assert_eq!(
        kinds(&ws).iter().filter(|k| *k == "half_written").count(),
        1,
        "a second rebuild adds nothing"
    );
}

/// The homes that ever journaled a question are read from the index — one
/// read, no journal — and are the same after a rebuild: a goal by its id, a
/// home that asked nothing absent.
#[test]
fn the_homes_that_ever_asked_are_read_from_the_index_and_survive_a_rebuild() {
    let (_d, ws) = ws();
    let asked = ws.create_goal(NewGoal::captured("asked once")).unwrap();
    let quiet = ws.create_goal(NewGoal::captured("never asked")).unwrap();
    let owner = ws.owner_keys().clone();
    ws.append_journal(
        &bisa_core::Home::Goal { goal: asked.id },
        JournalPayload::Question {
            work_item: None,
            gate: format!("ask_human:{}", asked.id),
            text: "Which database?".into(),
            expects: bisa_core::AskKind::free_text(),
        },
        &owner,
        None,
    )
    .unwrap();
    let read = |ws: &Workspace| {
        let homes = ws.activity_sources_of_kind("question").unwrap();
        (
            homes.contains(&("goal".to_string(), asked.id.to_string())),
            homes.contains(&("goal".to_string(), quiet.id.to_string())),
            homes.len(),
        )
    };
    assert_eq!(read(&ws), (true, false, 1));
    assert!(ws
        .activity_sources_of_kind("no_such_kind")
        .unwrap()
        .is_empty());
    ws.rebuild_index().unwrap();
    assert_eq!(
        read(&ws),
        (true, false, 1),
        "the rebuild replays the journal's rows"
    );
}

// ---------------------------------------------------------------------
// One row per fact, a row that can leave, and what the feed says of itself
// ---------------------------------------------------------------------

fn post(ws: &Workspace, scope: &str, text: &str) -> String {
    ws.post_message(
        scope,
        MessageBody::post(text),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap()
}

/// What the feed carries of the messages of one scope: each row's excerpt.
fn excerpts(ws: &Workspace, scope: &str) -> Vec<String> {
    ws.activity_page(Some(ActivityConcept::Channels), None, 200)
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == "message" && r.source_id == scope)
        .map(|r| {
            let event: serde_json::Value = serde_json::from_str(&r.event).unwrap();
            event["snippet"].as_str().unwrap_or_default().to_string()
        })
        .collect()
}

/// One row of the feed without its `seq`: its second, concept, kind,
/// source kind, source id, author and event.
type FeedRow = (u64, String, String, String, String, Option<String>, String);

/// The feed as a rebuild must reproduce it: every row but its `seq`, which
/// is the insertion order and no fact's own.
fn rows_of(ws: &Workspace) -> Vec<FeedRow> {
    ws.activity_page(None, None, 500)
        .unwrap()
        .into_iter()
        .map(|r| {
            (
                r.at,
                r.concept,
                r.kind,
                r.source_kind,
                r.source_id,
                r.author,
                r.event,
            )
        })
        .collect()
}

fn reopened(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

/// A message the cache already holds — a log that carries its line twice —
/// is one row of the feed and one message of its conversation. Both were
/// counted again for every time the event was indexed.
#[test]
fn a_message_indexed_twice_is_one_row_and_one_count() {
    let (_dir, ws) = ws();
    let c = ws
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workspace,
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = c.id.to_string();
    post(&ws, &scope, "said once");
    let log = ws.paths().conversation_log(&scope).unwrap();
    let line = std::fs::read_to_string(&log).unwrap();
    assert_eq!(line.lines().count(), 1);
    std::fs::write(&log, format!("{line}{line}")).unwrap();

    ws.rebuild_index().unwrap();
    assert_eq!(excerpts(&ws, &scope), ["said once"], "one row a message");
    assert_eq!(ws.conversation_row(c.id).unwrap().message_count, 1);
}

/// A retracted message's text is shown nowhere: its row leaves the feed
/// when it is taken back, and a rebuild from the files does not bring it
/// back. The feed kept the excerpt for good.
#[test]
fn a_retracted_message_leaves_the_feed_and_stays_gone_after_a_rebuild() {
    let (_dir, ws) = ws();
    let lounge = channel(&ws, "lounge");
    post(&ws, &lounge, "this stays");
    let taken_back = post(&ws, &lounge, "this was a mistake");
    assert_eq!(excerpts(&ws, &lounge), ["this was a mistake", "this stays"]);

    ws.retract(&taken_back).unwrap();
    assert_eq!(excerpts(&ws, &lounge), ["this stays"]);
    ws.rebuild_index().unwrap();
    assert_eq!(
        excerpts(&ws, &lounge),
        ["this stays"],
        "the files hold the retraction: a rebuild takes the row back again"
    );
    let held = rows_of(&ws);
    assert!(
        held.iter().all(|row| !row.6.contains("this was a mistake")),
        "no row carries the text: {held:?}"
    );
}

/// A reaction taken back is no message: the feed is left alone.
#[test]
fn a_retracted_reaction_leaves_the_feed_alone() {
    let (_dir, ws) = ws();
    let lounge = channel(&ws, "lounge");
    let said = post(&ws, &lounge, "well done");
    let reaction = ws.react(&said, "👍", None).unwrap();
    ws.retract(&reaction).unwrap();
    assert_eq!(excerpts(&ws, &lounge), ["well done"]);
}

/// A retraction that stands before its message in the file — a peer's,
/// which overtook it — still takes the message back when the index is
/// rebuilt: retractions are applied after every message.
#[test]
fn a_retraction_before_its_message_in_the_log_still_takes_it_back_on_a_rebuild() {
    let (_dir, ws) = ws();
    let lounge = channel(&ws, "lounge");
    let taken_back = post(&ws, &lounge, "never mind");
    ws.retract(&taken_back).unwrap();
    let log = ws.paths().conversation_log(&lounge).unwrap();
    let lines: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 2, "the message, then its retraction");
    std::fs::write(&log, format!("{}\n{}\n", lines[1], lines[0])).unwrap();

    ws.rebuild_index().unwrap();
    assert!(excerpts(&ws, &lounge).is_empty());
    let messages = ws.messages(&lounge, None, 10).unwrap();
    assert!(messages.iter().all(|m| m.retracted), "{messages:?}");
}

/// A rebuild reproduces the feed row for row — every field of every row,
/// in the same order — and not only its kinds.
#[test]
fn a_rebuilt_feed_is_the_live_feed_row_for_row() {
    let (dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("Ship it")).unwrap().id;
    ws.append_journal(
        &Home::Goal { goal },
        JournalPayload::Note {
            text: "kept".into(),
        },
        ws.owner_keys(),
        None,
    )
    .unwrap();
    let lounge = channel(&ws, "lounge");
    post(&ws, &lounge, "still here");
    for at in [7, 9] {
        ws.record_activity(
            &engine_fact(at, ActivityConcept::Node, "paused", ActivitySource::node()),
            true,
        )
        .unwrap();
    }
    let live = rows_of(&ws);
    assert!(live.len() >= 5, "{live:?}");
    drop(ws);

    let ws = reopened(&dir);
    ws.rebuild_index().unwrap();
    let rebuilt = rows_of(&ws);
    let sorted = |rows: &[_]| {
        let mut rows = rows.to_vec();
        rows.sort();
        rows
    };
    assert_eq!(sorted(&rebuilt), sorted(&live), "every row, every field");
    let seconds = |rows: &[FeedRow]| rows.iter().map(|row| row.0).collect::<Vec<_>>();
    assert_eq!(seconds(&rebuilt), seconds(&live), "newest first, as it was");
    ws.rebuild_index().unwrap();
    assert_eq!(rows_of(&ws), rebuilt, "two rebuilds write the same feed");
}

/// The feed says each row once it can be read — a journal fact's, a
/// message's, an engine fact's — on a channel of its own, and a rebuild is
/// one word: *rebuilt*, never a notice a row.
#[test]
fn the_feed_says_each_row_once_it_can_be_read_and_a_rebuild_says_so_once() {
    use bisa_store::ActivityNotice;
    let (_dir, ws) = ws();
    let mut heard = ws.subscribe_activity();
    let mut drain = |ws: &Workspace| -> Vec<ActivityNotice> {
        let mut out = Vec::new();
        while let Ok(notice) = heard.try_recv() {
            if let ActivityNotice::Row { seq, .. } = &notice {
                let page = ws.activity_page(None, None, 500).unwrap();
                assert!(
                    page.iter().any(|r| r.seq == *seq),
                    "a notice names a row that can be read: {notice:?}"
                );
            }
            out.push(notice);
        }
        out
    };

    let goal = ws.create_goal(NewGoal::captured("Ship it")).unwrap().id;
    let lounge = channel(&ws, "lounge");
    post(&ws, &lounge, "hello");
    ws.record_activity(
        &engine_fact(7, ActivityConcept::Node, "paused", ActivitySource::node()),
        true,
    )
    .unwrap();
    let said = drain(&ws);
    let of = |kind: &str| {
        said.iter()
            .filter_map(|n| match n {
                ActivityNotice::Row {
                    kind: k,
                    concept,
                    source,
                    ..
                } if k == kind => Some((*concept, source.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        of("note"),
        [(
            ActivityConcept::Goals,
            ActivitySource::new(ActivitySourceKind::Goal, goal.to_string())
        )],
        "the capture's note: {said:?}"
    );
    assert_eq!(
        of("message"),
        [(
            ActivityConcept::Channels,
            ActivitySource::new(ActivitySourceKind::Channel, lounge.clone())
        )]
    );
    assert_eq!(
        of("paused"),
        [(ActivityConcept::Node, ActivitySource::node())]
    );
    assert_eq!(said.len(), ws.activity_page(None, None, 500).unwrap().len());

    ws.rebuild_index().unwrap();
    assert_eq!(
        drain(&ws),
        [ActivityNotice::Rebuilt],
        "the rows a rebuild writes are no news"
    );
}

/// One writer puts a row in the feed (`Workspace::feed_row_in`): a seam
/// that inserted on its own would land a row nobody is told of.
#[test]
fn nothing_but_the_one_writer_inserts_a_feed_row() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut callers = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "index.rs" || !name.ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        for (n, line) in text.lines().enumerate() {
            if line.contains("idx.record_activity(") || line.contains("idx().record_activity(") {
                callers.push(format!("{name}:{}", n + 1));
            }
        }
    }
    assert_eq!(
        callers.len(),
        1,
        "the index's insert is called from: {callers:?}"
    );
    assert!(callers[0].starts_with("workspace.rs:"), "{callers:?}");
}

/// A retraction whose message the file no longer holds costs the rebuild
/// nothing: it is said and passed over, and the rest of the scope is read.
#[test]
fn a_retraction_of_a_message_the_file_lost_is_passed_over_by_a_rebuild() {
    let (_dir, ws) = ws();
    let lounge = channel(&ws, "lounge");
    let gone = post(&ws, &lounge, "cut from the file");
    ws.retract(&gone).unwrap();
    post(&ws, &lounge, "still told");
    let log = ws.paths().conversation_log(&lounge).unwrap();
    let kept: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .filter(|line| !line.contains("cut from the file"))
        .map(str::to_string)
        .collect();
    assert_eq!(kept.len(), 2, "the retraction and the later message");
    std::fs::write(&log, format!("{}\n", kept.join("\n"))).unwrap();

    ws.rebuild_index().unwrap();
    assert_eq!(excerpts(&ws, &lounge), ["still told"]);
}
