//! Channels, direct channels and their messages (05 — Channels): what the
//! timeline promises at its edges — a page that loses nothing, a reaction
//! that is one however often it is clicked, a post that is words or nothing,
//! a bound on how long, a direct channel that needs somebody else and is
//! between its participants, a deleted channel whose words are kept aside.

use bisa_core::{
    ChannelKind, MessageBody, PrincipalId, RosterPolicy, Tags, MAX_TEXT_BYTES, MAX_THINKING_BYTES,
};
use bisa_store::{MemoryKeyStore, NewAgent, PageBefore, PostOrigin, Workspace};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn channel(ws: &Workspace, name: &str) -> String {
    let roster = RosterPolicy::Listed {
        agents: vec![],
        teams: vec![],
        humans: vec![],
    };
    ws.create_channel(name, None, roster, Tags::default())
        .unwrap()
        .id
        .to_string()
}

fn agent(ws: &Workspace, name: &str) -> bisa_core::AgentId {
    ws.add_agent(NewAgent {
        name: name.into(),
        harness: "mock".into(),
        system_prompt: "x".into(),
        ..Default::default()
    })
    .unwrap()
    .id
}

fn say(ws: &Workspace, scope: &str, text: &str) -> String {
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

#[test]
fn a_page_cut_inside_one_second_loses_nothing_and_reads_nothing_twice() {
    let (_d, ws) = ws();
    let scope = channel(&ws, "burst");
    // Twelve posts as fast as the store takes them: most share a second.
    let posted: Vec<String> = (0..12)
        .map(|n| say(&ws, &scope, &format!("m{n}")))
        .collect();
    let all = ws.messages(&scope, None, 100).unwrap();
    assert_eq!(all.len(), 12);
    assert!(
        all.windows(2).any(|w| w[0].created_at == w[1].created_at),
        "the test needs two posts within one second, and got none"
    );

    // Pages of five, each continuing from the oldest row shown.
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<PageBefore> = None;
    loop {
        let page = ws.messages(&scope, cursor.clone(), 5).unwrap();
        if page.is_empty() {
            break;
        }
        for m in page.iter().rev() {
            assert!(!seen.contains(&m.id), "{} was read twice", m.content);
            seen.push(m.id.clone());
        }
        let oldest = &page[0];
        cursor = Some(PageBefore::row(oldest.created_at, oldest.id.clone()));
    }
    let mut expected = posted.clone();
    expected.reverse();
    assert_eq!(seen, expected, "every message, newest first, exactly once");

    // The second alone, exclusive, is what a caller without an id gets — and
    // it is the caller's to know that a second's siblings are dropped.
    let first = ws.messages(&scope, None, 5).unwrap();
    let by_second = ws
        .messages(&scope, Some(PageBefore::at(first[0].created_at)), 100)
        .unwrap();
    assert!(by_second.iter().all(|m| m.created_at < first[0].created_at));
}

/// The same words said twice are two messages — a reminder that fires
/// again, a loop that posts for every item, a person who says "yes" twice.
/// What makes a message one is its own identity, never that nothing else
/// like it was said that second.
#[test]
fn the_same_words_said_twice_within_a_second_are_two_messages() {
    let (_d, ws) = ws();
    let scope = channel(&ws, "echo");
    let ids: Vec<String> = (0..6).map(|_| say(&ws, &scope, "still waiting")).collect();
    let all = ws.messages(&scope, None, 100).unwrap();
    let distinct: std::collections::BTreeSet<&String> = ids.iter().collect();
    assert_eq!(distinct.len(), 6, "each post has an id of its own: {ids:?}");
    assert_eq!(all.len(), 6, "and each is read back");
    assert!(
        all.windows(2).any(|w| w[0].created_at == w[1].created_at),
        "the test needs two posts within one second, and got none"
    );
}

#[test]
fn a_reaction_clicked_twice_is_one_reaction_and_the_one_it_answers_can_be_retracted() {
    let (_d, ws) = ws();
    let scope = channel(&ws, "cheers");
    let target = say(&ws, &scope, "shipped");
    let first = ws.react(&target, "🎉", None).unwrap();
    let again = ws.react(&target, "🎉", None).unwrap();
    assert_eq!(
        first, again,
        "the second click answers the standing reaction"
    );
    assert_eq!(ws.list_reactions(&scope).unwrap().len(), 1);
    // A different emoji is another reaction; an agent's is its own.
    let other = ws.react(&target, "👀", None).unwrap();
    assert_ne!(other, first);
    let agent = ws
        .add_agent(NewAgent {
            name: "Cheerful".into(),
            system_prompt: "cheer".into(),
            harness: "claude-code".into(),
            ..Default::default()
        })
        .unwrap();
    let theirs = ws.react(&target, "🎉", Some(&agent.id)).unwrap();
    assert_ne!(theirs, first);
    assert_eq!(ws.list_reactions(&scope).unwrap().len(), 3);

    ws.retract(&again).unwrap();
    let left = ws.list_reactions(&scope).unwrap();
    assert_eq!(left.len(), 2);
    assert!(left.iter().all(|r| r.id != first));
    // Retracted, the same click reacts afresh and the reaction stands again
    // — within one second the fresh event may be byte for byte the
    // retracted one, which is fine: what counts is what stands.
    let fresh = ws.react(&target, "🎉", None).unwrap();
    assert_eq!(ws.list_reactions(&scope).unwrap().len(), 3, "{fresh}");
    ws.retract(&fresh).unwrap();
    assert_eq!(ws.list_reactions(&scope).unwrap().len(), 2);
}

#[test]
fn a_post_is_words_or_nothing_and_never_longer_than_the_bound() {
    let (_d, ws) = ws();
    let scope = channel(&ws, "words");
    for blank in ["", "   ", "\n\t "] {
        let refused = ws
            .post_message(
                &scope,
                MessageBody::post(blank),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap_err();
        assert!(
            refused.to_string().contains("empty message"),
            "{blank:?}: {refused}"
        );
    }
    let longest = "x".repeat(MAX_TEXT_BYTES);
    say(&ws, &scope, &longest);
    let too_long = ws
        .post_message(
            &scope,
            MessageBody::post("x".repeat(MAX_TEXT_BYTES + 1)),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap_err();
    assert!(too_long.to_string().contains("too long"), "{too_long}");
    assert!(
        too_long.to_string().contains("attachment"),
        "and says the way: {too_long}"
    );
    // Words outlast the thinking behind them.
    const _: () = assert!(MAX_TEXT_BYTES > MAX_THINKING_BYTES);
    assert_eq!(ws.messages(&scope, None, 10).unwrap().len(), 1);
}

#[test]
fn a_direct_channel_needs_somebody_else_and_is_one_however_it_is_asked_for() {
    let (_d, ws) = ws();
    let me = ws.owner_principal();
    let alone = ws.open_dm(&[]).unwrap_err();
    assert!(alone.to_string().contains("at least one other"), "{alone}");
    let with_myself = ws.open_dm(&[me.clone(), me.clone()]).unwrap_err();
    assert!(
        with_myself.to_string().contains("at least one other"),
        "{with_myself}"
    );

    let agent = ws
        .add_agent(NewAgent {
            name: "Pal".into(),
            system_prompt: "chat".into(),
            harness: "claude-code".into(),
            ..Default::default()
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    assert_eq!(dm.kind, ChannelKind::Direct);
    let same = ws
        .open_dm(&[agent.pubkey.clone(), me.clone(), agent.pubkey.clone()])
        .unwrap();
    assert_eq!(
        same.id, dm.id,
        "me twice, them twice: the one direct channel"
    );
    let stranger = PrincipalId::new("f".repeat(64)).unwrap();
    assert!(
        ws.open_dm(&[stranger]).is_err(),
        "a stranger's key opens nothing"
    );
}

#[test]
fn a_deleted_channel_keeps_its_words_aside_and_answers_nothing_after() {
    let (_d, ws) = ws();
    let scope = channel(&ws, "ephemeral");
    say(&ws, &scope, "kept as history");
    let log = ws.paths().conversation_log(&scope).unwrap();
    assert!(log.is_file());
    let id = bisa_core::ChannelId::new(&scope).unwrap();
    let deletable = bisa_core::DeletableChannel::new(ws.get_channel(&id).unwrap()).unwrap();
    ws.delete_channel(deletable).unwrap();
    assert!(ws.get_channel(&id).is_err());
    // Its words are still on disk, under a name no scope has; the store
    // answers nothing by the id any more — no page, no search, no count.
    assert!(!log.exists());
    let aside: Vec<_> = std::fs::read_dir(ws.paths().conversation_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".deleted"))
        .collect();
    assert_eq!(aside.len(), 1);
    let kept = std::fs::read_to_string(aside[0].path()).unwrap();
    assert!(kept.contains("kept as history"), "{kept}");
    assert!(ws.messages(&scope, None, 10).unwrap().is_empty());
    assert!(ws.unread_counts().unwrap().is_empty());
    ws.rebuild_index().unwrap();
    assert!(ws.messages(&scope, None, 10).unwrap().is_empty());
}

/// A post is in its log or it is nowhere: a line the log would not take
/// leaves no row behind. Indexed first and written after, the refused post
/// showed in the timeline until a rebuild lost it — and posted again, stood
/// twice.
#[cfg(unix)]
#[test]
fn a_post_whose_log_cannot_be_written_leaves_no_row() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, ws) = ws();
    let scope = channel(&ws, "ledger");
    say(&ws, &scope, "the first line");
    let log = ws.paths().conversation_log(&scope).unwrap();
    std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o400)).unwrap();
    let refused = ws.post_message(
        &scope,
        MessageBody::post("the line the disk refused"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    );
    std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(refused.is_err(), "{refused:?}");
    let shown: Vec<String> = ws
        .messages(&scope, None, 10)
        .unwrap()
        .into_iter()
        .map(|m| m.content)
        .collect();
    assert_eq!(shown, vec!["the first line"]);
    say(&ws, &scope, "the line the disk refused");
    assert_eq!(ws.messages(&scope, None, 10).unwrap().len(), 2);
    ws.rebuild_index().unwrap();
    assert_eq!(ws.messages(&scope, None, 10).unwrap().len(), 2);
}

/// A direct channel is between its participants: a mention names one of
/// them, its handle is their agents, and whoever is outside it neither posts
/// nor reacts in it. Named from outside, an agent was woken on a
/// conversation that was not its own, and its answer was refused.
#[test]
fn a_mention_in_a_direct_channel_names_only_who_is_in_it() {
    let (_d, ws) = ws();
    let scout = agent(&ws, "Scout");
    let ranger = agent(&ws, "Ranger");
    let scout_key = ws.get_agent(&scout).unwrap().pubkey;
    let ranger_key = ws.get_agent(&ranger).unwrap().pubkey;
    let dm = ws.open_dm(std::slice::from_ref(&scout_key)).unwrap();
    let scope = dm.id.to_string();

    for outsider in [
        ranger.to_string(),
        ranger_key.as_hex().to_string(),
        "general-agent".to_string(),
    ] {
        let refused = ws
            .resolve_mentions(&scope, std::slice::from_ref(&outsider))
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("a direct channel is between its participants"),
            "{outsider}: {refused}"
        );
    }
    assert_eq!(
        ws.resolve_mentions(&scope, &[scout.to_string()]).unwrap(),
        vec![scout_key.clone()]
    );
    assert_eq!(
        ws.resolve_mentions(&scope, std::slice::from_ref(&scope))
            .unwrap(),
        vec![scout_key.clone()],
        "the handle of a direct channel is its participants' agents — no core agent beside them"
    );
    // A disabled participant is not in the handle.
    ws.set_agent_enabled(&scout, false).unwrap();
    assert!(ws
        .resolve_mentions(&scope, std::slice::from_ref(&scope))
        .unwrap()
        .is_empty());
    ws.set_agent_enabled(&scout, true).unwrap();

    // The store holds it for a caller that brings keys of its own.
    let named = ws.post_message(
        &scope,
        MessageBody::post("a word"),
        None,
        std::slice::from_ref(&ranger_key),
        &[],
        None,
        PostOrigin::Asked,
    );
    assert!(named.is_err(), "{named:?}");
    let said = say(&ws, &scope, "between us");
    assert!(ws.react(&said, "👀", Some(&ranger)).is_err());
    ws.react(&said, "👀", Some(&scout)).unwrap();
    ws.react(&said, "👍", None).unwrap();
    // A standing channel names whom it names.
    let room = channel(&ws, "open");
    assert_eq!(
        ws.resolve_mentions(&room, &[ranger.to_string()]).unwrap(),
        vec![ranger_key]
    );
}

/// A reaction has an id too, and is no parent for a reply: refused in the
/// words a reply to nothing is, where it was the cache's own error before.
#[test]
fn a_reply_to_a_reaction_is_refused_in_words() {
    let (_d, ws) = ws();
    let scope = channel(&ws, "thread");
    let said = say(&ws, &scope, "a message");
    let reaction = ws.react(&said, "👍", None).unwrap();
    let refused = ws
        .post_message(
            &scope,
            MessageBody::post("a reply to a thumb"),
            Some(reaction),
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("cannot reply"), "{refused}");
    ws.post_message(
        &scope,
        MessageBody::post("a reply to the message"),
        Some(said),
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
}

/// What a list says of a room is its latest live post — read for every room
/// in one query: a retracted post is nobody's last words, and a room nobody
/// wrote in yet has none.
#[test]
fn the_latest_live_post_of_every_room_is_read_in_one_query() {
    let (_d, ws) = ws();
    let design = channel(&ws, "design");
    let ops = channel(&ws, "ops");
    say(&ws, &design, "first");
    let second = say(&ws, &design, "second");
    let latest = ws.latest_messages().unwrap();
    assert_eq!(latest[&design].content, "second");
    assert_eq!(latest[&design].scope_id, design);
    assert!(
        !latest.contains_key(&ops),
        "nothing said in ops: no last words"
    );
    // One room asked for by itself answers by the same rule — what a frame
    // about that room carries, so no reader mirrors the rule.
    let of = |scope: &str| ws.latest_message(scope).unwrap().map(|m| m.content);
    assert_eq!(of(&design).as_deref(), Some("second"));
    assert_eq!(of(&ops), None);
    ws.retract(&second).unwrap();
    let latest = ws.latest_messages().unwrap();
    assert_eq!(
        latest[&design].content, "first",
        "a retracted post gives way to the one before it"
    );
    assert_eq!(of(&design).as_deref(), Some("first"));
    let first = latest[&design].id.clone();
    ws.retract(&first).unwrap();
    assert!(
        !ws.latest_messages().unwrap().contains_key(&design),
        "every post retracted: nothing to say"
    );
    assert_eq!(of(&design), None);
}
