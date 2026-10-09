//! Conversations: the records (kind 33415) of every exchange a person started
//! with agents — its origin, its title, whether it is archived.
//!
//! The domain type is [`bisa_core::Conversation`]. Truth is the signed
//! snapshot in `conversations/state/33415-<id>.json`; the messages are the
//! conversation's own log, `conversation/<id>.jsonl`, read and written by
//! [`crate::conversation`] like any other scope's. The `conversations` and
//! `conversation_agents` index tables are rebuildable, and carry beside the
//! record what a list wants to say: when it last moved, how much was said,
//! who took part, how much since the last summary.
//!
//! A conversation leaves with what it is about: a goal's, a workflow's, a
//! project's or a workstream's deletion calls [`Workspace::remove_conversations_of`],
//! so the record, the log and the rows go together.

use crate::error::StoreError;
use crate::index::ConversationRow;
use crate::paths::Paths;
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_CONVERSATION;
use bisa_core::{
    validate_conversation_title, AgentId, Conversation, ConversationId, ConversationMode,
    ConversationOrigin, ProjectId,
};

/// What a caller says to start one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewConversation {
    pub origin: ConversationOrigin,
    pub title: Option<String>,
    /// The mode it starts in — the caller's, resolved from
    /// `agents.conversation.mode` where the conversation stands.
    pub mode: ConversationMode,
}

/// Which conversations a listing is of. Every field narrows; none lists all.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ConversationFilter {
    /// One of [`ConversationOrigin::KINDS`].
    pub origin_kind: Option<String>,
    /// The record the origin names, with `origin_kind`.
    pub origin_id: Option<String>,
    /// Every conversation standing in one project — the project's own and
    /// every checkout's — what the Project IDE lists. Composes with the
    /// rest; a caller narrows by this or by an origin, not both.
    pub project: Option<ProjectId>,
    /// Those an agent spoke in or was addressed in.
    pub agent: Option<AgentId>,
    /// `Some(false)` is the live ones, `Some(true)` the archived, `None` both.
    pub archived: Option<bool>,
    /// Words to find in the messages (full text) or in the title.
    pub query: Option<String>,
    /// Page: only conversations that moved before this moment — and, when
    /// the cursor names the last row shown, before that row.
    pub before: Option<crate::conversation::PageBefore>,
    pub limit: usize,
}

impl ConversationFilter {
    /// A page of everything, newest first.
    pub fn all(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    /// A project's conversations, its checkouts' included.
    pub fn of_project(project: ProjectId, limit: usize) -> Self {
        Self {
            project: Some(project),
            limit,
            ..Default::default()
        }
    }

    /// One origin's conversations.
    pub fn of(origin: &ConversationOrigin, limit: usize) -> Self {
        Self {
            origin_kind: Some(origin.kind().to_string()),
            origin_id: origin.id(),
            limit,
            ..Default::default()
        }
    }
}

impl Workspace {
    /// The origin names something this workspace has; a workstream's names
    /// its own project.
    fn check_origin(&self, origin: &ConversationOrigin) -> Result<(), StoreError> {
        match origin {
            ConversationOrigin::Node | ConversationOrigin::Workspace => Ok(()),
            ConversationOrigin::Goal { id } => self.get_goal(*id).map(|_| ()),
            ConversationOrigin::Workflow { id } => self.get_workflow(*id).map(|_| ()),
            ConversationOrigin::Project { id } => self.get_project(*id).map(|_| ()),
            ConversationOrigin::Drawing { id } => self.get_drawing(*id).map(|_| ()),
            ConversationOrigin::Note { id } => self.get_note(*id).map(|_| ()),
            ConversationOrigin::Workstream { id, project } => {
                let w = self.get_workstream(*id)?;
                if w.project != *project {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-workstream-belongs-project-not",
                        id = id.to_string(),
                        a0 = (w.project).to_string(),
                        project = project.to_string()
                    )));
                }
                Ok(())
            }
        }
    }

    fn write_conversation(&self, def: &Conversation) -> Result<(), StoreError> {
        def.validate()?;
        let d = def.id.to_string();
        let existing_rev =
            self.snapshots
                .current_revision(Paths::NS_CONVERSATIONS, KIND_CONVERSATION, &d)?;
        let event = self.snapshots.put(
            Paths::NS_CONVERSATIONS,
            KIND_CONVERSATION,
            &d,
            def,
            existing_rev + 1,
            &self.owner,
            now_secs().max(existing_rev),
            None,
            &[],
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_CONVERSATION,
            d,
            event: event.clone(),
            audience: EventAudience::Workspace,
        });
        self.index_conversation(def, &event.pubkey.to_hex())
    }

    pub(crate) fn index_conversation(
        &self,
        def: &Conversation,
        author: &str,
    ) -> Result<(), StoreError> {
        self.idx().upsert_conversation(
            &def.id.to_string(),
            def.origin.kind(),
            def.origin.id().as_deref(),
            def.origin.project().map(|p| p.to_string()).as_deref(),
            def.title.as_deref(),
            author,
            def.created_at,
            def.archived,
            def.mode.as_str(),
        )
    }

    pub fn create_conversation(&self, new: NewConversation) -> Result<Conversation, StoreError> {
        self.check_origin(&new.origin)?;
        let def = Conversation {
            id: ConversationId::from_ulid(mint_ulid()),
            origin: new.origin,
            title: validate_conversation_title(new.title.as_deref())?,
            created_at: now_secs(),
            archived: false,
            mode: new.mode,
            mode_before_plan: None,
        };
        self.write_conversation(&def)?;
        Ok(def)
    }

    pub fn get_conversation(&self, id: ConversationId) -> Result<Conversation, StoreError> {
        self.snapshots
            .get::<Conversation>(Paths::NS_CONVERSATIONS, KIND_CONVERSATION, &id.to_string())?
            .map(|(c, _)| c)
            .ok_or_else(|| StoreError::ConversationNotFound(id.to_string()))
    }

    /// The conversation a message scope names, if it is one.
    pub fn conversation_of_scope(&self, scope: &str) -> Option<Conversation> {
        let id = scope.parse::<ConversationId>().ok()?;
        self.get_conversation(id).ok()
    }

    /// One conversation as a list row: the record with its facts.
    pub fn conversation_row(&self, id: ConversationId) -> Result<ConversationRow, StoreError> {
        self.idx()
            .get_conversation(&id.to_string())?
            .ok_or_else(|| StoreError::ConversationNotFound(id.to_string()))
    }

    /// Conversations a filter admits, the most recently moved first. A
    /// `query` finds words in the messages through the full-text index and
    /// in the titles; the two answers are one set.
    pub fn list_conversations(
        &self,
        filter: &ConversationFilter,
    ) -> Result<Vec<ConversationRow>, StoreError> {
        let idx = self.idx();
        let ids: Option<Vec<String>> = match filter.query.as_deref().map(str::trim) {
            Some(q) if !q.is_empty() => {
                let mut ids = idx.conversations_titled_like(q)?;
                // The full-text table files a message under its scope id;
                // a query the tokenizer refuses is simply no message hit.
                let hits = idx.search(&fts_query(q)).unwrap_or_default();
                for hit in hits {
                    if hit.parse::<ConversationId>().is_ok() && !ids.contains(&hit) {
                        ids.push(hit);
                    }
                }
                Some(ids)
            }
            _ => None,
        };
        idx.conversations_matching(
            filter.origin_kind.as_deref(),
            filter.origin_id.as_deref(),
            filter.project.map(|p| p.to_string()).as_deref(),
            filter.agent.as_ref().map(|a| a.as_str()),
            filter.archived,
            ids.as_deref(),
            filter.before.clone(),
            filter.limit.max(1),
        )
    }

    /// Give a conversation a title, or take it away (`None`).
    pub fn rename_conversation(
        &self,
        id: ConversationId,
        title: Option<&str>,
    ) -> Result<Conversation, StoreError> {
        let stored = self.get_conversation(id)?;
        let def = Conversation {
            title: validate_conversation_title(title)?,
            ..stored
        };
        self.write_conversation(&def)?;
        Ok(def)
    }

    pub fn set_conversation_archived(
        &self,
        id: ConversationId,
        archived: bool,
    ) -> Result<Conversation, StoreError> {
        let stored = self.get_conversation(id)?;
        if stored.archived == archived {
            return Ok(stored);
        }
        let def = Conversation { archived, ..stored };
        self.write_conversation(&def)?;
        Ok(def)
    }

    /// Move a conversation to another mode. The mode it left for a plan is
    /// remembered by the record itself ([`Conversation::set_mode`]).
    pub fn set_conversation_mode(
        &self,
        id: ConversationId,
        mode: ConversationMode,
    ) -> Result<Conversation, StoreError> {
        let mut def = self.get_conversation(id)?;
        if def.mode == mode {
            return Ok(def);
        }
        def.set_mode(mode);
        self.write_conversation(&def)?;
        Ok(def)
    }

    /// Remove a conversation: the record, its log and its rows, together.
    pub fn delete_conversation(&self, id: ConversationId) -> Result<(), StoreError> {
        self.get_conversation(id)?;
        let d = id.to_string();
        let log = self.paths.conversation_log(&d)?;
        match std::fs::remove_file(&log) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(log.display().to_string(), e)),
        }
        self.snapshots
            .delete_snapshot(Paths::NS_CONVERSATIONS, KIND_CONVERSATION, &d)?;
        self.remove_changes_of(id)?;
        let idx = self.idx();
        idx.in_transaction(|| {
            idx.delete_scope_facts(&d)?;
            idx.delete_conversation(&d)
        })
    }

    /// Every conversation about one thing goes with it — what a goal's, a
    /// workflow's, a project's, a workstream's, a note's or a drawing's
    /// deletion calls.
    pub(crate) fn remove_conversations_of(
        &self,
        origin_kind: &str,
        origin_id: &str,
    ) -> Result<(), StoreError> {
        let rows = self.idx().conversations_matching(
            Some(origin_kind),
            Some(origin_id),
            None,
            None,
            None,
            None,
            None,
            usize::MAX / 2,
        )?;
        for row in rows {
            let id = row.id.parse::<ConversationId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-id-index",
                    e = e.to_string()
                ))
            })?;
            self.delete_conversation(id)?;
        }
        Ok(())
    }

    /// Rebuild support: the `conversations` table from every snapshot. The
    /// facts beside the records are the messages' and land when those are
    /// replayed, after this.
    pub(crate) fn reindex_conversations(&self) -> Result<(), StoreError> {
        for d in self
            .snapshots
            .list_ds(Paths::NS_CONVERSATIONS, KIND_CONVERSATION)?
        {
            let read =
                self.snapshots
                    .get::<Conversation>(Paths::NS_CONVERSATIONS, KIND_CONVERSATION, &d);
            if let Some((def, ev)) =
                crate::workspace::tolerated("conversation", &d, read)?.flatten()
            {
                self.index_conversation(&def, &ev.pubkey.to_hex())?;
            }
        }
        Ok(())
    }
}

/// A person's words as an FTS5 query: each word quoted, so punctuation and
/// operators in what they typed are matched, never parsed.
fn fts_query(words: &str) -> String {
    words
        .split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_every_word_quoted_so_nothing_a_person_typed_is_an_operator() {
        assert_eq!(fts_query("dark mode"), "\"dark\" \"mode\"");
        assert_eq!(fts_query("  a  OR b "), "\"a\" \"OR\" \"b\"");
        assert_eq!(fts_query("say \"hi\""), "\"say\" \"\"\"hi\"\"\"");
        assert_eq!(fts_query(""), "");
    }

    // added by the coverage pass: conversations.rs

    // --- the bare lines of the conversations module ---

    /// A mode already held is kept as it is; a row that names no
    /// conversation refuses a removal by name; a snapshot that will not
    /// read is walked past by the rebuild; a log nobody may remove is an
    /// I/O error by its path.
    #[test]
    fn a_mode_held_is_kept_and_bad_rows_and_torn_snapshots_are_said() {
        let dir = tempfile::tempdir().unwrap();
        let ws = crate::workspace::Workspace::open_with_keystore(
            dir.path(),
            Box::new(crate::identity::MemoryKeyStore::default()),
        )
        .unwrap();
        let project = ws
            .create_project(crate::projects::NewProject::managed("web").unwrap())
            .unwrap();
        let origin = ConversationOrigin::Workstream {
            id: bisa_core::WorkstreamId::from_ulid(project.id.0),
            project: project.id,
        };
        let new = |title: &str| NewConversation {
            origin: origin.clone(),
            title: Some(title.into()),
            mode: ConversationMode::Auto,
        };
        let one = ws.create_conversation(new("one")).unwrap();
        let same = ws
            .set_conversation_mode(one.id, ConversationMode::Auto)
            .unwrap();
        assert_eq!(same, one);
        let two = ws.create_conversation(new("two")).unwrap();
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE conversations SET id = 'not-a-conversation' WHERE id = '{}'",
                two.id
            ))
            .unwrap();
        let err = ws
            .remove_conversations_of(origin.kind(), &origin.id().unwrap_or_default())
            .unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        let snapshot = ws
            .paths
            .state_dir(Paths::NS_CONVERSATIONS)
            .join(format!("{KIND_CONVERSATION}-{}.json", two.id));
        std::fs::write(&snapshot, b"{torn").unwrap();
        ws.rebuild_index().unwrap();
        assert!(ws.get_conversation(one.id).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let log = ws.paths.conversation_log(&one.id.to_string()).unwrap();
            std::fs::create_dir_all(log.parent().unwrap()).unwrap();
            std::fs::write(&log, b"").unwrap();
            let folder = log.parent().unwrap().to_path_buf();
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
            let unremovable = ws.delete_conversation(one.id);
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                matches!(unremovable, Err(StoreError::Io { .. })),
                "{unremovable:?}"
            );
        }
    }
}
