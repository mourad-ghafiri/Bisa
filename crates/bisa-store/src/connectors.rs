//! Connectors: the declarative definitions of outside platforms' APIs a
//! workflow step may call (kind 33414), and the **accounts** that hold a
//! person's way in.
//!
//! The two are kept apart on purpose. A definition — a base URL, the hosts it
//! may reach, an auth scheme, its operations — reads the same on every node,
//! so it is a truth file with a snapshot that syncs, like a skill. An account
//! is this machine's: its record (a label, the non-secret parameters, which
//! secret fields are set) lives under `identity/connectors/<connector>/`, and
//! every secret field lives in the keystore under
//! `connector:<connector>:<account>:<field>`. No route, event or snapshot
//! carries a value; the store answers which fields are set and where they
//! live ([`AccountSecrets`]).
//!
//! **Nothing is deleted while something points at it** ([`crate::usage`]): a
//! definition stays while an account or a workflow step names it.
//!
//! Truth: `connectors/<id>.json`; snapshot events in
//! `connectors/state/33414-<id>.json`; `connectors` index table plus its tag
//! rows (both rebuildable); `identity/connectors/<id>/<account>.json`.

use crate::error::StoreError;
use crate::paths::Paths;
use crate::teams::origin_str;
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_CONNECTOR;
use bisa_core::tags::TagEntity;
use bisa_core::{
    AccountAuth, AccountId, AuthScheme, Connector, ConnectorAccount, ConnectorId, Operation,
    Origin, ParamDef, SecretField, Tags,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fields a caller supplies when creating a connector definition.
#[derive(Clone, Debug)]
pub struct NewConnector {
    pub id: ConnectorId,
    pub name: String,
    pub description: String,
    pub tags: Tags,
    pub base_url: String,
    pub hosts: Vec<String>,
    pub insecure_tls: bool,
    pub auth: AuthScheme,
    pub params: Vec<ParamDef>,
    pub operations: Vec<Operation>,
    pub check: Option<bisa_core::OperationId>,
}

/// Fields a caller supplies when adding an account to a connector. The
/// secrets travel separately ([`Workspace::set_connector_secrets`]) so a
/// record and a value never share a struct.
#[derive(Clone, Debug)]
pub struct NewConnectorAccount {
    pub connector: ConnectorId,
    pub label: String,
    pub params: BTreeMap<String, serde_json::Value>,
    pub default: bool,
}

/// Where a stored secret lives — the file store under `identity/`, or the OS
/// keyring when `BISA_KEYSTORE=keyring`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    File,
    Keyring,
}

/// Which secret fields an account holds and where — never a value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSecrets {
    pub fields_set: Vec<SecretField>,
    pub source: SecretSource,
}

/// The keystore name of one secret field of one account.
pub fn secret_name(connector: &ConnectorId, account: AccountId, field: SecretField) -> String {
    format!("connector:{connector}:{account}:{}", field.as_str())
}

impl Workspace {
    // --- definitions ---------------------------------------------------

    fn write_connector(&self, def: &Connector) -> Result<(), StoreError> {
        let problems = def.validate();
        if !problems.is_empty() {
            // Every problem, named: a person fixes them all at once, not one per try.
            let named: Vec<String> = problems
                .iter()
                .map(|p| match &p.field {
                    Some(field) => format!("{field}: {}", p.text),
                    None => p.text.to_string(),
                })
                .collect();
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-connector",
                a0 = (def.id).to_string(),
                a1 = named.join("; "),
                a2 = String::new()
            )));
        }
        let path = self.paths.connector_file(&def.id);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(def)?)?;
        let existing_rev = self.snapshots.current_revision(
            Paths::NS_CONNECTORS,
            KIND_CONNECTOR,
            def.id.as_str(),
        )?;
        let event = self.snapshots.put(
            Paths::NS_CONNECTORS,
            KIND_CONNECTOR,
            def.id.as_str(),
            def,
            existing_rev + 1,
            &self.owner,
            now_secs(),
            None,
            def.tags.as_slice(),
        )?;
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_CONNECTOR,
            d: def.id.to_string(),
            event,
            audience: EventAudience::Workspace,
        });
        self.index_connector(def)
    }

    pub(crate) fn index_connector(&self, def: &Connector) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.upsert_connector(
            def.id.as_str(),
            &def.name,
            &def.description,
            def.auth.word(),
            origin_str(&def.origin),
            def.created_at,
        )?;
        idx.set_tags(TagEntity::Connector, def.id.as_str(), def.tags.as_slice())
    }

    pub fn create_connector(&self, new: NewConnector) -> Result<Connector, StoreError> {
        self.create_connector_with_origin(new, Origin::Local)
    }

    /// Only [`crate::catalog`] passes anything but `Local`: provenance is not
    /// a caller's to claim.
    pub(crate) fn create_connector_with_origin(
        &self,
        new: NewConnector,
        origin: Origin,
    ) -> Result<Connector, StoreError> {
        if self.paths.connector_file(&new.id).exists() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-connector-already-exists",
                a0 = (new.id).to_string()
            )));
        }
        let def = Connector {
            id: new.id,
            name: new.name,
            description: new.description,
            tags: new.tags,
            origin,
            base_url: new.base_url,
            hosts: new.hosts,
            insecure_tls: new.insecure_tls,
            auth: new.auth,
            params: new.params,
            operations: new.operations,
            check: new.check,
            created_at: now_secs(),
        };
        self.write_connector(&def)?;
        Ok(def)
    }

    pub fn get_connector(&self, id: &ConnectorId) -> Result<Connector, StoreError> {
        let path = self.paths.connector_file(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "connector",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "connector", e))
    }

    pub fn list_connectors(&self) -> Result<Vec<Connector>, StoreError> {
        let dir = self.paths.connectors_dir();
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            let Ok(id) = ConnectorId::new(stem) else {
                tracing::warn!("connectors/{name}: not a connector id, skipping");
                continue;
            };
            if let Some(connector) =
                crate::workspace::tolerated("connector", stem, self.get_connector(&id))?
            {
                out.push(connector);
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Replace a connector's definition. The id is immutable — steps and
    /// accounts reference it — and so is its provenance: a catalog entry
    /// edited by hand is still the catalog's, and an install will not
    /// overwrite it.
    pub fn update_connector(&self, def: Connector) -> Result<Connector, StoreError> {
        let existing = self.get_connector(&def.id)?;
        let def = Connector {
            origin: existing.origin,
            created_at: existing.created_at,
            ..def
        };
        self.write_connector(&def)?;
        Ok(def)
    }

    /// Delete a connector, and refuse while an account or a workflow step
    /// still names it.
    pub fn remove_connector(&self, id: &ConnectorId) -> Result<(), StoreError> {
        self.get_connector(id)?;
        self.refuse_if_used(crate::usage::UsageKind::Connector, id.as_str())?;
        let path = self.paths.connector_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        self.snapshots
            .delete_snapshot(Paths::NS_CONNECTORS, KIND_CONNECTOR, id.as_str())?;
        let idx = self.idx();
        idx.delete_connector(id.as_str())?;
        idx.clear_tags(TagEntity::Connector, id.as_str())
    }

    pub(crate) fn reindex_connectors(&self) -> Result<(), StoreError> {
        for def in self.list_connectors()? {
            self.index_connector(&def)?;
        }
        Ok(())
    }

    // --- accounts ------------------------------------------------------

    fn write_connector_account(&self, account: &ConnectorAccount) -> Result<(), StoreError> {
        let dir = self.paths.connector_accounts_dir(&account.connector);
        crate::identity::ensure_private_dir(&self.paths.identity_dir())?;
        crate::identity::ensure_private_dir(dir.parent().unwrap_or(&dir))?;
        crate::identity::ensure_private_dir(&dir)?;
        let path = self
            .paths
            .connector_account_file(&account.connector, account.id);
        // No snapshot, no bus event: an account does not leave the machine.
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(account)?)
    }

    /// Add an account to a connector. The connector must exist and the
    /// record must validate against it; `default` on the new account clears
    /// the others'. Secrets are set afterwards, field by field.
    pub fn create_connector_account(
        &self,
        new: NewConnectorAccount,
    ) -> Result<ConnectorAccount, StoreError> {
        let connector = self.get_connector(&new.connector)?;
        let others = self.list_connector_accounts(&new.connector)?;
        let account = ConnectorAccount {
            id: AccountId::from_ulid(mint_ulid()),
            connector: new.connector,
            label: new.label,
            params: new.params,
            default: new.default || others.is_empty(),
            auth: AccountAuth::default(),
            created_at: now_secs(),
        };
        refuse_account_problems(&connector, &account)?;
        if account.default {
            for mut other in others {
                if other.default {
                    other.default = false;
                    self.write_connector_account(&other)?;
                }
            }
        }
        self.write_connector_account(&account)?;
        Ok(account)
    }

    pub fn get_connector_account(
        &self,
        connector: &ConnectorId,
        account: AccountId,
    ) -> Result<ConnectorAccount, StoreError> {
        let path = self.paths.connector_account_file(connector, account);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "connector account",
                    id: format!("{connector}/{account}"),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|e| StoreError::unreadable(&path, "connector account", e))
    }

    /// A connector's accounts, the default first, then by label.
    pub fn list_connector_accounts(
        &self,
        connector: &ConnectorId,
    ) -> Result<Vec<ConnectorAccount>, StoreError> {
        let dir = self.paths.connector_accounts_dir(connector);
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            let Ok(id) = stem.parse::<AccountId>() else {
                tracing::warn!(
                    "connectors/{connector}/accounts/{name}: not an account id, skipping"
                );
                continue;
            };
            if let Some(account) = crate::workspace::tolerated(
                "connector account",
                stem,
                self.get_connector_account(connector, id),
            )? {
                out.push(account);
            }
        }
        out.sort_by(|a, b| {
            b.default
                .cmp(&a.default)
                .then_with(|| a.label.cmp(&b.label))
        });
        Ok(out)
    }

    /// Every connector account on this machine — what validation reads.
    pub fn list_all_connector_accounts(&self) -> Result<Vec<ConnectorAccount>, StoreError> {
        let mut out = Vec::new();
        for connector in self.list_connectors()? {
            out.extend(self.list_connector_accounts(&connector.id)?);
        }
        Ok(out)
    }

    /// The account a step with no account named runs as: the default, else
    /// the only one, else none.
    pub fn default_connector_account(
        &self,
        connector: &ConnectorId,
    ) -> Result<Option<ConnectorAccount>, StoreError> {
        let mut accounts = self.list_connector_accounts(connector)?;
        Ok(match accounts.len() {
            0 => None,
            1 => accounts.pop(),
            _ => accounts.into_iter().find(|a| a.default),
        })
    }

    /// Replace an account's label and parameters. The id, the connector, the
    /// default mark and the auth facts stay.
    pub fn update_connector_account(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        label: String,
        params: BTreeMap<String, serde_json::Value>,
    ) -> Result<ConnectorAccount, StoreError> {
        let def = self.get_connector(connector)?;
        let mut existing = self.get_connector_account(connector, account)?;
        existing.label = label;
        existing.params = params;
        refuse_account_problems(&def, &existing)?;
        self.write_connector_account(&existing)?;
        Ok(existing)
    }

    /// Make one account the connector's default; the others lose the mark.
    pub fn set_default_connector_account(
        &self,
        connector: &ConnectorId,
        account: AccountId,
    ) -> Result<ConnectorAccount, StoreError> {
        // The account first: a default moved to one nobody added would have
        // taken the mark off the one that had it, and left none.
        let mut chosen = self.get_connector_account(connector, account)?;
        for mut a in self.list_connector_accounts(connector)? {
            let is_it = a.id == account;
            if a.default != is_it {
                a.default = is_it;
                self.write_connector_account(&a)?;
            }
            if is_it {
                chosen = a;
            }
        }
        Ok(chosen)
    }

    /// Forget an account and every secret it held. A default that goes
    /// leaves the connector with no default until one is chosen.
    pub fn delete_connector_account(
        &self,
        connector: &ConnectorId,
        account: AccountId,
    ) -> Result<(), StoreError> {
        let existing = self.get_connector_account(connector, account)?;
        for field in SecretField::ALL.iter().copied() {
            self.identity
                .delete_secret(&secret_name(connector, account, field))?;
        }
        let path = self.paths.connector_account_file(connector, account);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        let _ = existing;
        Ok(())
    }

    // --- secrets -------------------------------------------------------

    /// Set secret fields on an account — each replaced, never read back. A
    /// field the connector's scheme does not use is refused by name. The
    /// account's record learns which fields are set; the values go to the
    /// keystore and nowhere else.
    pub fn set_connector_secrets(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        secrets: &BTreeMap<SecretField, String>,
    ) -> Result<ConnectorAccount, StoreError> {
        let def = self.get_connector(connector)?;
        let mut existing = self.get_connector_account(connector, account)?;
        for field in secrets.keys() {
            if !def.auth.fields().contains(field) {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-connector-has-no-secret-field",
                    connector = connector.to_string(),
                    a0 = (def.auth.word()).to_string(),
                    a1 = (field.as_str()).to_string()
                )));
            }
        }
        for (field, value) in secrets {
            if value.trim().is_empty() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-secret-field-empty",
                    a0 = (field.as_str()).to_string()
                )));
            }
            self.identity
                .set_secret(&secret_name(connector, account, *field), value)?;
            if !existing.auth.fields_set.contains(field) {
                existing.auth.fields_set.push(*field);
            }
        }
        existing.auth.fields_set.sort_by_key(|f| f.as_str());
        self.write_connector_account(&existing)?;
        Ok(existing)
    }

    /// Record what an OAuth exchange or refresh produced: the tokens go to
    /// the keystore, the expiry and scope to the record.
    pub fn set_connector_tokens(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        access_token: &str,
        refresh_token: Option<&str>,
        expires_at: Option<u64>,
        scope: Option<String>,
    ) -> Result<ConnectorAccount, StoreError> {
        let mut secrets = BTreeMap::new();
        secrets.insert(SecretField::AccessToken, access_token.to_string());
        if let Some(refresh) = refresh_token {
            secrets.insert(SecretField::RefreshToken, refresh.to_string());
        }
        let mut existing = self.set_connector_secrets(connector, account, &secrets)?;
        existing.auth.expires_at = expires_at;
        existing.auth.scope = scope;
        self.write_connector_account(&existing)?;
        Ok(existing)
    }

    /// One secret field's value — for the engine, at the moment a request is
    /// built, and for nothing else.
    pub fn connector_secret(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        field: SecretField,
    ) -> Result<Option<String>, StoreError> {
        self.identity
            .secret(&secret_name(connector, account, field))
    }

    /// Which fields an account holds, and where they live.
    pub fn connector_account_secrets(
        &self,
        connector: &ConnectorId,
        account: AccountId,
    ) -> Result<AccountSecrets, StoreError> {
        let existing = self.get_connector_account(connector, account)?;
        Ok(AccountSecrets {
            fields_set: existing.auth.fields_set,
            source: if self.identity.uses_keyring() {
                SecretSource::Keyring
            } else {
                SecretSource::File
            },
        })
    }
}

/// An account's record must fit its connector: a label, and parameters the
/// definition declares.
fn refuse_account_problems(def: &Connector, account: &ConnectorAccount) -> Result<(), StoreError> {
    if let Some(p) = account.validate(def).first() {
        return Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-account-connector",
            a0 = (account.label).to_string(),
            a1 = (def.id).to_string(),
            a2 = p.text.to_string()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use bisa_core::{HttpMethod, OperationId, OutputSpec};

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn def(id: &str) -> NewConnector {
        NewConnector {
            id: ConnectorId::new(id).unwrap(),
            name: "Example".into(),
            description: "An example platform.".into(),
            tags: Tags::default(),
            base_url: "https://api.example.com".into(),
            hosts: vec!["api.example.com".into()],
            insecure_tls: false,
            auth: AuthScheme::Bearer,
            params: vec![],
            operations: vec![Operation {
                id: OperationId::new("ping").unwrap(),
                name: "Ping".into(),
                description: "Answers.".into(),
                method: HttpMethod::Get,
                path: "/ping".into(),
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                body: None,
                params: vec![],
                output: OutputSpec::default(),
                writes: false,
                timeout_secs: None,
                idempotency: None,
                page: None,
            }],
            check: None,
        }
    }

    #[test]
    fn a_connector_is_written_snapshotted_indexed_and_listed() {
        let (_d, ws) = ws();
        let created = ws.create_connector(def("example")).unwrap();
        assert_eq!(created.origin, Origin::Local);
        assert_eq!(ws.list_connectors().unwrap().len(), 1);
        assert_eq!(ws.get_connector(&created.id).unwrap().name, "Example");
        assert!(ws
            .paths
            .state_dir(Paths::NS_CONNECTORS)
            .join(format!("{KIND_CONNECTOR}-example.json"))
            .exists());
        assert!(ws.create_connector(def("example")).is_err(), "no clobber");
        let mut renamed = created.clone();
        renamed.name = "Renamed".into();
        renamed.origin = Origin::Catalog {
            slug: "example".into(),
        };
        let updated = ws.update_connector(renamed).unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(
            updated.origin,
            Origin::Local,
            "provenance is not a caller's to claim"
        );
    }

    #[test]
    fn an_invalid_definition_is_refused_by_name() {
        let (_d, ws) = ws();
        let mut bad = def("bad");
        bad.hosts = vec![];
        let err = ws.create_connector(bad).unwrap_err().to_string();
        assert!(err.contains("connector bad"), "{err}");
        assert!(ws.list_connectors().unwrap().is_empty());
    }

    #[test]
    fn accounts_hold_no_value_and_the_first_is_the_default() {
        let (_d, ws) = ws();
        let c = ws.create_connector(def("example")).unwrap();
        let a = ws
            .create_connector_account(NewConnectorAccount {
                connector: c.id.clone(),
                label: "work".into(),
                params: BTreeMap::new(),
                default: false,
            })
            .unwrap();
        assert!(a.default, "the only account is the default");
        let b = ws
            .create_connector_account(NewConnectorAccount {
                connector: c.id.clone(),
                label: "personal".into(),
                params: BTreeMap::new(),
                default: true,
            })
            .unwrap();
        assert!(b.default);
        assert!(!ws.get_connector_account(&c.id, a.id).unwrap().default);
        assert_eq!(
            ws.default_connector_account(&c.id).unwrap().unwrap().id,
            b.id
        );

        let secrets = BTreeMap::from([(SecretField::Token, "xoxb-secret-value".to_string())]);
        let with = ws.set_connector_secrets(&c.id, a.id, &secrets).unwrap();
        assert_eq!(with.auth.fields_set, vec![SecretField::Token]);
        let on_disk =
            std::fs::read_to_string(ws.paths.connector_account_file(&c.id, a.id)).unwrap();
        assert!(
            !on_disk.contains("xoxb-secret-value"),
            "the record never holds a value"
        );
        assert_eq!(
            ws.connector_secret(&c.id, a.id, SecretField::Token)
                .unwrap()
                .as_deref(),
            Some("xoxb-secret-value")
        );
        let facts = ws.connector_account_secrets(&c.id, a.id).unwrap();
        assert_eq!(facts.fields_set, vec![SecretField::Token]);

        let wrong = BTreeMap::from([(SecretField::ApiKey, "k".to_string())]);
        assert!(
            ws.set_connector_secrets(&c.id, a.id, &wrong).is_err(),
            "not a bearer field"
        );

        assert!(
            ws.remove_connector(&c.id).is_err(),
            "accounts still name it"
        );
        ws.delete_connector_account(&c.id, a.id).unwrap();
        assert!(ws
            .connector_secret(&c.id, a.id, SecretField::Token)
            .unwrap()
            .is_none());
        ws.delete_connector_account(&c.id, b.id).unwrap();
        ws.remove_connector(&c.id).unwrap();
        assert!(ws.list_connectors().unwrap().is_empty());
    }
}
