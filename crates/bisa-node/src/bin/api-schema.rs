//! Emit the node's wire contract as one JSON Schema bundle.
//!
//! `just gen-types` runs this and feeds the output to
//! `json-schema-to-typescript`, producing `desktop/src/types.gen.ts`. The
//! desktop never hand-mirrors a Rust type that appears here — drift between
//! the API and the UI becomes a build failure instead of a runtime surprise.
//!
//! Only types deriving `schemars::JsonSchema` can be bundled: the node's own
//! DTOs, `bisa-core`, and `bisa-harness`'s model-plan types (which
//! derive it because the agent DTOs carry them). The rest of
//! `bisa-store`, `bisa-engine` and `bisa-harness` do not, so
//! their TS shapes live in the clearly-marked `desktop/src/types.hand.ts`.
//! Adding the derive to those crates would move them into this bundle
//! unchanged.

use bisa_node::dto;
use schemars::generate::SchemaSettings;
use schemars::SchemaGenerator;
use serde_json::{json, Map, Value};

/// The bundle under construction: **one** generator for every type, so a
/// definition is made once and every `$ref` names the type it means.
///
/// Every type has a schema name of its own. Two that share one — a generic
/// instantiated several ways, two enums called the same in two crates — are
/// told apart by a number the generator appends, and a number moves when the
/// order of the bundle does: the name a screen imported would come to mean
/// another type. [`Bundle::finish`] refuses a numbered name; the type is
/// given its own with `#[schemars(rename = …)]` (`{T}` names a parameter).
struct Bundle {
    generator: SchemaGenerator,
    /// The names the desktop imports, each a `$ref` into the definitions.
    named: Map<String, Value>,
}

impl Bundle {
    fn new() -> Self {
        Self {
            generator: SchemaSettings::draft07().into_generator(),
            named: Map::new(),
        }
    }

    /// Register `T` under `name`: its definition and everything it references
    /// land in the shared definitions; `name` becomes a `$ref` to it.
    fn add<T: schemars::JsonSchema>(&mut self, name: &str) {
        let schema = self.generator.subschema_for::<T>();
        self.named.insert(name.to_string(), schema.to_value());
    }

    /// Every definition, under the names the desktop imports — and none of
    /// them a name the generator had to number.
    fn finish(self) -> Map<String, Value> {
        let defs = self.renamed();
        let shared = numbered(defs.keys().map(String::as_str));
        assert!(
            shared.is_empty(),
            "two types share a schema name, and the generator numbered one: {shared:?} \
             — name each with #[schemars(rename = …)]"
        );
        defs
    }

    /// The definitions with every name of ours in place. A name of ours that
    /// schemars already used is the definition itself. A name schemars
    /// spelled otherwise (`SettingDef` for `SettingDefDto`, `CatalogKind` for
    /// `CatalogKindDto`) **renames** the definition — the entry moves and every
    /// `$ref` to the old spelling is rewritten — so the generated TypeScript
    /// exports exactly our name and nothing else.
    fn renamed(mut self) -> Map<String, Value> {
        let mut defs: Map<String, Value> =
            self.generator.take_definitions(true).into_iter().collect();
        let mut renames: Vec<(String, String)> = Vec::new();
        for (name, schema) in self.named {
            if defs.contains_key(&name) {
                continue;
            }
            let target = schema
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/definitions/"))
                .map(str::to_string);
            match target {
                Some(old) if defs.contains_key(&old) => {
                    let moved = defs.remove(&old).expect("checked above");
                    defs.insert(name.clone(), moved);
                    renames.push((old, name));
                }
                _ => {
                    defs.insert(name, schema);
                }
            }
        }
        if renames.is_empty() {
            return defs;
        }
        let mut text = serde_json::to_string(&defs).expect("definitions serialize");
        for (old, new) in &renames {
            text = text.replace(
                &format!("\"#/definitions/{old}\""),
                &format!("\"#/definitions/{new}\""),
            );
        }
        serde_json::from_str(&text).expect("definitions deserialize")
    }
}

/// The names a generator numbered: `Name2` beside `Name`. A name that ends in
/// digits of its own (`Sha256`) has no sibling without them and is not one.
fn numbered<'a>(names: impl Iterator<Item = &'a str> + Clone) -> Vec<&'a str> {
    let all: std::collections::BTreeSet<&str> = names.clone().collect();
    names
        .filter(|name| {
            let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
            base.len() < name.len() && all.contains(base)
        })
        .collect()
}

fn main() {
    let mut bundle = Bundle::new();
    macro_rules! add {
        ($t:ty, $name:literal) => {
            bundle.add::<$t>($name)
        };
    }

    // --- core domain ---
    add!(bisa_core::Goal, "Goal");
    add!(bisa_core::AttachmentRef, "AttachmentRef");
    add!(bisa_node::dto::GoalDocumentRow, "GoalDocumentRow");
    add!(bisa_core::Archived, "Archived");
    add!(bisa_engine::retire::Fate, "Fate");
    add!(bisa_engine::retire::GoalPlan, "GoalPlan");
    add!(bisa_engine::retire::WorkflowPlan, "WorkflowPlan");
    add!(bisa_engine::retire::ProjectFacts, "ProjectFacts");
    add!(bisa_engine::retire::Retired, "Retired");
    add!(bisa_node::dto::RetirementDto, "Retirement");
    add!(bisa_node::dto::ArchiveBody, "ArchiveBody");
    add!(bisa_node::dto::InitRepositoryBody, "InitRepositoryBody");
    add!(bisa_node::dto::InitRepositoryReply, "InitRepositoryReply");
    add!(bisa_node::dto::GoalDocumentsBody, "GoalDocumentsBody");
    add!(bisa_core::GoalStatus, "GoalStatus");
    add!(bisa_core::GoalOrigin, "GoalOrigin");
    add!(bisa_core::Holder, "Holder");
    add!(bisa_core::WorkflowOrigin, "WorkflowOrigin");
    add!(bisa_core::ProjectOrigin, "ProjectOrigin");
    add!(bisa_core::Closure, "Closure");
    add!(bisa_core::goal::Budget, "Budget");
    add!(bisa_core::goal::BudgetSpent, "BudgetSpent");
    add!(bisa_core::goal::GoalEdge, "GoalEdge");
    // --- the workflow and the run ---
    add!(bisa_core::Workflow, "Workflow");
    add!(bisa_core::InputDef, "InputDef");
    add!(bisa_core::InputKind, "InputKind");
    add!(bisa_core::Step, "Step");
    add!(bisa_core::StepKind, "StepKind");
    add!(bisa_core::Flow, "Flow");
    add!(bisa_core::Join, "Join");
    add!(bisa_core::OnFail, "OnFail");
    add!(bisa_core::CheckKind, "CheckKind");
    add!(bisa_core::Rule, "Rule");
    add!(bisa_core::Condition, "Condition");
    add!(bisa_core::WaitFor, "WaitFor");
    add!(bisa_core::RunOutcome, "RunOutcome");
    add!(bisa_core::Problem, "Problem");
    add!(bisa_core::ProblemKind, "ProblemKind");
    add!(bisa_core::WorkflowRun, "WorkflowRun");
    add!(bisa_core::RunScope, "RunScope");
    add!(bisa_core::Home, "Home");
    add!(bisa_core::StepRecord, "StepRecord");
    add!(bisa_core::StepState, "StepState");
    add!(bisa_core::RunStatus, "RunStatus");
    add!(bisa_core::CancelCause, "CancelCause");
    add!(bisa_core::RunEvent, "RunEvent");
    add!(bisa_core::event::StepFact, "StepFact");
    add!(bisa_core::event::RunFact, "RunFact");
    add!(bisa_core::ResolvedSetting, "ResolvedSetting");
    add!(bisa_core::SettingScope, "SettingScope");
    add!(bisa_node::dto::GitIdentityView, "GitIdentityView");
    add!(bisa_node::dto::GitIdent, "GitIdent");
    add!(bisa_node::dto::CommitterView, "CommitterView");
    add!(bisa_node::dto::HarnessUsage, "HarnessUsage");
    add!(bisa_node::dto::LogsView, "LogsView");
    add!(bisa_node::dto::LogFileView, "LogFile");
    add!(bisa_node::dto::CrashReportView, "CrashReportView");
    add!(bisa_node::dto::FolderRepo, "FolderRepo");
    add!(bisa_node::dto::RepoCommitRow, "RepoCommitRow");
    add!(bisa_node::dto::RepoCommitBody, "RepoCommitBody");
    add!(bisa_node::dto::RepoRemoteBody, "RepoRemoteBody");
    add!(bisa_node::dto::RepoIdentityBody, "RepoIdentityBody");
    add!(bisa_node::dto::PullOutcomeBody, "PullOutcomeBody");
    add!(bisa_harness::UsageState, "UsageState");
    add!(bisa_harness::UsageReport, "UsageReport");
    add!(bisa_harness::UsageWindow, "UsageWindow");
    add!(bisa_node::dto::PendingCommitterView, "PendingCommitterView");
    add!(bisa_node::dto::GitConfigView, "GitConfigView");
    add!(bisa_node::dto::GitConfigKeyDto, "GitConfigKey");
    add!(bisa_node::dto::GitConfigEntryDto, "GitConfigEntry");
    add!(bisa_node::dto::GitConfigWrite, "GitConfigWrite");
    // --- connectors ---
    add!(bisa_core::Connector, "Connector");
    add!(bisa_core::AuthScheme, "AuthScheme");
    add!(bisa_core::KeyPlace, "KeyPlace");
    add!(bisa_core::HttpMethod, "HttpMethod");
    add!(bisa_core::Operation, "ConnectorOperationDef");
    add!(bisa_core::ParamDef, "ConnectorParamDef");
    add!(bisa_core::ParamKind, "ConnectorParamKind");
    add!(bisa_core::OutputSpec, "ConnectorOutputSpec");
    add!(bisa_core::SecretField, "SecretField");
    add!(bisa_node::dto::ConnectorRow, "ConnectorRow");
    add!(bisa_node::dto::ConnectorOperationRow, "ConnectorOperation");
    add!(bisa_node::dto::ConnectorDetailDto, "ConnectorDetail");
    add!(bisa_node::dto::ConnectorAccountRow, "ConnectorAccountRow");
    add!(bisa_node::dto::ConnectorOauthFacts, "ConnectorOauthFacts");
    add!(bisa_node::dto::SecretSourceDto, "SecretSource");
    add!(
        bisa_node::dto::NewConnectorAccountBody,
        "NewConnectorAccount"
    );
    add!(bisa_node::dto::OauthCompleteBody, "OauthComplete");
    add!(
        bisa_node::dto::ConnectorDefinitionBody,
        "ConnectorDefinition"
    );
    add!(bisa_node::dto::ConnectorProblemDto, "ConnectorProblem");
    add!(
        bisa_node::dto::ConnectorValidationDto,
        "ConnectorValidation"
    );
    add!(bisa_engine::connectors::OAuthStart, "OAuthStart");
    add!(bisa_engine::connectors::AccountCheck, "AccountCheck");
    add!(
        bisa_engine::connectors::AccountCheckState,
        "AccountCheckState"
    );
    add!(bisa_engine::codehost::AccountsView, "AccountsView");
    add!(bisa_engine::codehost::CodeHostKind, "CodeHostKind");
    add!(bisa_engine::codehost::CodeHostHealth, "CodeHostHealth");
    add!(bisa_engine::codehost::CliProbe, "CliProbe");
    add!(bisa_engine::codehost::CliProgram, "CliProgram");
    add!(bisa_engine::codehost::LoginPlan, "LoginPlan");
    add!(bisa_engine::codehost::RemoteInspection, "RemoteInspection");
    // --- mobile development (ide/19) ---
    add!(
        bisa_engine::mobile_development::MobileDevelopmentStatus,
        "MobileDevelopmentStatus"
    );
    add!(
        bisa_engine::mobile_development::Toolchain,
        "MobileToolchain"
    );
    add!(bisa_engine::mobile_development::Device, "MobileDevice");
    add!(
        bisa_engine::mobile_development::MobileDevelopmentShot,
        "MobileDevelopmentShot"
    );
    add!(
        bisa_engine::mobile_development::MobileDevelopmentProject,
        "MobileDevelopmentProject"
    );
    add!(
        bisa_engine::mobile_development::MobileDevelopmentRunCommand,
        "MobileDevelopmentRunCommand"
    );
    add!(
        bisa_engine::mobile_development::MobileDevelopmentChange,
        "MobileDevelopmentChange"
    );
    add!(bisa_node::dto::CreateSimulatorBody, "CreateSimulator");
    add!(bisa_engine::codehost::AccountChoice, "AccountChoice");
    add!(bisa_node::dto::InspectBody, "Inspect");
    add!(bisa_engine::codehost::Connection, "CodeHostConnection");
    add!(bisa_engine::codehost::RepoAccess, "RepoAccess");
    add!(bisa_engine::codehost::RemoteProtocol, "RemoteProtocol");
    add!(
        bisa_engine::ide::connection::RepoConnection,
        "RepoConnection"
    );
    add!(
        bisa_engine::ide::connection::ConnectionCheck,
        "ConnectionCheck"
    );
    add!(bisa_engine::gitprofiles::GitProfilesView, "GitProfilesView");
    add!(bisa_engine::gitprofiles::GitProfileView, "GitProfileView");
    add!(bisa_core::GitProfile, "GitProfile");
    add!(bisa_core::ProfileSpec, "ProfileSpec");
    add!(bisa_engine::ssh::SshOverview, "SshOverview");
    add!(bisa_engine::ssh::PublicKey, "SshPublicKey");
    add!(bisa_engine::ssh::HostGreeting, "HostGreeting");
    add!(bisa_engine::ssh::Resolved, "SshResolved");
    add!(bisa_engine::ssh::NewKey, "SshNewKey");
    add!(bisa_node::dto::AddAccountBody, "AddAccount");
    add!(bisa_node::dto::DefaultAccountBody, "DefaultAccount");
    add!(bisa_node::dto::AccountPinBody, "AccountPin");
    add!(bisa_node::dto::SshTestBody, "SshTest");
    add!(bisa_node::dto::SettingDefDto, "SettingDef");
    add!(bisa_node::dto::SettingsWriteBody, "SettingsWrite");
    add!(bisa_core::ProxyMode, "ProxyMode");
    add!(bisa_engine::network::NetworkStatus, "NetworkStatus");
    add!(bisa_engine::network::NetworkCheck, "NetworkCheck");
    add!(bisa_node::dto::NetworkCheckBody, "NetworkCheckRequest");
    add!(bisa_node::dto::IdeFileDto, "IdeFile");
    add!(bisa_node::dto::FileSides, "FileSides");
    add!(bisa_node::dto::CommitFileDiff, "CommitFileDiff");
    add!(bisa_node::dto::CommitFileSides, "CommitFileSides");
    add!(bisa_node::dto::GitConflict, "GitConflict");
    add!(bisa_node::dto::GitConflictKind, "GitConflictKind");
    add!(bisa_node::dto::GitOperationFacts, "GitOperationFacts");
    add!(bisa_node::dto::GitSideRef, "GitSideRef");
    add!(bisa_node::dto::GitSideRole, "GitSideRole");
    add!(bisa_node::dto::GitStep, "GitStep");
    add!(bisa_node::dto::GitMergePreview, "GitMergePreview");
    add!(bisa_node::dto::IdeWriteBody, "IdeWriteFile");
    add!(bisa_node::dto::IdeCreateBody, "IdeCreateEntry");
    add!(bisa_node::dto::IdeMoveBody, "IdeMoveEntry");
    add!(bisa_node::dto::IdeDeleteBody, "IdeDeleteEntries");
    add!(bisa_node::dto::IdeDeleteEntry, "IdeDeleteEntry");
    add!(bisa_node::dto::HunkBody, "GitHunkApply");
    add!(bisa_node::dto::CheckoutBody, "GitCheckout");
    add!(bisa_node::dto::BranchCreateBody, "GitBranchCreate");
    add!(bisa_node::dto::TagCreateBody, "GitTagCreate");
    add!(bisa_node::dto::RemoteAddBody, "GitRemoteAdd");
    add!(bisa_node::dto::UpstreamBody, "GitBranchUpstream");
    add!(bisa_node::dto::RebaseBody, "GitRebase");
    add!(bisa_node::dto::RebasePlanBody, "GitRebasePlan");
    add!(bisa_node::dto::RebaseStepBody, "GitRebaseStep");
    add!(bisa_node::dto::RebaseActionBody, "GitRebaseAction");
    add!(bisa_node::dto::MergeBody, "GitMerge");
    add!(bisa_node::dto::MergeModeBody, "GitMergeMode");
    add!(bisa_node::dto::CherryPickBody, "GitCherryPick");
    add!(bisa_node::dto::RevertBody, "GitRevert");
    add!(bisa_node::dto::GitAmendBody, "GitAmend");
    add!(bisa_node::dto::OperationBody, "GitOperation");
    add!(bisa_node::dto::ResolveBody, "GitResolve");
    add!(bisa_node::dto::ResolutionBody, "GitResolution");
    add!(bisa_node::dto::DiscardBody, "GitDiscard");
    add!(bisa_node::dto::RestoreBody, "GitRestore");
    add!(bisa_node::dto::RecoveryRefView, "GitRecoveryRef");
    add!(bisa_node::dto::RecoveryKindDto, "GitRecoveryKind");
    add!(bisa_node::dto::StashEntryView, "GitStash");
    add!(bisa_node::dto::StashPushBody, "GitStashPush");
    add!(bisa_node::dto::StashTargetBody, "GitStashTarget");
    add!(bisa_node::dto::PrReviewBody, "PrReview");
    add!(bisa_node::dto::LspDocumentBody, "LspDocument");
    add!(bisa_node::dto::LspRequestBody, "LspRequest");
    add!(bisa_node::dto::PrMergeBody, "PrMerge");
    // --- security ---
    add!(bisa_engine::security::SecurityStatus, "SecurityStatus");
    add!(bisa_engine::security::Decision, "GuardDecision");
    add!(bisa_engine::security::HarnessGuard, "HarnessGuard");
    add!(
        bisa_engine::security::ClassifierSettings,
        "ClassifierSettings"
    );
    add!(bisa_engine::security::OnHarmful, "OnHarmful");
    add!(bisa_engine::security::RedactRule, "RedactRule");
    add!(bisa_engine::security::Detector, "RedactDetector");
    add!(bisa_engine::security::Origin, "RuleOrigin");
    add!(bisa_engine::security::GuardRule, "GuardRule");
    add!(bisa_engine::security::Matcher, "GuardMatcher");
    add!(bisa_engine::security::Action, "GuardAction");
    add!(bisa_engine::security::PolicyProblem, "PolicyProblem");
    add!(bisa_engine::security::Feature, "SecurityFeature");
    add!(bisa_core::event::GuardVerdict, "GuardVerdict");
    add!(bisa_core::event::GuardJudge, "GuardJudge");
    add!(bisa_engine::security::RedactPreview, "RedactPreview");
    add!(bisa_engine::security::GuardPreview, "GuardPreview");
    add!(bisa_engine::security::GuardReply, "GuardReply");
    add!(bisa_engine::changes::ChangesView, "ChangesView");
    add!(bisa_engine::changes::FileReviewView, "FileReviewView");
    add!(bisa_engine::changes::settle::Settled, "ChangesSettled");
    add!(bisa_engine::changes::asks::AskView, "ConversationAsk");
    add!(
        bisa_engine::changes::asks::AskSubject,
        "ConversationAskSubject"
    );
    add!(bisa_engine::content::ContentVerdict, "ContentVerdict");
    add!(
        bisa_engine::changes::asks::AskAnswer,
        "ConversationAskAnswer"
    );
    add!(bisa_engine::decider::DeciderStatus, "DeciderStatus");
    add!(bisa_engine::readiness::Readiness, "Readiness");
    add!(bisa_engine::readiness::Check, "ReadinessCheck");
    add!(bisa_engine::readiness::CheckId, "ReadinessCheckId");
    add!(bisa_engine::readiness::CheckState, "ReadinessCheckState");
    add!(bisa_engine::readiness::Door, "ReadinessDoor");
    add!(bisa_engine::readiness::Fix, "ReadinessFix");
    add!(bisa_harness::install::InstallHint, "InstallHint");
    add!(bisa_harness::install::PlatformCommand, "PlatformCommand");
    add!(bisa_harness::install::Platform, "InstallPlatform");
    add!(bisa_engine::decider::JudgementRecord, "JudgementRecord");
    add!(bisa_core::DecisionRequest, "DecisionRequest");
    add!(bisa_core::DecisionResponse, "DecisionResponse");
    add!(bisa_node::dto::DecisionKeyBody, "DecisionKeyRequest");
    add!(bisa_node::dto::RedactPreviewBody, "RedactPreviewRequest");
    add!(bisa_node::dto::GuardPreviewBody, "GuardPreviewRequest");
    add!(bisa_node::dto::GuardHookBody, "GuardHookRequest");
    add!(bisa_node::dto::ResolveBody, "GitResolve");
    add!(
        bisa_engine::codehost::CodeHostCapabilities,
        "CodeHostCapabilities"
    );
    add!(
        bisa_node::dto::WorkstreamScriptsView,
        "WorkstreamScriptsView"
    );
    add!(bisa_engine::scripts::ScriptStatus, "ScriptStatus");
    add!(bisa_engine::scripts::Phase, "ScriptPhase");
    add!(bisa_engine::scripts::RunCommand, "RunCommand");
    add!(bisa_node::dto::ServeBody, "ServeFolder");
    add!(bisa_node::ide::serve::ServedView, "ServedFolder");
    add!(bisa_node::ide::serve::ServedOwner, "ServedOwner");
    add!(bisa_engine::browser::BrowserAction, "BrowserAction");
    add!(bisa_engine::browser::BrowserRequest, "BrowserRequest");
    add!(bisa_engine::browser::BrowserScope, "BrowserScope");
    add!(bisa_engine::browser::BrowserHome, "BrowserHome");
    add!(bisa_engine::browser::BrowserHomeScope, "BrowserHomeScope");
    add!(bisa_engine::browser::ReadFormat, "BrowserReadFormat");
    add!(bisa_engine::browser::WaitUntil, "BrowserWaitUntil");
    add!(bisa_engine::browser::BrowserDialog, "BrowserDialog");
    add!(bisa_engine::browser::ConsoleLine, "BrowserConsoleLine");
    add!(
        bisa_engine::browser::ScrollPosition,
        "BrowserScrollPosition"
    );
    add!(bisa_engine::browser::BrowserTab, "BrowserTab");
    add!(bisa_engine::browser::BrowserResult, "BrowserResult");
    add!(
        bisa_engine::browser::PendingBrowserRequest,
        "BrowserPending"
    );
    add!(bisa_engine::codehost::PullRequest, "PullRequest");
    add!(bisa_engine::codehost::CheckRun, "CheckRun");
    add!(bisa_engine::codehost::MergeStrategy, "MergeStrategy");
    add!(bisa_engine::codehost::RepoRef, "RepoRef");
    // PR reviews & resolvable threads.
    add!(bisa_engine::codehost::ReviewSummary, "ReviewSummary");
    add!(bisa_engine::codehost::ReviewThread, "ReviewThread");
    add!(
        bisa_engine::codehost::ReviewThreadComment,
        "ReviewThreadComment"
    );
    add!(bisa_node::dto::PrThreadResolveBody, "PrThreadResolve");
    add!(bisa_node::dto::PrThreadReplyBody, "PrThreadReply");
    add!(bisa_node::dto::ReviewNoteBody, "ReviewNoteCreate");
    add!(bisa_node::dto::ReviewNoteEditBody, "ReviewNoteEdit");
    add!(bisa_node::dto::ReviewSendBody, "ReviewNotesSend");
    add!(bisa_core::ReviewNote, "ReviewNote");
    add!(bisa_core::DiffScope, "DiffScope");
    add!(bisa_core::LineRange, "LineRange");
    add!(bisa_core::ClosureReason, "ClosureReason");
    add!(bisa_core::Gate, "Gate");
    add!(bisa_core::AskKind, "AskKind");
    add!(bisa_core::AskOption, "AskOption");
    add!(bisa_core::Answer, "Answer");
    add!(bisa_core::Attachment, "Attachment");
    add!(bisa_core::Channel, "Channel");
    add!(bisa_core::RosterPolicy, "RosterPolicy");
    add!(bisa_core::Member, "Member");
    add!(bisa_core::MessageBody, "MessageBody");
    add!(bisa_core::ContextRef, "ContextRef");
    add!(bisa_core::ArtifactRef, "ArtifactRef");
    add!(bisa_core::ArtifactKind, "ArtifactKind");
    add!(bisa_core::ArtifactSource, "ArtifactSource");
    add!(dto::NamedFileBody, "NamedFileBody");
    add!(bisa_core::Agent, "Agent");
    add!(bisa_core::Team, "Team");
    add!(bisa_core::workitem::WorkItemSpec, "WorkItemSpec");
    add!(bisa_core::workitem::WorkItemState, "WorkItemState");
    add!(bisa_core::ToolTier, "ToolTier");
    add!(bisa_core::Assignee, "Assignee");
    add!(bisa_core::Project, "Project");
    add!(bisa_core::ProjectRoot, "ProjectRoot");
    add!(bisa_core::Vcs, "Vcs");
    add!(bisa_core::project::CodeHost, "CodeHost");
    add!(bisa_core::PublishPolicy, "PublishPolicy");
    add!(bisa_core::Workstream, "Workstream");
    add!(bisa_core::WorkstreamKind, "WorkstreamKind");
    add!(bisa_core::WorkstreamState, "WorkstreamState");
    add!(bisa_core::BoardColumn, "BoardColumn");
    add!(bisa_core::DueDate, "DueDate");
    add!(bisa_core::WorkstreamBoard, "WorkstreamBoard");
    // --- events: what a run begins on, waits for and is diverted by ---
    add!(bisa_core::StartOn, "StartOn");
    add!(bisa_core::Schedule, "Schedule");
    add!(bisa_core::FireOn, "FireOn");
    add!(bisa_core::Guard, "Guard");
    add!(bisa_core::Overlap, "Overlap");
    add!(bisa_core::MessageFilter, "MessageFilter");
    add!(bisa_core::MessageFrom, "MessageFrom");
    add!(bisa_core::SignalFilter, "SignalFilter");
    add!(bisa_core::ProjectFilter, "ProjectFilter");
    add!(bisa_core::ProjectChange, "ProjectChange");
    add!(bisa_core::RunFilter, "RunFilter");
    add!(bisa_core::RunEnd, "RunEnd");
    add!(bisa_core::PlatformFilter, "PlatformFilter");
    add!(bisa_core::Boundary, "Boundary");
    add!(bisa_core::BoundaryOn, "BoundaryOn");
    add!(bisa_core::BoundaryAct, "BoundaryAct");
    add!(bisa_core::Pick, "Pick");
    add!(bisa_core::Finish, "Finish");
    add!(bisa_core::Listening, "Listening");
    add!(bisa_core::Paused, "Paused");
    add!(bisa_core::PauseReason, "PauseReason");
    add!(bisa_core::ListenerHost, "ListenerHost");
    add!(bisa_core::ListenerKey, "ListenerKey");
    add!(bisa_core::Signal, "Signal");
    add!(bisa_core::SignalSource, "SignalSource");
    add!(bisa_core::SignalScope, "SignalScope");
    add!(bisa_core::Chain, "Chain");
    add!(bisa_core::Fired, "Fired");
    add!(bisa_core::Tags, "Tags");
    add!(bisa_core::tags::TagEntity, "TagEntity");
    add!(bisa_core::tags::TagMatch, "TagMatch");

    // --- harness: the model plan (shared by the agent DTOs and the store) ---
    add!(bisa_harness::ModelPlan, "ModelPlan");
    add!(bisa_harness::ModelChoice, "ModelChoice");
    add!(bisa_harness::ModelStrategy, "ModelStrategy");
    // Named, because the desktop imports them by name: a level a session
    // runs at, and what a plan, a model or a step asks for.
    add!(bisa_core::Effort, "Effort");
    add!(bisa_core::EffortChoice, "EffortChoice");

    // --- node DTOs: requests ---
    add!(dto::NewGoalBody, "NewGoalBody");
    add!(dto::NewWorkflowBody, "NewWorkflowBody");
    add!(dto::SetWorkflowBody, "SetWorkflowBody");
    add!(dto::PutWorkflowBody, "PutWorkflowBody");
    add!(dto::ErrorBody, "ErrorBody");
    add!(dto::StartRunBody, "StartRunBody");
    add!(dto::ReleaseStepBody, "ReleaseStepBody");
    add!(dto::NodeInfo, "NodeInfo");
    add!(dto::StopBody, "StopBody");
    add!(dto::CloseBody, "CloseBody");
    add!(dto::GovernanceBody, "GovernanceBody");
    add!(bisa_store::GatePolicy, "GatePolicy");
    add!(dto::TeamPatch, "TeamPatch");
    add!(dto::ReplaceBody, "IdeReplace");
    add!(dto::OpenInteractiveBody, "OpenInteractiveBody");
    add!(dto::ExitBody, "ExitBody");
    add!(dto::FetchBody, "GitFetch");
    add!(dto::PullBody, "GitPull");
    add!(dto::RemoteSetBody, "GitRemoteSet");
    add!(dto::LspPathBody, "LspPath");
    add!(dto::LspRestartBody, "LspRestart");
    add!(dto::StopOutcome, "StopOutcome");
    add!(dto::AmendBody, "AmendBody");
    add!(dto::StepAnswerBody, "StepAnswerBody");
    add!(dto::DecideBody, "DecideBody");
    add!(dto::AssigneesBody, "AssigneesBody");
    add!(dto::NewChannelBody, "NewChannelBody");
    add!(dto::PatchChannelBody, "PatchChannelBody");
    add!(dto::NewMessageBody, "NewMessageBody");
    add!(dto::NewConversationBody, "NewConversationBody");
    add!(dto::PatchConversationBody, "PatchConversationBody");
    add!(dto::SettleChangesBody, "SettleChangesBody");
    add!(dto::RestoreChangesBody, "RestoreChangesBody");
    add!(dto::ConversationView, "ConversationView");
    add!(dto::ConversationsResponse, "ConversationsResponse");
    add!(dto::LiveTurnView, "LiveTurnView");
    add!(dto::LiveTurnsResponse, "LiveTurnsResponse");
    add!(bisa_core::ConversationOrigin, "ConversationOrigin");
    add!(dto::OpenDmBody, "OpenDmBody");
    add!(dto::ReactBody, "ReactBody");
    add!(dto::ScopeBody, "ScopeBody");
    add!(dto::NewAgentBody, "NewAgentBody");
    add!(dto::PatchAgentBody, "PatchAgentBody");
    add!(dto::NewSkillBody, "NewSkillBody");
    add!(dto::PatchSkillBody, "PatchSkillBody");
    add!(dto::NewNoteBody, "NewNoteBody");
    add!(dto::NewPetBody, "NewPetBody");
    // --- addons (18 — Addons) ---
    add!(bisa_core::AddonManifest, "AddonManifest");
    add!(bisa_core::AddonWindow, "AddonWindow");
    add!(bisa_core::AddonFrame, "AddonFrame");
    add!(bisa_core::AddonDock, "AddonDock");
    add!(bisa_core::AddonPermission, "AddonPermission");
    add!(bisa_core::AddonProblem, "AddonProblem");
    add!(bisa_core::Origin, "Origin");
    add!(dto::AddonDto, "Addon");
    add!(dto::AddonsDto, "Addons");
    add!(dto::AddonProblemsDto, "AddonProblems");
    add!(dto::AddonOfferDto, "AddonOffer");
    add!(dto::AddonOffersDto, "AddonOffers");
    add!(dto::NewAddonBody, "NewAddonBody");
    add!(dto::ValidateAddonBody, "ValidateAddonBody");
    add!(dto::AddonPatchBody, "AddonPatchBody");
    add!(dto::AddonFetchBody, "AddonFetchBody");
    add!(bisa_engine::addons::AddonFetchResult, "AddonFetchResult");
    add!(bisa_engine::addons::AddonsChange, "AddonsChange");
    add!(dto::PatchNoteBody, "PatchNoteBody");
    add!(dto::OwnerScopeBody, "OwnerScopeBody");
    add!(dto::NoteRow, "NoteRow");
    // --- drawings (19 — Drawings) ---
    add!(bisa_core::OwnerScope, "OwnerScope");
    add!(bisa_core::Scene, "Scene");
    add!(bisa_core::SceneAppState, "SceneAppState");
    add!(bisa_core::DrawingSummary, "DrawingSummary");
    add!(dto::NewDrawingBody, "NewDrawingBody");
    add!(dto::PatchDrawingBody, "PatchDrawingBody");
    add!(dto::DrawingRow, "DrawingRow");
    add!(dto::DrawingDetail, "DrawingDetail");
    add!(bisa_engine::drawings::DrawAction, "DrawAction");
    add!(bisa_engine::drawings::DrawRequest, "DrawRequest");
    add!(bisa_engine::drawings::DrawScope, "DrawScope");
    add!(bisa_engine::drawings::DrawResult, "DrawResult");
    add!(bisa_engine::drawings::PendingDrawRequest, "DrawingPending");
    add!(bisa_harness::McpServerConfig, "McpServerConfig");
    add!(dto::NewMcpBody, "NewMcpBody");
    add!(dto::PatchMcpBody, "PatchMcpBody");
    add!(dto::ProbeMcpBody, "ProbeMcpBody");
    add!(dto::ProbeMcpByIdBody, "ProbeMcpByIdBody");
    add!(bisa_mcp_probe::McpProbeReport, "McpProbeReport");
    add!(bisa_engine::mcp_health::McpHealthView, "McpHealthView");
    add!(dto::McpServerView, "McpServerView");
    add!(dto::TeamBody, "TeamBody");
    add!(dto::PersonBody, "PersonBody");
    add!(dto::RoleBody, "RoleBody");
    add!(dto::PersonRow, "PersonRow");
    add!(dto::RoleRow, "RoleRow");
    add!(dto::PermissionRow, "PermissionRow");
    add!(dto::NewInviteBody, "NewInviteBody");
    add!(dto::JoinHostBody, "JoinHostBody");
    add!(dto::HostedPostBody, "HostedPostBody");
    add!(dto::HostedPosted, "HostedPosted");
    add!(dto::HostedChannelRow, "HostedChannelRow");
    add!(dto::RelayUrlBody, "RelayUrlBody");
    add!(dto::SyncReport, "SyncReport");
    add!(dto::JoinRequest, "JoinRequest");
    add!(bisa_core::MemberRole, "MemberRole");
    add!(bisa_core::Permission, "Permission");
    add!(bisa_core::WorkspaceMember, "WorkspaceMember");
    add!(bisa_node::dto::MeBody, "MeBody");
    add!(bisa_core::Invite, "Invite");
    add!(bisa_core::InviteState, "InviteState");
    add!(bisa_store::HeldMessage, "HeldMessage");
    add!(bisa_store::HeldReason, "HeldReason");
    add!(bisa_store::PeopleChange, "PeopleChange");
    add!(bisa_collab::RelayHealth, "RelayHealth");
    add!(bisa_collab::RelayCheck, "RelayCheck");
    add!(bisa_collab::Directory, "Directory");
    add!(bisa_collab::HostCard, "HostCard");
    add!(bisa_guest::Hosted, "Hosted");
    add!(bisa_guest::HostedState, "HostedState");
    add!(bisa_guest::HostedMessage, "HostedMessage");
    add!(bisa_guest::HostedReaction, "HostedReaction");
    add!(dto::NewProjectBody, "NewProjectBody");
    add!(dto::PatchProjectBody, "PatchProjectBody");
    add!(dto::ProjectAttachBody, "ProjectAttachBody");
    add!(dto::NewWorkstreamBody, "NewWorkstreamBody");
    add!(bisa_core::WorkstreamSource, "WorkstreamSource");
    add!(dto::PatchWorkstreamBody, "PatchWorkstreamBody");
    add!(dto::PlaceWorkstreamBody, "PlaceWorkstreamBody");
    add!(dto::CommitBody, "CommitBody");
    add!(dto::PrBody, "PrBody");
    add!(dto::StageBody, "StageBody");
    add!(dto::WorkstreamCommitBody, "WorkstreamCommitBody");
    add!(dto::ListeningBody, "ListeningBody");
    add!(dto::GoalListeningBody, "GoalListeningBody");
    add!(dto::EmitSignalBody, "EmitSignalBody");
    add!(dto::CatalogInstallBody, "CatalogInstallBody");

    // --- node DTOs: responses ---
    add!(dto::GuidanceInfo, "GuidanceInfo");
    add!(dto::WorkflowRow, "WorkflowRow");
    add!(dto::GoalRow, "GoalRow");
    add!(dto::RunStrip, "RunStrip");
    add!(dto::StripStep, "StripStep");
    add!(dto::RunSummary, "RunSummary");
    add!(dto::StartedBy, "StartedBy");
    add!(dto::EventKind, "EventKind");
    add!(dto::StartSummary, "StartSummary");
    add!(dto::ListenerView, "ListenerView");
    add!(dto::PublicHook, "PublicHook");
    add!(bisa_engine::HookSecret, "HookSecret");
    add!(dto::SignalView, "SignalView");
    add!(dto::SignalStateDto, "SignalState");
    add!(dto::GoalArmed, "GoalArmed");
    add!(dto::WorkflowRuns, "WorkflowRuns");
    add!(dto::RunView, "RunView");
    add!(dto::NeedsAction, "NeedsAction");
    add!(dto::Representative, "Representative");
    add!(dto::InboxRow, "InboxRow");
    add!(dto::ConversationKind, "ConversationKind");
    add!(dto::PulseRow, "PulseRow");
    add!(dto::PulsePage, "PulsePage");
    add!(dto::PulseCursor, "PulseCursor");
    // Named explicitly rather than left to hoisting, because the desktop
    // imports them by name: `PulseEvent` is what a Pulse row switches on, and
    // `JournalPayload` is what makes that switch exhaustive.
    add!(dto::PulseEvent, "PulseEvent");
    add!(dto::PulseMessage, "PulseMessage");
    add!(bisa_core::event::JournalPayload, "JournalPayload");
    add!(bisa_core::GuidancePhase, "GuidancePhase");
    add!(bisa_core::GuidanceStatus, "GuidanceStatus");
    add!(bisa_engine::guided::DesignStatus, "DesignStatus");
    // The session roster: what a rail row, the Agents pane and the pet read.
    add!(dto::SessionsResponse, "SessionsResponse");
    add!(bisa_engine::SessionPresence, "SessionRow");
    add!(bisa_engine::SessionState, "SessionState");
    add!(bisa_engine::WaitingOn, "WaitingOn");
    add!(bisa_engine::SubagentPresence, "SubagentPresence");
    add!(bisa_engine::ExecutionOutcome, "ExecutionOutcome");
    add!(bisa_harness::InputKind, "InputRequestKind");
    add!(bisa_engine::ide::files::Disposal, "Disposal");
    add!(dto::IdeCopyBody, "IdeCopyEntry");
    add!(dto::BranchRenameBody, "GitBranchRename");
    add!(dto::GitStatusInfo, "GitStatusInfo");
    add!(dto::ChangedFile, "ChangedFile");
    add!(dto::GitFileRow, "GitFileRow");
    add!(dto::ProjectRow, "ProjectRow");
    add!(dto::TagFacet, "TagFacet");
    add!(dto::TagEntityCount, "TagEntityCount");
    add!(dto::CatalogKindDto, "CatalogKind");
    add!(dto::CatalogEntryDto, "CatalogEntry");
    add!(dto::InstalledDto, "Installed");
    add!(dto::InstalledWorkflowDto, "InstalledWorkflow");
    add!(dto::ReferenceKindDto, "ReferenceKind");
    add!(dto::ReferenceDto, "Reference");
    add!(bisa_core::FileScope, "FileScope");
    add!(dto::FileEntryKindDto, "FileEntryKind");
    add!(dto::FileEntryDto, "FileEntry");
    add!(dto::FileTreeDto, "FileTree");
    add!(dto::FileContentDto, "FileContent");
    add!(dto::PlacementDto, "Placement");
    add!(dto::CacheStatsDto, "CacheStatsDto");

    let defs = bundle.finish();
    let bundle = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "BisaApi",
        "description": "Generated by `just gen-types` — do not edit by hand.",
        "type": "object",
        "properties": defs.keys().map(|k| {
            (k.clone(), json!({ "$ref": format!("#/definitions/{k}") }))
        }).collect::<Map<String, Value>>(),
        "definitions": defs,
    });

    // schemars emits `#/$defs/...`; json-schema-to-typescript resolves
    // `#/definitions/...` — normalize once here rather than post-processing
    // the generated TypeScript.
    let text = serde_json::to_string_pretty(&bundle)
        .expect("bundle serializes")
        .replace("#/$defs/", "#/definitions/");
    println!("{text}");
}

#[cfg(test)]
mod tests {
    use super::numbered;

    #[test]
    fn a_name_the_generator_numbered_is_found_and_a_name_with_digits_of_its_own_is_not() {
        let names = [
            "Platform",
            "Platform2",
            "Sha256",
            "ValueRef",
            "ValueRef12",
            "Step",
        ];
        assert_eq!(
            numbered(names.iter().copied()),
            ["Platform2", "ValueRef12"],
            "Sha256 has no sibling called Sha"
        );
        assert!(numbered(["Goal", "Sha256"].iter().copied()).is_empty());
    }
}
