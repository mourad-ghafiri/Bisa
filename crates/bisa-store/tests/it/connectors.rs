//! A connector definition installs from the catalog and reads back whole; an
//! account holds its secrets in the keystore and nowhere a route reads; a
//! definition stays while an account or a workflow step names it.

use bisa_core::WorkflowOrigin;
use bisa_core::{
    AccountId, ConnectorId, InputDef, InputKind, InputName, OperationId, Origin, SecretField, Step,
    StepId, StepKind, Tags, ValueRef,
};
use bisa_store::{
    CatalogKind, MemoryKeyStore, NewConnectorAccount, NewWorkflow, Paths, ReferenceKind,
    SecretSource, UsageKind, Workspace,
};
use std::collections::BTreeMap;

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn slack() -> ConnectorId {
    ConnectorId::new("slack").unwrap()
}

fn call_step(id: &str, account: Option<ValueRef<AccountId>>) -> Step {
    Step {
        id: StepId::new(id).unwrap(),
        name: id.to_uppercase(),
        kind: StepKind::Connector {
            connector: Some(slack()),
            operation: Some(OperationId::new("post_message").unwrap()),
            account,
            params: BTreeMap::from([
                ("channel".to_string(), "general".to_string()),
                ("text".to_string(), "hello {inputs.who}".to_string()),
            ]),
            output_schema: None,
            // The fixture's write is the person's word; the gate rule has its
            // own tests in the core.
            unattended: true,
        },
        then: vec![],
        boundaries: vec![],
        join: Default::default(),
        on_fail: Default::default(),
        retries: 0,
        max_visits: 3,
        position: None,
    }
}

#[test]
fn a_catalog_connector_installs_and_an_account_keeps_its_secret_in_the_keystore() {
    let (dir, ws) = ws();
    let installed = ws.install(CatalogKind::Connector, "slack").unwrap();
    assert_eq!(installed.connectors, vec!["slack".to_string()]);
    let def = ws.get_connector(&slack()).unwrap();
    assert_eq!(
        def.origin,
        Origin::Catalog {
            slug: "slack".into()
        }
    );
    assert!(def
        .operation(&OperationId::new("post_message").unwrap())
        .is_some());
    assert!(ws.list_connector_accounts(&slack()).unwrap().is_empty());

    let account = ws
        .create_connector_account(NewConnectorAccount {
            connector: slack(),
            label: "work".into(),
            params: BTreeMap::new(),
            default: false,
        })
        .unwrap();
    assert!(account.default, "the first account is the default");
    let secrets = BTreeMap::from([(SecretField::Token, "xoxb-not-a-real-token".to_string())]);
    let with = ws
        .set_connector_secrets(&slack(), account.id, &secrets)
        .unwrap();
    assert_eq!(with.auth.fields_set, vec![SecretField::Token]);
    let facts = ws.connector_account_secrets(&slack(), account.id).unwrap();
    assert_eq!(facts.fields_set, vec![SecretField::Token]);
    assert_eq!(
        facts.source,
        SecretSource::File,
        "a memory keystore reads as the file store"
    );
    // The record on disk and every listing carry the fact, never the value.
    let record = std::fs::read_to_string(
        Paths::new(dir.path()).connector_account_file(&slack(), account.id),
    )
    .unwrap();
    assert!(!record.contains("xoxb-not-a-real-token"));
    let listed = serde_json::to_string(&ws.list_connector_accounts(&slack()).unwrap()).unwrap();
    assert!(!listed.contains("xoxb-not-a-real-token"));
    assert_eq!(
        ws.connector_secret(&slack(), account.id, SecretField::Token)
            .unwrap()
            .as_deref(),
        Some("xoxb-not-a-real-token"),
        "the engine reads it at the moment a request is built"
    );
    // An OAuth field is not a bearer connector's.
    let wrong = BTreeMap::from([(SecretField::RefreshToken, "r".to_string())]);
    assert!(ws
        .set_connector_secrets(&slack(), account.id, &wrong)
        .is_err());
}

#[test]
fn a_connector_stays_while_an_account_or_a_step_names_it() {
    let (_d, ws) = ws();
    ws.install(CatalogKind::Connector, "slack").unwrap();
    let account = ws
        .create_connector_account(NewConnectorAccount {
            connector: slack(),
            label: "work".into(),
            params: BTreeMap::new(),
            default: true,
        })
        .unwrap();
    let wf = ws
        .create_workflow_draft(
            NewWorkflow {
                name: "tell slack".into(),
                description: String::new(),
                inputs: vec![InputDef {
                    name: InputName::new("who").unwrap(),
                    label: "Who".into(),
                    kind: InputKind::Text,
                    default: None,
                    required: true,
                }],
                steps: vec![call_step("tell", None)],
                tags: Tags::default(),
                decision_making: false,
            },
            WorkflowOrigin::Workspace,
        )
        .unwrap();
    let usage = ws.usage_of(UsageKind::Connector, "slack").unwrap();
    let kinds: Vec<ReferenceKind> = usage.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&ReferenceKind::Account), "{usage:?}");
    assert!(kinds.contains(&ReferenceKind::Workflow), "{usage:?}");
    let err = ws.remove_connector(&slack()).unwrap_err().to_string();
    assert!(err.contains("still used by"), "{err}");

    // The step validates against the installed definition and the default account.
    let problems = ws.validate_workflow(&wf.0).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    // With the account gone and no default left, the step cannot run.
    ws.delete_connector_account(&slack(), account.id).unwrap();
    let problems = ws.validate_workflow(&wf.0).unwrap();
    assert!(
        problems
            .iter()
            .any(|p| p.kind == bisa_core::ProblemKind::UnknownAccount),
        "{problems:?}"
    );
    ws.delete_workflow(wf.0.id).unwrap();
    ws.remove_connector(&slack()).unwrap();
    assert!(ws.list_connectors().unwrap().is_empty());
}

/// The default mark moves only to an account that is there: asked to move
/// it to one nobody added, the store refuses by name and the account that
/// had the mark keeps it — never a connector with no default at all.
#[test]
fn the_default_moves_only_to_an_account_that_exists() {
    let (_dir, ws) = ws();
    ws.install(CatalogKind::Connector, "slack").unwrap();
    let account = |label: &str| {
        ws.create_connector_account(NewConnectorAccount {
            connector: slack(),
            label: label.into(),
            params: BTreeMap::new(),
            default: false,
        })
        .unwrap()
    };
    let (work, home) = (account("work"), account("home"));
    assert!(work.default && !home.default);

    let nobody: bisa_core::AccountId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    let refused = ws
        .set_default_connector_account(&slack(), nobody)
        .expect_err("an account nobody added");
    assert!(
        matches!(
            refused,
            bisa_store::StoreError::DefinitionNotFound {
                kind: "connector account",
                ..
            }
        ),
        "{refused}"
    );
    let marks: Vec<(String, bool)> = ws
        .list_connector_accounts(&slack())
        .unwrap()
        .into_iter()
        .map(|a| (a.label, a.default))
        .collect();
    assert_eq!(
        marks,
        [("work".to_string(), true), ("home".to_string(), false)],
        "the mark stayed where it was"
    );

    let moved = ws.set_default_connector_account(&slack(), home.id).unwrap();
    assert!(moved.default);
    let marks: Vec<(String, bool)> = ws
        .list_connector_accounts(&slack())
        .unwrap()
        .into_iter()
        .map(|a| (a.label, a.default))
        .collect();
    assert_eq!(
        marks,
        [("home".to_string(), true), ("work".to_string(), false)],
        "the default first"
    );
}
