//! A connector definition installs from the catalog and reads back whole; an
//! account holds its secrets in the keystore and nowhere a route reads; a
//! definition stays while an account or a workflow step names it.

use bisa_core::WorkflowOrigin;
use bisa_core::{
    AccountId, AuthScheme, ConnectorId, InputDef, InputKind, InputName, OperationId, Origin,
    SecretField, Step, StepId, StepKind, Tags, ValueRef,
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

/// The catalog's refresh: an installed copy below the bundle's revision is
/// rewritten with its id, provenance and time kept, and the credential a
/// scheme change moves goes to the field the new scheme reads — the person
/// never re-enters it. A copy at the bundle's revision, a person's own
/// definition, and a bundle entry that does not parse are left alone.
#[test]
fn a_catalog_connector_is_refreshed_to_the_bundles_revision_and_its_secret_moves_with_the_scheme() {
    let (_dir, ws) = ws();
    ws.install(CatalogKind::Connector, "linear").unwrap();
    let cid = ConnectorId::new("linear").unwrap();
    // Age the installed copy to what the bundle shipped before: the bearer
    // scheme, at no revision.
    let mut old = ws.get_connector(&cid).unwrap();
    old.revision = 0;
    old.auth = AuthScheme::Bearer;
    let old = ws.update_connector(old).unwrap();
    let account = ws
        .create_connector_account(NewConnectorAccount {
            connector: cid.clone(),
            label: "work".into(),
            params: BTreeMap::new(),
            default: true,
        })
        .unwrap();
    ws.set_connector_secrets(
        &cid,
        account.id,
        &BTreeMap::from([(SecretField::Token, "lin_api_1".to_string())]),
    )
    .unwrap();
    // A person's own copy of a built-in, under another id: never the catalog's.
    let mine = bisa_store::catalog::parse_connector(
        "trello",
        include_str!("../../../../library/catalog/connectors/trello.toml"),
    )
    .unwrap()
    .into_new("my-trello")
    .unwrap();
    let mine = ws.create_connector(mine).unwrap();
    assert_eq!(mine.origin, Origin::Local);

    let refreshed = ws.refresh_catalog_connectors().unwrap();
    assert_eq!(refreshed.definitions, vec!["linear".to_string()]);
    assert_eq!(refreshed.accounts_moved, 1);
    let fresh = ws.get_connector(&cid).unwrap();
    assert!(
        matches!(fresh.auth, AuthScheme::ApiKey { .. }),
        "the bundle's scheme: {:?}",
        fresh.auth
    );
    assert!(fresh.revision >= 1);
    assert_eq!(fresh.origin, old.origin, "the provenance stays");
    assert_eq!(fresh.created_at, old.created_at, "and so does the time");
    assert_eq!(
        ws.connector_secret(&cid, account.id, SecretField::ApiKey)
            .unwrap()
            .as_deref(),
        Some("lin_api_1"),
        "the key is the same string under the new scheme's field"
    );
    assert_eq!(
        ws.connector_secret(&cid, account.id, SecretField::Token)
            .unwrap(),
        None,
        "and gone from the old one"
    );
    let acct = ws.get_connector_account(&cid, account.id).unwrap();
    assert_eq!(acct.auth.fields_set, vec![SecretField::ApiKey]);
    // The record fits its scheme again, so an edit goes through.
    ws.update_connector_account(&cid, account.id, "work (eu)".into(), BTreeMap::new())
        .unwrap();
    assert_eq!(
        ws.get_connector(&mine.id).unwrap(),
        mine,
        "a person's own is untouched"
    );
    assert!(
        ws.refresh_catalog_connectors()
            .unwrap()
            .definitions
            .is_empty(),
        "a second pass finds nothing to do"
    );

    // A bundle given by hand: one entry that does not parse costs only
    // itself, a higher revision of another is taken, an equal one is not.
    let slack_v9 = include_str!("../../../../library/catalog/connectors/slack.toml")
        .replace("revision = 1", "revision = 9");
    ws.install(CatalogKind::Connector, "slack").unwrap();
    let bundle = [("linear", "this is not toml"), ("slack", slack_v9.as_str())];
    let refreshed = ws.refresh_catalog_connectors_from(&bundle).unwrap();
    assert_eq!(refreshed.definitions, vec!["slack".to_string()]);
    assert_eq!(refreshed.accounts_moved, 0);
    assert_eq!(ws.get_connector(&slack()).unwrap().revision, 9);
    assert_eq!(
        ws.get_connector(&cid).unwrap().revision,
        fresh.revision,
        "the entry that did not parse left its copy standing"
    );
    assert!(ws
        .refresh_catalog_connectors_from(&bundle)
        .unwrap()
        .definitions
        .is_empty());
}
