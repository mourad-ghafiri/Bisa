//! How the engine's refusals — and those of the crates beneath it that carry
//! no `Text` of their own (git, the code hosts, the connectors, SSH, the mobile
//! tools, isolation, the language servers) — are said to a person: one `Text`
//! per variant, `error-<crate>-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

/// `CodeHostError` said to a person (codehost carries no `Text` of its own).
pub fn codehost_code_host_error(e: &bisa_codehost::CodeHostError) -> Text {
    match e {
        bisa_codehost::CodeHostError::NotAuthenticated(v0) => bisa_core::text!(
            "error-codehost-code-host-not-authenticated",
            v0 = v0.to_string()
        ),
        bisa_codehost::CodeHostError::NotFound(v0) => {
            bisa_core::text!("error-codehost-code-host-not-found", v0 = v0.to_string())
        }
        bisa_codehost::CodeHostError::Refused(v0) => {
            bisa_core::text!("error-codehost-code-host-refused", v0 = v0.to_string())
        }
        bisa_codehost::CodeHostError::Unsupported(v0) => {
            bisa_core::text!("error-codehost-code-host-unsupported", v0 = v0.to_string())
        }
        bisa_codehost::CodeHostError::Transport(v0) => {
            bisa_core::text!("error-codehost-code-host-transport", v0 = v0.to_string())
        }
    }
}

/// `ConnectorError` said to a person (connectors carries no `Text` of its own).
pub fn connector(e: &bisa_connectors::ConnectorError) -> Text {
    match e {
        bisa_connectors::ConnectorError::BadDefinition(v0) => bisa_core::text!(
            "error-connectors-connector-bad-definition",
            v0 = v0.to_string()
        ),
        bisa_connectors::ConnectorError::BadParam { name, why, .. } => bisa_core::text!(
            "error-connectors-connector-bad-param",
            name = name.to_string(),
            why = why.to_string()
        ),
        bisa_connectors::ConnectorError::Unresolved(v0) => {
            bisa_core::text!("error-connectors-connector-unresolved", v0 = v0.to_string())
        }
        bisa_connectors::ConnectorError::HostRefused { host, allowed, .. } => bisa_core::text!(
            "error-connectors-connector-host-refused",
            host = host.to_string(),
            allowed = allowed.to_string()
        ),
        bisa_connectors::ConnectorError::NotAuthenticated(v0) => bisa_core::text!(
            "error-connectors-connector-not-authenticated",
            v0 = v0.to_string()
        ),
        bisa_connectors::ConnectorError::NotFound(v0) => {
            bisa_core::text!("error-connectors-connector-not-found", v0 = v0.to_string())
        }
        bisa_connectors::ConnectorError::Refused { status, reason, .. } => bisa_core::text!(
            "error-connectors-connector-refused",
            status = *status,
            reason = reason.to_string()
        ),
        bisa_connectors::ConnectorError::RateLimited { retry_after, .. } => bisa_core::text!(
            "error-connectors-connector-rate-limited",
            retry_after = *retry_after
        ),
        bisa_connectors::ConnectorError::Upstream { status, reason, .. } => bisa_core::text!(
            "error-connectors-connector-upstream",
            status = *status,
            reason = reason.to_string()
        ),
        bisa_connectors::ConnectorError::Unreachable(v0) => bisa_core::text!(
            "error-connectors-connector-unreachable",
            v0 = v0.to_string()
        ),
        bisa_connectors::ConnectorError::Transport(v0) => {
            bisa_core::text!("error-connectors-connector-transport", v0 = v0.to_string())
        }
        bisa_connectors::ConnectorError::Timeout(v0) => {
            bisa_core::text!("error-connectors-connector-timeout", v0 = format!("{v0:?}"))
        }
        bisa_connectors::ConnectorError::Open {
            host, until_secs, ..
        } => bisa_core::text!(
            "error-connectors-connector-open",
            host = host.to_string(),
            a0 = (bisa_connectors::OPEN_AFTER).to_string(),
            until_secs = *until_secs
        ),
        bisa_connectors::ConnectorError::SelectMissing { path, .. } => bisa_core::text!(
            "error-connectors-connector-select-missing",
            path = path.to_string()
        ),
        bisa_connectors::ConnectorError::TooLarge(v0) => {
            bisa_core::text!("error-connectors-connector-too-large", v0 = *v0)
        }
        bisa_connectors::ConnectorError::OAuth(v0) => {
            bisa_core::text!("error-connectors-connector-o-auth", v0 = v0.to_string())
        }
        bisa_connectors::ConnectorError::Store(v0) => {
            bisa_core::text!("error-connectors-connector-store", v0 = v0.to_string())
        }
    }
}

impl Localize for crate::guided::DesignRefusal {
    fn text(&self) -> Text {
        match self {
            crate::guided::DesignRefusal::ManualGoal(v0) => bisa_core::text!(
                "error-engine-design-refusal-manual-goal",
                v0 = v0.to_string()
            ),
            crate::guided::DesignRefusal::Closed(v0) => {
                bisa_core::text!("error-engine-design-refusal-closed", v0 = v0.to_string())
            }
            crate::guided::DesignRefusal::HasWorkflow(v0) => bisa_core::text!(
                "error-engine-design-refusal-has-workflow",
                v0 = v0.to_string()
            ),
            crate::guided::DesignRefusal::HasRun(v0) => {
                bisa_core::text!("error-engine-design-refusal-has-run", v0 = v0.to_string())
            }
            crate::guided::DesignRefusal::Busy(v0) => {
                bisa_core::text!("error-engine-design-refusal-busy", v0 = v0.to_string())
            }
            crate::guided::DesignRefusal::DesignOff => {
                bisa_core::text!("error-engine-design-refusal-design-off")
            }
        }
    }
}

impl Localize for crate::interactive::InteractiveError {
    fn text(&self) -> Text {
        match self {
            crate::interactive::InteractiveError::UnknownHarness(v0) => bisa_core::text!(
                "error-engine-interactive-unknown-harness",
                v0 = format!("{v0:?}")
            ),
            crate::interactive::InteractiveError::NotInteractive(v0) => bisa_core::text!(
                "error-engine-interactive-not-interactive",
                v0 = v0.to_string()
            ),
            crate::interactive::InteractiveError::UnknownSession(v0) => bisa_core::text!(
                "error-engine-interactive-unknown-session",
                v0 = v0.to_string()
            ),
            crate::interactive::InteractiveError::BadSecret => {
                bisa_core::text!("error-engine-interactive-bad-secret")
            }
            crate::interactive::InteractiveError::Io(v0) => {
                bisa_core::text!("error-engine-interactive-io", v0 = v0.to_string())
            }
            crate::interactive::InteractiveError::Store(inner) => inner.text(),
        }
    }
}

impl Localize for crate::EngineError {
    fn text(&self) -> Text {
        match self {
            crate::EngineError::Store(inner) => inner.text(),
            crate::EngineError::UnknownGate(v0) => {
                bisa_core::text!("error-engine-unknown-gate", v0 = v0.to_string())
            }
            crate::EngineError::GateAlreadyDecided(v0) => {
                bisa_core::text!("error-engine-gate-already-decided", v0 = v0.to_string())
            }
            crate::EngineError::Io(v0) => bisa_core::text!("error-engine-io", v0 = v0.to_string()),
            crate::EngineError::Vcs(inner) => vcs(inner),
            crate::EngineError::CodeHost(inner) => codehost_code_host_error(inner),
            crate::EngineError::Connector(inner) => connector(inner),
            crate::EngineError::Ssh(inner) => ssh(inner),
            crate::EngineError::SshUnavailable(v0) => {
                bisa_core::text!("error-engine-ssh-unavailable", v0 = v0.to_string())
            }
            crate::EngineError::MobileDevelopment(inner) => mobile_development(inner),
            crate::EngineError::MobileDevelopmentUnavailable(t) => t.clone(),
            crate::EngineError::Iso(v0) => {
                bisa_core::text!("error-engine-iso", v0 = v0.to_string())
            }
            crate::EngineError::IsoFailed(v0) => {
                bisa_core::text!("error-engine-iso-failed", v0 = v0.to_string())
            }
            crate::EngineError::Security(v0) => {
                bisa_core::text!("error-engine-security", v0 = v0.to_string())
            }
            crate::EngineError::Provider(inner) => inner.text(),
            crate::EngineError::OwnPullRequest { author, .. } => {
                bisa_core::text!("error-engine-own-pull-request", author = author.to_string())
            }
            crate::EngineError::NothingToCommit(v0) => {
                bisa_core::text!("error-engine-nothing-to-commit", v0 = v0.to_string())
            }
            crate::EngineError::ProjectAmbiguous { step, count, .. } => bisa_core::text!(
                "error-engine-project-ambiguous",
                step = step.to_string(),
                count = *count
            ),
            crate::EngineError::IdentityUnset { project, .. } => {
                bisa_core::text!("error-engine-identity-unset", project = project.to_string())
            }
            crate::EngineError::RepoIdentityUnset { what } => {
                bisa_core::text!("error-engine-repo-identity-unset", what = what.to_string())
            }
            crate::EngineError::PullNotOffered { what } => {
                bisa_core::text!("error-engine-pull-not-offered", what = what.to_string())
            }
            crate::EngineError::PublishManual { project, what, .. } => bisa_core::text!(
                "error-engine-publish-manual",
                project = project.to_string(),
                what = what.to_string()
            ),
            crate::EngineError::PublishNoGoal {
                project,
                workstream,
                what,
                ..
            } => bisa_core::text!(
                "error-engine-publish-no-goal",
                project = project.to_string(),
                workstream = workstream.to_string(),
                what = what.to_string()
            ),
            crate::EngineError::PublishDeclined { what, .. } => {
                bisa_core::text!("error-engine-publish-declined", what = what.to_string())
            }
            crate::EngineError::NothingToPublish {
                workstream,
                branch,
                base,
                ..
            } => bisa_core::text!(
                "error-engine-nothing-to-publish",
                branch = branch.to_string(),
                base = base.to_string(),
                workstream = workstream.to_string()
            ),
            crate::EngineError::Invalid(t) => t.clone(),
            crate::EngineError::Conflict(t) => t.clone(),
            crate::EngineError::PullRequestNotOpen { number, state, .. } => bisa_core::text!(
                "error-engine-pull-request-not-open",
                number = *number,
                state = state.to_string()
            ),
            crate::EngineError::WorkstreamScript { phase, reason, .. } => bisa_core::text!(
                "error-engine-workstream-script",
                phase = phase.to_string(),
                reason = reason.to_string()
            ),
            crate::EngineError::FileConflict { path, .. } => {
                bisa_core::text!("error-engine-file-conflict", path = path.to_string())
            }
            crate::EngineError::Locked { path, holder, .. } => bisa_core::text!(
                "error-engine-locked",
                path = path.to_string(),
                a0 = (match holder {
                    Some(h) => format!(", {h}"),
                    None => String::new(),
                })
                .to_string()
            ),
            crate::EngineError::Answer(inner) => inner.text(),
            crate::EngineError::Core(inner) => inner.text(),
            crate::EngineError::Run(inner) => inner.text(),
            crate::EngineError::Template(inner) => inner.text(),
            crate::EngineError::DesignRefused(inner) => inner.text(),
        }
    }
}

impl Localize for crate::registry::RegistryError {
    fn text(&self) -> Text {
        match self {
            crate::registry::RegistryError::CasFailed => {
                bisa_core::text!("error-engine-registry-cas-failed")
            }
            crate::registry::RegistryError::Stale {
                expected, current, ..
            } => bisa_core::text!(
                "error-engine-registry-stale",
                expected = *expected,
                current = *current
            ),
            crate::registry::RegistryError::Aborted(v0) => {
                bisa_core::text!("error-engine-registry-aborted", v0 = v0.to_string())
            }
            crate::registry::RegistryError::NotFound(v0) => {
                bisa_core::text!("error-engine-registry-not-found", v0 = v0.to_string())
            }
        }
    }
}

impl Localize for crate::scheduler::ScheduleRejection {
    fn text(&self) -> Text {
        match self {
            crate::scheduler::ScheduleRejection::NotOpen(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-not-open",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::BudgetExhausted(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-budget-exhausted",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::GoalMissing(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-goal-missing",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::RunMissing(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-run-missing",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::HarnessDisabled(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-harness-disabled",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::DepthExhausted => {
                bisa_core::text!("error-engine-schedule-rejection-depth-exhausted")
            }
            crate::scheduler::ScheduleRejection::SpawnNotAllowed {
                agent, allowlist, ..
            } => bisa_core::text!(
                "error-engine-schedule-rejection-spawn-not-allowed",
                agent = format!("{agent:?}"),
                allowlist = format!("{allowlist:?}")
            ),
            crate::scheduler::ScheduleRejection::GoalClosed(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-goal-closed",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::RunFinished(v0) => bisa_core::text!(
                "error-engine-schedule-rejection-run-finished",
                v0 = v0.to_string()
            ),
            crate::scheduler::ScheduleRejection::NoStep => {
                bisa_core::text!("error-engine-schedule-rejection-no-step")
            }
            crate::scheduler::ScheduleRejection::StepNotRunning { step, state, .. } => {
                bisa_core::text!(
                    "error-engine-schedule-rejection-step-not-running",
                    step = step.to_string(),
                    state = state.to_string()
                )
            }
            crate::scheduler::ScheduleRejection::CapsClosed => {
                bisa_core::text!("error-engine-schedule-rejection-caps-closed")
            }
        }
    }
}

/// `IsoError` said to a person (iso carries no `Text` of its own).
pub fn iso(e: &bisa_iso::IsoError) -> Text {
    match e {
        bisa_iso::IsoError::Unavailable(v0) => {
            bisa_core::text!("error-iso-unavailable", v0 = v0.to_string())
        }
        bisa_iso::IsoError::Other(v0) => bisa_core::text!("error-iso-other", v0 = v0.to_string()),
    }
}

/// `LspError` said to a person (lsp carries no `Text` of its own).
pub fn lsp(e: &bisa_lsp::LspError) -> Text {
    match e {
        bisa_lsp::LspError::Spawn(v0) => bisa_core::text!("error-lsp-spawn", v0 = v0.to_string()),
        bisa_lsp::LspError::Protocol(v0) => {
            bisa_core::text!("error-lsp-protocol", v0 = v0.to_string())
        }
        bisa_lsp::LspError::Closed => bisa_core::text!("error-lsp-closed"),
        bisa_lsp::LspError::Timeout(v0) => {
            bisa_core::text!("error-lsp-timeout", v0 = v0.to_string())
        }
        bisa_lsp::LspError::ServerError { code, message, .. } => bisa_core::text!(
            "error-lsp-server-error",
            message = message.to_string(),
            code = *code
        ),
        bisa_lsp::LspError::NoServer(v0) => {
            bisa_core::text!("error-lsp-no-server", v0 = v0.to_string())
        }
    }
}

/// `MobileDevelopmentError` said to a person (mobile carries no `Text` of its own).
pub fn mobile_development(e: &bisa_mobile_development::MobileDevelopmentError) -> Text {
    match e {
        bisa_mobile_development::MobileDevelopmentError::NotInstalled(v0) => {
            bisa_core::text!(
                "error-mobile-development-not-installed",
                v0 = v0.to_string()
            )
        }
        bisa_mobile_development::MobileDevelopmentError::Timeout { what, secs, .. } => {
            bisa_core::text!(
                "error-mobile-development-timeout",
                what = what.to_string(),
                secs = *secs
            )
        }
        bisa_mobile_development::MobileDevelopmentError::Failed { what, detail, .. } => {
            bisa_core::text!(
                "error-mobile-development-failed",
                what = what.to_string(),
                detail = detail.to_string()
            )
        }
        bisa_mobile_development::MobileDevelopmentError::NoSuchDevice(v0) => {
            bisa_core::text!(
                "error-mobile-development-no-such-device",
                v0 = v0.to_string()
            )
        }
        bisa_mobile_development::MobileDevelopmentError::Unsupported(v0) => {
            bisa_core::text!("error-mobile-development-unsupported", v0 = v0.to_string())
        }
    }
}

/// `SshError` said to a person (ssh carries no `Text` of its own).
pub fn ssh(e: &bisa_ssh::SshError) -> Text {
    match e {
        bisa_ssh::SshError::Unavailable(v0) => {
            bisa_core::text!("error-ssh-unavailable", v0 = v0.to_string())
        }
        bisa_ssh::SshError::Refused(v0) => {
            bisa_core::text!("error-ssh-refused", v0 = v0.to_string())
        }
        bisa_ssh::SshError::Timeout { what, secs, .. } => {
            bisa_core::text!("error-ssh-timeout", what = what.to_string(), secs = *secs)
        }
        bisa_ssh::SshError::Failed { what, detail, .. } => bisa_core::text!(
            "error-ssh-failed",
            what = what.to_string(),
            detail = detail.to_string()
        ),
    }
}

/// `VcsError` said to a person (vcs carries no `Text` of its own).
pub fn vcs(e: &bisa_vcs::VcsError) -> Text {
    match e {
        bisa_vcs::VcsError::NotAvailable(v0) => {
            bisa_core::text!("error-vcs-not-available", v0 = v0.to_string())
        }
        bisa_vcs::VcsError::NotARepository(v0) => bisa_core::text!(
            "error-vcs-not-a-repository",
            a0 = (v0.display()).to_string()
        ),
        bisa_vcs::VcsError::Dirty { path, details, .. } => bisa_core::text!(
            "error-vcs-dirty",
            a0 = (path.display()).to_string(),
            details = details.to_string()
        ),
        bisa_vcs::VcsError::Conflict { message, .. } => {
            bisa_core::text!("error-vcs-conflict", message = message.to_string())
        }
        bisa_vcs::VcsError::NotFastForward { ahead, behind, .. } => bisa_core::text!(
            "error-vcs-not-fast-forward",
            ahead = *ahead,
            behind = *behind
        ),
        bisa_vcs::VcsError::InProgress(v0) => {
            bisa_core::text!("error-vcs-in-progress", v0 = format!("{v0:?}"))
        }
        bisa_vcs::VcsError::NothingToStash => bisa_core::text!("error-vcs-nothing-to-stash"),
        bisa_vcs::VcsError::NothingToDiscard => bisa_core::text!("error-vcs-nothing-to-discard"),
        bisa_vcs::VcsError::StashMoved { index, commit, .. } => bisa_core::text!(
            "error-vcs-stash-moved",
            index = *index,
            commit = commit.to_string()
        ),
        bisa_vcs::VcsError::NoRemote(v0) => {
            bisa_core::text!("error-vcs-no-remote", v0 = v0.to_string())
        }
        bisa_vcs::VcsError::NotAuthenticated(v0) => {
            bisa_core::text!("error-vcs-not-authenticated", v0 = v0.to_string())
        }
        bisa_vcs::VcsError::IdentityUnset(v0) => {
            bisa_core::text!("error-vcs-identity-unset", v0 = v0.to_string())
        }
        bisa_vcs::VcsError::RepositoryBusy { path, .. } => bisa_core::text!(
            "error-vcs-repository-busy",
            a0 = (path.display()).to_string()
        ),
        bisa_vcs::VcsError::InvalidArg { what, value, .. } => bisa_core::text!(
            "error-vcs-invalid-arg",
            what = what.to_string(),
            value = format!("{value:?}")
        ),
        bisa_vcs::VcsError::Timeout { what, secs, .. } => {
            bisa_core::text!("error-vcs-timeout", what = what.to_string(), secs = *secs)
        }
        bisa_vcs::VcsError::Command {
            what, code, stderr, ..
        } => bisa_core::text!(
            "error-vcs-command",
            what = what.to_string(),
            code = code.to_string(),
            stderr = stderr.to_string()
        ),
        bisa_vcs::VcsError::Other(v0) => bisa_core::text!("error-vcs-other", v0 = v0.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guided::DesignRefusal;
    use crate::interactive::InteractiveError;
    use crate::registry::RegistryError;
    use crate::scheduler::ScheduleRejection;
    use crate::EngineError;
    use bisa_core::text::plain;
    use bisa_core::{GoalId, StepId};

    /// Ids whose English catalog sentence says more than `Display`, on
    /// purpose: it begins with the same words and goes on.
    const SAYS_MORE: &[&str] = &["error-vcs-nothing-to-discard"];

    /// The catalog's sentence is the one `Display` shows, word for word.
    fn judge(said: Text, shown: String) {
        let id = said.id.to_string();
        assert!(plain::has(&id), "{id} is a message of the English catalog");
        let said = said.to_string();
        if SAYS_MORE.contains(&id.as_str()) {
            assert!(said.starts_with(&shown), "{id}: {said:?} begins {shown:?}");
            return;
        }
        assert_eq!(said, shown, "{id}");
    }

    fn goal() -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))
    }

    fn words() -> Text {
        bisa_core::text!("error-engine-unknown-gate", v0 = "g")
    }

    #[test]
    fn the_crates_beneath_the_engine_are_said_from_the_catalog_as_their_display_says() {
        use bisa_codehost::CodeHostError as C;
        for e in [
            C::NotAuthenticated("a".into()),
            C::NotFound("b".into()),
            C::Refused("c".into()),
            C::Unsupported("d".into()),
            C::Transport("e".into()),
        ] {
            judge(codehost_code_host_error(&e), e.to_string());
        }
        use bisa_connectors::ConnectorError as K;
        for e in [
            K::BadDefinition("a".into()),
            K::BadParam {
                name: "n".into(),
                why: "w".into(),
            },
            K::Unresolved("u".into()),
            K::HostRefused {
                host: "h".into(),
                allowed: "a, b".into(),
            },
            K::NotAuthenticated("n".into()),
            K::NotFound("f".into()),
            K::Refused {
                status: 403,
                reason: "no".into(),
            },
            K::RateLimited { retry_after: 7 },
            K::Upstream {
                status: 502,
                reason: "down".into(),
            },
            K::Unreachable("u".into()),
            K::Transport("t".into()),
            K::Timeout(std::time::Duration::from_secs(3)),
            K::Open {
                host: "h".into(),
                until_secs: 9,
            },
            K::SelectMissing { path: "p".into() },
            K::TooLarge(2_000_000),
            K::OAuth("o".into()),
            K::Store("s".into()),
        ] {
            judge(connector(&e), e.to_string());
        }
        use bisa_iso::IsoError as I;
        for e in [I::Unavailable("u".into()), I::Other("o".into())] {
            judge(iso(&e), e.to_string());
        }
        use bisa_lsp::LspError as L;
        for e in [
            L::Spawn("s".into()),
            L::Protocol("p".into()),
            L::Closed,
            L::Timeout("t".into()),
            L::ServerError {
                code: -32600,
                message: "m".into(),
            },
            L::NoServer("rs".into()),
        ] {
            judge(lsp(&e), e.to_string());
        }
        use bisa_mobile_development::MobileDevelopmentError as M;
        for e in [
            M::NotInstalled("adb".into()),
            M::Timeout {
                what: "boot".into(),
                secs: 4,
            },
            M::Failed {
                what: "install".into(),
                detail: "d".into(),
            },
            M::NoSuchDevice("dev".into()),
            M::Unsupported("u".into()),
        ] {
            judge(mobile_development(&e), e.to_string());
        }
        use bisa_ssh::SshError as S;
        for e in [
            S::Unavailable("u".into()),
            S::Refused("r".into()),
            S::Timeout {
                what: "add".into(),
                secs: 2,
            },
            S::Failed {
                what: "keygen".into(),
                detail: "d".into(),
            },
        ] {
            judge(ssh(&e), e.to_string());
        }
        use bisa_vcs::VcsError as V;
        for e in [
            V::NotAvailable("n".into()),
            V::NotARepository("/r".into()),
            V::Dirty {
                path: "/r".into(),
                details: "d".into(),
            },
            V::Conflict {
                message: "m".into(),
                paths: vec![],
                in_progress: None,
            },
            V::NotFastForward {
                ahead: 1,
                behind: 2,
            },
            V::InProgress(bisa_vcs::interactive::InProgress::Merge),
            V::NothingToStash,
            V::NothingToDiscard,
            V::StashMoved {
                index: 1,
                commit: "abc".into(),
                now: None,
            },
            V::NoRemote("origin".into()),
            V::NotAuthenticated("n".into()),
            V::IdentityUnset("i".into()),
            V::RepositoryBusy { path: "/r".into() },
            V::InvalidArg {
                what: "ref".into(),
                value: "-x".into(),
            },
            V::Timeout {
                what: "fetch".into(),
                secs: 5,
            },
            V::Command {
                what: "push".into(),
                code: "1".into(),
                stderr: "e".into(),
            },
            V::Other("o".into()),
        ] {
            judge(vcs(&e), e.to_string());
        }
    }

    #[test]
    fn the_engines_own_refusals_are_said_from_the_catalog_as_their_display_says() {
        for e in [
            DesignRefusal::ManualGoal(goal()),
            DesignRefusal::Closed(goal()),
            DesignRefusal::HasWorkflow(goal()),
            DesignRefusal::HasRun(goal()),
            DesignRefusal::Busy(goal()),
            DesignRefusal::DesignOff,
        ] {
            judge(e.text(), e.to_string());
        }
        for e in [
            InteractiveError::UnknownHarness("h".into()),
            InteractiveError::NotInteractive("n".into()),
            InteractiveError::UnknownSession(crate::registry::LiveRunId::mint()),
            InteractiveError::BadSecret,
            InteractiveError::Io("io".into()),
            InteractiveError::Store(bisa_store::StoreError::KeyStore("k".into())),
        ] {
            judge(e.text(), e.to_string());
        }
        for e in [
            RegistryError::CasFailed,
            RegistryError::Stale {
                expected: 1,
                current: 2,
            },
            RegistryError::Aborted("a".into()),
            RegistryError::NotFound("n".into()),
        ] {
            judge(e.text(), e.to_string());
        }
        for e in [
            ScheduleRejection::NotOpen("done".into()),
            ScheduleRejection::BudgetExhausted("goal:g".into()),
            ScheduleRejection::GoalMissing("g".into()),
            ScheduleRejection::RunMissing("r".into()),
            ScheduleRejection::HarnessDisabled("h".into()),
            ScheduleRejection::DepthExhausted,
            ScheduleRejection::SpawnNotAllowed {
                agent: "a".into(),
                allowlist: vec!["b".into()],
            },
            ScheduleRejection::GoalClosed("g".into()),
            ScheduleRejection::RunFinished("r".into()),
            ScheduleRejection::NoStep,
            ScheduleRejection::StepNotRunning {
                step: "s".into(),
                state: "done".into(),
            },
            ScheduleRejection::CapsClosed,
        ] {
            judge(e.text(), e.to_string());
        }
    }

    #[test]
    fn every_engine_error_is_said_from_the_catalog_as_its_display_says() {
        let holder = crate::lock::LockHolder {
            pid: 7,
            started_at: 1,
        };
        for e in [
            EngineError::Store(bisa_store::StoreError::KeyStore("k".into())),
            EngineError::UnknownGate("g".into()),
            EngineError::GateAlreadyDecided("g".into()),
            EngineError::Io(std::io::Error::other("no")),
            EngineError::Vcs(bisa_vcs::VcsError::Other("v".into())),
            EngineError::CodeHost(bisa_codehost::CodeHostError::NotFound("c".into())),
            EngineError::Connector(bisa_connectors::ConnectorError::Store("s".into())),
            EngineError::Ssh(bisa_ssh::SshError::Refused("r".into())),
            EngineError::SshUnavailable("u".into()),
            EngineError::MobileDevelopment(
                bisa_mobile_development::MobileDevelopmentError::NotInstalled("adb".into()),
            ),
            EngineError::MobileDevelopmentUnavailable(words()),
            EngineError::Iso("i".into()),
            EngineError::IsoFailed("f".into()),
            EngineError::Security("s".into()),
            EngineError::Provider(bisa_decision::ProviderError::Unreachable("p".into())),
            EngineError::OwnPullRequest {
                author: "ada".into(),
            },
            EngineError::NothingToCommit("p".into()),
            EngineError::ProjectAmbiguous {
                step: StepId::new("s").unwrap(),
                count: 2,
            },
            EngineError::IdentityUnset {
                project: "p".into(),
            },
            EngineError::RepoIdentityUnset { what: "the notes" },
            EngineError::PullNotOffered { what: "drawings" },
            EngineError::PublishManual {
                project: "p".into(),
                what: "push".into(),
            },
            EngineError::PublishNoGoal {
                project: "p".into(),
                workstream: "w".into(),
                what: "push".into(),
            },
            EngineError::PublishDeclined {
                what: "push".into(),
            },
            EngineError::NothingToPublish {
                workstream: "w".into(),
                branch: "b".into(),
                base: "main".into(),
            },
            EngineError::Conflict(words()),
            EngineError::PullRequestNotOpen {
                number: 4,
                state: "merged".into(),
            },
            EngineError::WorkstreamScript {
                phase: crate::scripts::Phase::Clean,
                reason: "refused".into(),
                output: "o".into(),
            },
            EngineError::FileConflict {
                path: "a.rs".into(),
                current_hash: "h".into(),
                current_text: "t".into(),
            },
            EngineError::Locked {
                path: "/w".into(),
                holder: None,
            },
            EngineError::Locked {
                path: "/w".into(),
                holder: Some(holder),
            },
            EngineError::Answer(bisa_core::AnswerError::Empty),
            EngineError::Core(bisa_core::CoreError::InvalidPrincipal("x".into())),
            EngineError::Run(bisa_core::RunError::NotStarted),
            EngineError::Template(bisa_core::TemplateError::Unbalanced { at: 1 }),
            EngineError::DesignRefused(DesignRefusal::DesignOff),
        ] {
            judge(e.text(), e.to_string());
        }
        // An invalid request carries its own sentence; `Display` prefixes it.
        let invalid = EngineError::Invalid(words());
        assert_eq!(invalid.text().to_string(), words().to_string());
        assert_eq!(invalid.to_string(), format!("invalid: {}", words()));
    }

    /// What the crates beneath say is a refusal when the person can act on
    /// it, a malfunction when the machine's moment is the reason.
    #[test]
    fn the_crates_beneath_decide_whether_their_failure_is_a_refusal() {
        use bisa_mobile_development::MobileDevelopmentError as M;
        let refusals = [
            EngineError::CodeHost(bisa_codehost::CodeHostError::NotFound("c".into())),
            EngineError::Provider(bisa_decision::ProviderError::Misconfigured("p".into())),
            EngineError::Ssh(bisa_ssh::SshError::Refused("r".into())),
            EngineError::MobileDevelopment(M::NoSuchDevice("d".into())),
            EngineError::MobileDevelopment(M::Unsupported("u".into())),
        ];
        for e in &refusals {
            assert!(e.is_refusal(), "{e}");
        }
        let malfunctions = [
            EngineError::CodeHost(bisa_codehost::CodeHostError::Transport("t".into())),
            EngineError::Provider(bisa_decision::ProviderError::Unreachable("u".into())),
            EngineError::Ssh(bisa_ssh::SshError::Unavailable("u".into())),
            EngineError::MobileDevelopment(M::NotInstalled("adb".into())),
            EngineError::MobileDevelopmentUnavailable(words()),
            EngineError::IsoFailed("f".into()),
            EngineError::Io(std::io::Error::other("no")),
        ];
        for e in &malfunctions {
            assert!(!e.is_refusal(), "{e}");
        }
    }

    /// A failed write is logged for its home, with its item when one is
    /// named, and never fails the work.
    #[test]
    fn a_failure_is_warned_for_its_home_with_or_without_its_item() {
        let home = bisa_core::Home::Goal { goal: goal() };
        let item = bisa_core::WorkItemId::from_ulid(ulid::Ulid::from_parts(2, 2));
        crate::warn_on_err_for::<(), String>(home, Some(item), Err("x".into()), "a write");
        crate::warn_on_err_for::<(), String>(home, None, Err("x".into()), "a write");
        crate::warn_on_err_for::<(), String>(home, None, Ok(()), "a write");
    }
}
