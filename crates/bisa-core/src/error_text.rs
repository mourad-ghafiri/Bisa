//! How the domain's refusals are said to a person: one `Text` per variant,
//! the message `error-core-…` of `locales/en/errors.ftl` ([17](../../../docs/architecture/17-internationalisation.md)).
//! Generated from each variant's `#[error]` string and held to the catalog by
//! `crates/bisa-i18n/tests/it/catalog.rs`; the `Display` stays the developer's English.

use crate::text::{Localize, Text};

impl Localize for crate::addon::AddonError {
    fn text(&self) -> Text {
        use crate::addon::AddonError as E;
        match self {
            E::EntryMissing(v0) => {
                crate::text!("error-core-addon-entry-missing", v0 = v0.to_string())
            }
            E::ReservedFile(v0) => {
                crate::text!("error-core-addon-reserved-file", v0 = v0.to_string())
            }
            E::NotServedFile(v0) => {
                crate::text!("error-core-addon-not-served-file", v0 = v0.to_string())
            }
            E::BadBundlePath(v0) => {
                crate::text!("error-core-addon-bad-bundle-path", v0 = v0.to_string())
            }
            E::TooManyFiles { found } => crate::text!(
                "error-core-addon-too-many-files",
                found = found.to_string(),
                max = crate::addon::MAX_ADDON_FILES.to_string()
            ),
            E::FileTooLarge { name, bytes } => crate::text!(
                "error-core-addon-file-too-large",
                name = name.to_string(),
                bytes = bytes.to_string(),
                max = crate::addon::MAX_ADDON_FILE_BYTES.to_string()
            ),
            E::TooLarge { bytes } => crate::text!(
                "error-core-addon-too-large",
                bytes = bytes.to_string(),
                max = crate::addon::MAX_ADDON_BYTES.to_string()
            ),
            E::GrantNotDeclared(v0) => {
                crate::text!("error-core-addon-grant-not-declared", v0 = v0.to_string())
            }
            E::NotEnabled(v0) => {
                crate::text!("error-core-addon-not-enabled", v0 = v0.to_string())
            }
            E::FilesAbsent(v0) => {
                crate::text!("error-core-addon-files-absent", v0 = v0.to_string())
            }
            E::NotGranted { id, word } => crate::text!(
                "error-core-addon-not-granted",
                id = id.to_string(),
                word = word.to_string()
            ),
        }
    }
}

impl Localize for crate::agent::AgentError {
    fn text(&self) -> Text {
        match self {
            crate::agent::AgentError::EmptyName => crate::text!("error-core-agent-empty-name"),
            crate::agent::AgentError::EmptyHarness => {
                crate::text!("error-core-agent-empty-harness")
            }
            crate::agent::AgentError::CoreIdReserved => {
                crate::text!("error-core-agent-core-id-reserved")
            }
            crate::agent::AgentError::DecisionMakingAgentIdReserved => {
                crate::text!("error-core-agent-decision-making-agent-id-reserved")
            }
            crate::agent::AgentError::CoreCannotBeDisabled => {
                crate::text!("error-core-agent-core-cannot-be-disabled")
            }
            crate::agent::AgentError::CoreCannotBeRemoved => {
                crate::text!("error-core-agent-core-cannot-be-removed")
            }
            crate::agent::AgentError::CoreNameFixed => {
                crate::text!("error-core-agent-core-name-fixed")
            }
            crate::agent::AgentError::CoreFieldFixed(v0) => {
                crate::text!("error-core-agent-core-field-fixed", v0 = v0.to_string())
            }
            crate::agent::AgentError::Immutable(v0) => {
                crate::text!("error-core-agent-immutable", v0 = v0.to_string())
            }
        }
    }
}

impl Localize for crate::ask::AnswerError {
    fn text(&self) -> Text {
        match self {
            crate::ask::AnswerError::Empty => crate::text!("error-core-answer-empty"),
            crate::ask::AnswerError::SelectionOnDecision => {
                crate::text!("error-core-answer-selection-on-decision")
            }
            crate::ask::AnswerError::UnsureOnDecision => {
                crate::text!("error-core-answer-unsure-on-decision")
            }
            crate::ask::AnswerError::UnknownOption(v0) => {
                crate::text!("error-core-answer-unknown-option", v0 = format!("{v0:?}"))
            }
            crate::ask::AnswerError::MultiNotOffered => {
                crate::text!("error-core-answer-multi-not-offered")
            }
            crate::ask::AnswerError::ManyRecommended => {
                crate::text!("error-core-answer-many-recommended")
            }
            crate::ask::AnswerError::EmptyOptionId => {
                crate::text!("error-core-answer-empty-option-id")
            }
            crate::ask::AnswerError::DuplicateOptionId(v0) => crate::text!(
                "error-core-answer-duplicate-option-id",
                v0 = format!("{v0:?}")
            ),
        }
    }
}

impl Localize for crate::board::BoardError {
    fn text(&self) -> Text {
        match self {
            crate::board::BoardError::UnknownColumn(v0) => {
                crate::text!("error-core-board-unknown-column", v0 = format!("{v0:?}"))
            }
            crate::board::BoardError::BadDate(v0) => {
                crate::text!("error-core-board-bad-date", v0 = format!("{v0:?}"))
            }
        }
    }
}

impl Localize for crate::channel::ChannelError {
    fn text(&self) -> Text {
        match self {
            crate::channel::ChannelError::Permanent { id, .. } => {
                crate::text!("error-core-channel-permanent", id = id.to_string())
            }
            crate::channel::ChannelError::GeneralShape => {
                crate::text!("error-core-channel-general-shape")
            }
            crate::channel::ChannelError::EveryoneReserved { id, .. } => {
                crate::text!("error-core-channel-everyone-reserved", id = id.to_string())
            }
            crate::channel::ChannelError::CoreReserved { id, .. } => {
                crate::text!("error-core-channel-core-reserved", id = id.to_string())
            }
            crate::channel::ChannelError::DirectNeedsAudience => {
                crate::text!("error-core-channel-direct-needs-audience")
            }
        }
    }
}

impl Localize for crate::conversation::ConversationError {
    fn text(&self) -> Text {
        match self {
            crate::conversation::ConversationError::BlankTitle => {
                crate::text!("error-core-conversation-blank-title")
            }
            crate::conversation::ConversationError::TitleTooLong(v0) => crate::text!(
                "error-core-conversation-title-too-long",
                max = (crate::conversation::MAX_TITLE_CHARS).to_string(),
                v0 = *v0
            ),
        }
    }
}

impl Localize for crate::decision::DecisionContractError {
    fn text(&self) -> Text {
        match self {
            crate::decision::DecisionContractError::NoQuestions => {
                crate::text!("error-core-decision-contract-no-questions")
            }
            crate::decision::DecisionContractError::TooLarge { bytes, .. } => crate::text!(
                "error-core-decision-contract-too-large",
                bytes = *bytes,
                max = (crate::decision::MAX_REQUEST_BYTES).to_string()
            ),
            crate::decision::DecisionContractError::EmptyQuestionId => {
                crate::text!("error-core-decision-contract-empty-question-id")
            }
            crate::decision::DecisionContractError::EmptyInstructions(v0) => crate::text!(
                "error-core-decision-contract-empty-instructions",
                v0 = v0.to_string()
            ),
            crate::decision::DecisionContractError::OptionCount {
                question, found, ..
            } => crate::text!(
                "error-core-decision-contract-option-count",
                question = question.to_string(),
                found = *found
            ),
            crate::decision::DecisionContractError::EmptyOption(v0) => crate::text!(
                "error-core-decision-contract-empty-option",
                v0 = v0.to_string()
            ),
            crate::decision::DecisionContractError::LevelCount {
                question, found, ..
            } => crate::text!(
                "error-core-decision-contract-level-count",
                question = question.to_string(),
                found = *found
            ),
            crate::decision::DecisionContractError::EmptyModel => {
                crate::text!("error-core-decision-contract-empty-model")
            }
            crate::decision::DecisionContractError::Unanswered(v0) => crate::text!(
                "error-core-decision-contract-unanswered",
                v0 = v0.to_string()
            ),
            crate::decision::DecisionContractError::UnaskedAnswer(v0) => crate::text!(
                "error-core-decision-contract-unasked-answer",
                v0 = v0.to_string()
            ),
            crate::decision::DecisionContractError::WrongType {
                question,
                asked,
                answered,
                ..
            } => crate::text!(
                "error-core-decision-contract-wrong-type",
                question = question.to_string(),
                asked = asked.to_string(),
                answered = answered.to_string()
            ),
            crate::decision::DecisionContractError::UnknownChoice {
                question, choice, ..
            } => crate::text!(
                "error-core-decision-contract-unknown-choice",
                question = question.to_string(),
                choice = choice.to_string()
            ),
            crate::decision::DecisionContractError::UnknownOutcome {
                question, outcome, ..
            } => crate::text!(
                "error-core-decision-contract-unknown-outcome",
                question = question.to_string(),
                outcome = outcome.to_string()
            ),
            crate::decision::DecisionContractError::ChoiceWithoutProbability(v0) => crate::text!(
                "error-core-decision-contract-choice-without-probability",
                v0 = v0.to_string()
            ),
            crate::decision::DecisionContractError::LegendMismatch {
                question,
                levels,
                found,
                ..
            } => crate::text!(
                "error-core-decision-contract-legend-mismatch",
                question = question.to_string(),
                found = *found,
                levels = *levels
            ),
            crate::decision::DecisionContractError::ScoreOffTheLegend {
                question, score, ..
            } => crate::text!(
                "error-core-decision-contract-score-off-the-legend",
                question = question.to_string(),
                score = *score
            ),
            crate::decision::DecisionContractError::OutOfRange {
                question,
                field,
                value,
                ..
            } => crate::text!(
                "error-core-decision-contract-out-of-range",
                question = question.to_string(),
                field = field.to_string(),
                value = *value
            ),
            crate::decision::DecisionContractError::NotADistribution { question, sum, .. } => {
                crate::text!(
                    "error-core-decision-contract-not-a-distribution",
                    question = question.to_string(),
                    sum = *sum
                )
            }
        }
    }
}

impl Localize for crate::error::CoreError {
    fn text(&self) -> Text {
        match self {
            crate::error::CoreError::InvalidPrincipal(v0) => {
                crate::text!("error-core-invalid-principal", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::Run(inner) => inner.text(),
            crate::error::CoreError::Template(inner) => inner.text(),
            crate::error::CoreError::WorkItem(inner) => inner.text(),
            crate::error::CoreError::Workstream(inner) => inner.text(),
            crate::error::CoreError::Channel(inner) => inner.text(),
            crate::error::CoreError::Agent(inner) => inner.text(),
            crate::error::CoreError::Team(inner) => inner.text(),
            crate::error::CoreError::Skill(inner) => inner.text(),
            crate::error::CoreError::Mcp(inner) => inner.text(),
            crate::error::CoreError::Note(inner) => inner.text(),
            crate::error::CoreError::Draw(inner) => inner.text(),
            crate::error::CoreError::Conversation(inner) => inner.text(),
            crate::error::CoreError::Pet(inner) => inner.text(),
            crate::error::CoreError::Addon(inner) => inner.text(),
            crate::error::CoreError::Member(inner) => inner.text(),
            crate::error::CoreError::ReviewNote(inner) => inner.text(),
            crate::error::CoreError::GitProfile(inner) => inner.text(),
            crate::error::CoreError::Settings(inner) => inner.text(),
            crate::error::CoreError::Answer(inner) => inner.text(),
            crate::error::CoreError::Decision(inner) => inner.text(),
            crate::error::CoreError::UnknownDecisionWord { what, value, .. } => crate::text!(
                "error-core-unknown-decision-word",
                what = what.to_string(),
                value = format!("{value:?}")
            ),
            crate::error::CoreError::UnknownEffort(v0) => {
                crate::text!("error-core-unknown-effort", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownKind(v0) => {
                crate::text!("error-core-unknown-kind", v0 = *v0)
            }
            crate::error::CoreError::UnknownGate(v0) => {
                crate::text!("error-core-unknown-gate", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownGoalStatus(v0) => {
                crate::text!("error-core-unknown-goal-status", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownHolder(v0) => {
                crate::text!("error-core-unknown-holder", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownScopeKind(v0) => {
                crate::text!("error-core-unknown-scope-kind", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::ThinkingTooLarge { bytes, .. } => crate::text!(
                "error-core-thinking-too-large",
                bytes = *bytes,
                max = (crate::message::MAX_THINKING_BYTES).to_string()
            ),
            crate::error::CoreError::TextTooLarge { bytes, .. } => crate::text!(
                "error-core-text-too-large",
                bytes = *bytes,
                max = (crate::message::MAX_TEXT_BYTES).to_string()
            ),
            crate::error::CoreError::UnknownFileScope(v0) => {
                crate::text!("error-core-unknown-file-scope", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownArtifactKind(v0) => {
                crate::text!("error-core-unknown-artifact-kind", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownActivityConcept(v0) => crate::text!(
                "error-core-unknown-activity-concept",
                v0 = format!("{v0:?}")
            ),
            crate::error::CoreError::TooManyArtifacts(v0) => crate::text!(
                "error-core-too-many-artifacts",
                max = (crate::artifact::MAX_ARTIFACTS_PER_MESSAGE).to_string(),
                v0 = *v0
            ),
            crate::error::CoreError::InvalidArtifactName(v0) => {
                crate::text!("error-core-invalid-artifact-name", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidArtifactTitle(v0) => {
                crate::text!("error-core-invalid-artifact-title", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::ArtifactTitleTooLong { bytes, .. } => crate::text!(
                "error-core-artifact-title-too-long",
                bytes = *bytes,
                max = (crate::artifact::MAX_ARTIFACT_TITLE_BYTES).to_string()
            ),
            crate::error::CoreError::ArtifactTooLarge { bytes, .. } => crate::text!(
                "error-core-artifact-too-large",
                bytes = *bytes,
                max = (crate::attachment::MAX_ATTACHMENT_BYTES).to_string()
            ),
            crate::error::CoreError::UnknownScope(v0) => {
                crate::text!("error-core-unknown-scope", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownGoalMode(v0) => {
                crate::text!("error-core-unknown-goal-mode", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidAssignee(v0) => {
                crate::text!("error-core-invalid-assignee", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidSlug(v0) => {
                crate::text!("error-core-invalid-slug", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidId { what, value, .. } => crate::text!(
                "error-core-invalid-id",
                what = what.to_string(),
                value = format!("{value:?}")
            ),
            crate::error::CoreError::InvalidWord { what, value, .. } => crate::text!(
                "error-core-invalid-word",
                what = what.to_string(),
                value = format!("{value:?}")
            ),
            crate::error::CoreError::InvalidBranch(v0) => {
                crate::text!("error-core-invalid-branch", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidRelPath(v0) => {
                crate::text!("error-core-invalid-rel-path", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidHash(v0) => {
                crate::text!("error-core-invalid-hash", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::InvalidRange { start, end, .. } => {
                crate::text!("error-core-invalid-range", start = *start, end = *end)
            }
            crate::error::CoreError::ContextTooLarge { bytes, .. } => crate::text!(
                "error-core-context-too-large",
                bytes = *bytes,
                a0 = (crate::message::MAX_CONTEXT_BYTES).to_string()
            ),
            crate::error::CoreError::InvalidTag(v0) => {
                crate::text!("error-core-invalid-tag", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::UnknownTagEntity(v0) => {
                crate::text!("error-core-unknown-tag-entity", v0 = format!("{v0:?}"))
            }
            crate::error::CoreError::TooManyTags(v0) => crate::text!(
                "error-core-too-many-tags",
                v0 = *v0,
                max = (crate::tags::MAX_TAGS).to_string()
            ),
        }
    }
}

impl Localize for crate::git_profile::GitProfileError {
    fn text(&self) -> Text {
        match self {
            crate::git_profile::GitProfileError::EmptyLabel => {
                crate::text!("error-core-git-profile-empty-label")
            }
            crate::git_profile::GitProfileError::Host(v0) => {
                crate::text!("error-core-git-profile-host", v0 = format!("{v0:?}"))
            }
            crate::git_profile::GitProfileError::Alias(v0) => {
                crate::text!("error-core-git-profile-alias", v0 = format!("{v0:?}"))
            }
            crate::git_profile::GitProfileError::Owner(v0) => {
                crate::text!("error-core-git-profile-owner", v0 = format!("{v0:?}"))
            }
            crate::git_profile::GitProfileError::Name(v0) => {
                crate::text!("error-core-git-profile-name", v0 = format!("{v0:?}"))
            }
            crate::git_profile::GitProfileError::Email(v0) => {
                crate::text!("error-core-git-profile-email", v0 = format!("{v0:?}"))
            }
            crate::git_profile::GitProfileError::SshKey(v0) => {
                crate::text!("error-core-git-profile-ssh-key", v0 = format!("{v0:?}"))
            }
            crate::git_profile::GitProfileError::Account(v0) => {
                crate::text!("error-core-git-profile-account", v0 = format!("{v0:?}"))
            }
        }
    }
}

impl Localize for crate::invite::ClaimRefusal {
    fn text(&self) -> Text {
        match self {
            crate::invite::ClaimRefusal::UnknownOrUsed => {
                crate::text!("error-core-claim-refusal-unknown-or-used")
            }
            crate::invite::ClaimRefusal::Expired => {
                crate::text!("error-core-claim-refusal-expired")
            }
            crate::invite::ClaimRefusal::Revoked => {
                crate::text!("error-core-claim-refusal-revoked")
            }
            crate::invite::ClaimRefusal::AwaitingHost => {
                crate::text!("error-core-claim-refusal-awaiting-host")
            }
        }
    }
}

impl Localize for crate::invite::InviteError {
    fn text(&self) -> Text {
        match self {
            crate::invite::InviteError::OwnerRole => crate::text!("error-core-invite-owner-role"),
            crate::invite::InviteError::NoLife => crate::text!("error-core-invite-no-life"),
        }
    }
}

impl Localize for crate::mcp::McpError {
    fn text(&self) -> Text {
        match self {
            crate::mcp::McpError::EmptyName => crate::text!("error-core-mcp-empty-name"),
            crate::mcp::McpError::ReservedName => crate::text!(
                "error-core-mcp-reserved-name",
                max = (crate::mcp::RESERVED_MCP_NAME).to_string()
            ),
            crate::mcp::McpError::EmptyCommand => crate::text!("error-core-mcp-empty-command"),
            crate::mcp::McpError::RelativeCwd(v0) => {
                crate::text!("error-core-mcp-relative-cwd", v0 = v0.to_string())
            }
            crate::mcp::McpError::InvalidUrl(v0) => {
                crate::text!("error-core-mcp-invalid-url", v0 = v0.to_string())
            }
            crate::mcp::McpError::BadHeaderName(v0) => {
                crate::text!("error-core-mcp-bad-header-name", v0 = v0.to_string())
            }
            crate::mcp::McpError::BadHeaderValue(v0) => {
                crate::text!("error-core-mcp-bad-header-value", v0 = v0.to_string())
            }
        }
    }
}

impl Localize for crate::member::MemberError {
    fn text(&self) -> Text {
        match self {
            crate::member::MemberError::OwnerImmutable => {
                crate::text!("error-core-member-owner-immutable")
            }
            crate::member::MemberError::SecondOwner => {
                crate::text!("error-core-member-second-owner")
            }
        }
    }
}

impl Localize for crate::note::NoteError {
    fn text(&self) -> Text {
        match self {
            crate::note::NoteError::EmptyTitle => crate::text!("error-core-note-empty-title"),
            crate::note::NoteError::TooLarge(v0) => crate::text!(
                "error-core-note-too-large",
                v0 = *v0,
                max = (crate::note::MAX_NOTE_BYTES).to_string()
            ),
            crate::note::NoteError::BadFrontMatter(v0) => {
                crate::text!("error-core-note-bad-front-matter", v0 = v0.to_string())
            }
        }
    }
}

impl Localize for crate::draw::DrawError {
    fn text(&self) -> Text {
        use crate::draw::DrawError as E;
        match self {
            E::EmptyTitle => crate::text!("error-core-draw-empty-title"),
            E::TitleTooLong { chars } => crate::text!(
                "error-core-draw-title-too-long",
                chars = *chars,
                max = crate::draw::MAX_DRAWING_TITLE_CHARS.to_string()
            ),
            E::TooLarge { bytes } => crate::text!(
                "error-core-draw-too-large",
                bytes = *bytes,
                max = crate::draw::MAX_SCENE_BYTES.to_string()
            ),
            E::TooManyElements { count } => crate::text!(
                "error-core-draw-too-many-elements",
                count = *count,
                max = crate::draw::MAX_DRAWING_ELEMENTS.to_string(),
                per_call = crate::draw::MAX_SKELETON_PER_CALL.to_string()
            ),
            E::ElementRefused { kind } => {
                crate::text!("error-core-draw-element-refused", kind = kind.to_string())
            }
            E::BadElement { why } => {
                crate::text!("error-core-draw-bad-element", why = why.to_string())
            }
        }
    }
}

impl Localize for crate::pet::PetError {
    fn text(&self) -> Text {
        match self {
            crate::pet::PetError::Animation { id, why, .. } => crate::text!(
                "error-core-pet-animation",
                id = id.to_string(),
                why = why.to_string()
            ),
        }
    }
}

impl Localize for crate::photo::PhotoRefusal {
    fn text(&self) -> Text {
        match self {
            crate::photo::PhotoRefusal::NotAHash { word, .. } => crate::text!(
                "error-core-photo-refusal-not-a-hash",
                word = word.to_string()
            ),
            crate::photo::PhotoRefusal::NotHeld { word, .. } => {
                crate::text!("error-core-photo-refusal-not-held", word = word.to_string())
            }
            crate::photo::PhotoRefusal::NotAPicture { word, .. } => crate::text!(
                "error-core-photo-refusal-not-a-picture",
                word = word.to_string()
            ),
            crate::photo::PhotoRefusal::TooLarge {
                word,
                bytes,
                max,
                edge,
                ..
            } => crate::text!(
                "error-core-photo-refusal-too-large",
                word = word.to_string(),
                bytes = *bytes,
                max = *max,
                edge = *edge
            ),
        }
    }
}

impl Localize for crate::review_note::ReviewNoteError {
    fn text(&self) -> Text {
        match self {
            crate::review_note::ReviewNoteError::EmptyBody => {
                crate::text!("error-core-review-note-empty-body")
            }
        }
    }
}

impl Localize for crate::run::RunError {
    fn text(&self) -> Text {
        match self {
            crate::run::RunError::NotStarted => crate::text!("error-core-run-not-started"),
            crate::run::RunError::NoStart => crate::text!("error-core-run-no-start"),
            crate::run::RunError::AmendChangesWorkflow { expected, got, .. } => crate::text!(
                "error-core-run-amend-changes-workflow",
                got = got.to_string(),
                expected = expected.to_string()
            ),
            crate::run::RunError::AmendNeedsInput(v0) => {
                crate::text!("error-core-run-amend-needs-input", v0 = v0.to_string())
            }
            crate::run::RunError::AlreadyStarted => crate::text!("error-core-run-already-started"),
            crate::run::RunError::Finished => crate::text!("error-core-run-finished"),
            crate::run::RunError::UnknownStep { step, .. } => {
                crate::text!("error-core-run-unknown-step", step = step.to_string())
            }
            crate::run::RunError::NotLive {
                step,
                event,
                state,
                expected,
                ..
            } => crate::text!(
                "error-core-run-not-live",
                event = event.to_string(),
                step = step.to_string(),
                state = state.to_string(),
                expected = expected.to_string()
            ),
            crate::run::RunError::WrongKind {
                step,
                event,
                kind,
                expected,
                ..
            } => crate::text!(
                "error-core-run-wrong-kind",
                event = event.to_string(),
                step = step.to_string(),
                kind = kind.to_string(),
                expected = expected.to_string()
            ),
            crate::run::RunError::BadAnswer { step, source, .. } => crate::text!(
                "error-core-run-bad-answer",
                step = step.to_string(),
                source = source.to_string()
            ),
            crate::run::RunError::AmendTouchesStartedStep { step, .. } => crate::text!(
                "error-core-run-amend-touches-started-step",
                step = step.to_string()
            ),
            crate::run::RunError::StaleBoundary { step, boundary, .. } => crate::text!(
                "error-core-run-stale-boundary",
                boundary = boundary.to_string(),
                step = step.to_string()
            ),
            crate::run::RunError::UnknownBoundary { step, boundary, .. } => crate::text!(
                "error-core-run-unknown-boundary",
                step = step.to_string(),
                boundary = boundary.to_string()
            ),
        }
    }
}

impl Localize for crate::settings::SettingsError {
    fn text(&self) -> Text {
        match self {
            crate::settings::SettingsError::UnknownKey(v0) => {
                crate::text!("error-core-settings-unknown-key", v0 = v0.to_string())
            }
            crate::settings::SettingsError::ScopeNotAllowed {
                key,
                scope,
                allowed,
                ..
            } => crate::text!(
                "error-core-settings-scope-not-allowed",
                key = key.to_string(),
                scope = scope.as_str().to_string(),
                allowed = allowed
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            crate::settings::SettingsError::InvalidValue { key, why, .. } => crate::text!(
                "error-core-settings-invalid-value",
                key = key.to_string(),
                why = why.to_string()
            ),
        }
    }
}

impl Localize for crate::skill::SkillError {
    fn text(&self) -> Text {
        match self {
            crate::skill::SkillError::EmptyName => crate::text!("error-core-skill-empty-name"),
            crate::skill::SkillError::EmptyDescription => {
                crate::text!("error-core-skill-empty-description")
            }
            crate::skill::SkillError::TooLarge(v0) => crate::text!(
                "error-core-skill-too-large",
                v0 = *v0,
                max = (crate::skill::MAX_SKILL_BYTES).to_string()
            ),
        }
    }
}

impl Localize for crate::team::TeamError {
    fn text(&self) -> Text {
        match self {
            crate::team::TeamError::EmptyName => crate::text!("error-core-team-empty-name"),
            crate::team::TeamError::NestedTeam(v0) => {
                crate::text!("error-core-team-nested-team", v0 = v0.to_string())
            }
            crate::team::TeamError::CoreAgentStored => {
                crate::text!("error-core-team-core-agent-stored")
            }
        }
    }
}

impl Localize for crate::template::TemplateError {
    fn text(&self) -> Text {
        match self {
            crate::template::TemplateError::Unbalanced { at, .. } => {
                crate::text!("error-core-template-unbalanced", at = *at)
            }
            crate::template::TemplateError::Unknown(v0) => {
                crate::text!("error-core-template-unknown", v0 = v0.to_string())
            }
            crate::template::TemplateError::EventOutsideMapping(v0) => crate::text!(
                "error-core-template-event-outside-mapping",
                v0 = v0.to_string()
            ),
            crate::template::TemplateError::UnknownInMapping(v0) => crate::text!(
                "error-core-template-unknown-in-mapping",
                v0 = v0.to_string()
            ),
            crate::template::TemplateError::UnknownInStartEvent(v0) => crate::text!(
                "error-core-template-unknown-in-start-event",
                v0 = v0.to_string()
            ),
            crate::template::TemplateError::UnknownInConnector(v0) => crate::text!(
                "error-core-template-unknown-in-connector",
                v0 = v0.to_string()
            ),
            crate::template::TemplateError::Unresolved { key, why, .. } => crate::text!(
                "error-core-template-unresolved",
                key = key.to_string(),
                why = why.to_string()
            ),
        }
    }
}

impl Localize for crate::workflow::InputError {
    fn text(&self) -> Text {
        match self {
            crate::workflow::InputError::WrongKind {
                input, want, got, ..
            } => crate::text!(
                "error-core-input-wrong-kind",
                input = input.to_string(),
                want = want.to_string(),
                got = got.to_string()
            ),
            crate::workflow::InputError::Missing { input, .. } => {
                crate::text!("error-core-input-missing", input = input.to_string())
            }
            crate::workflow::InputError::Unknown { inputs, .. } => crate::text!(
                "error-core-input-unknown",
                a0 = (inputs
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", "))
                .to_string()
            ),
        }
    }
}

impl Localize for crate::workitem::WorkItemError {
    fn text(&self) -> Text {
        match self {
            crate::workitem::WorkItemError::Illegal {
                from, transition, ..
            } => crate::text!(
                "error-core-work-item-illegal",
                transition = transition.to_string(),
                from = from.to_string()
            ),
            crate::workitem::WorkItemError::Terminal(v0) => {
                crate::text!("error-core-work-item-terminal", v0 = v0.to_string())
            }
        }
    }
}

impl Localize for crate::workstream::WorkstreamError {
    fn text(&self) -> Text {
        match self {
            crate::workstream::WorkstreamError::Illegal {
                from, transition, ..
            } => crate::text!(
                "error-core-workstream-illegal",
                transition = transition.to_string(),
                from = from.to_string()
            ),
            crate::workstream::WorkstreamError::Terminal => {
                crate::text!("error-core-workstream-terminal")
            }
            crate::workstream::WorkstreamError::Primary { transition, .. } => crate::text!(
                "error-core-workstream-primary",
                transition = transition.to_string()
            ),
            crate::workstream::WorkstreamError::NoRepository { id, .. } => {
                crate::text!("error-core-workstream-no-repository", id = id.to_string())
            }
            crate::workstream::WorkstreamError::NoOwnBranch { id, .. } => {
                crate::text!("error-core-workstream-no-own-branch", id = id.to_string())
            }
        }
    }
}
