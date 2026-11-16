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
                    None => p.text.to_string(), // LCOV_EXCL_LINE: every problem the core makes names its field (`ConnectorProblem::at`)
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
        )?; // LCOV_EXCL_LINE: the store's disk arm: a journal append that fails below the lock
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
        )?; // LCOV_EXCL_LINE: the store's disk arm: a snapshot write that fails below the lock
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
        self.create_connector_with_origin(new, Origin::Local, 0)
    }

    /// Only [`crate::catalog`] passes anything but `Local` and a revision:
    /// provenance is not a caller's to claim, and a person's own definition
    /// has no revision.
    pub(crate) fn create_connector_with_origin(
        &self,
        new: NewConnector,
        origin: Origin,
        revision: u32,
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
            revision,
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
    /// edited by hand is still the catalog's, an install will not overwrite
    /// it, and the catalog's own refresh ([`Workspace::refresh_catalog_connectors`])
    /// brings it to the bundle's revision.
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

    /// The one writer of connector accounts at a time (`accounts_writes`).
    /// A writer that panicked left nothing half-written — a record is one
    /// atomic file — so its poison is taken off.
    fn accounts_writer(&self) -> std::sync::MutexGuard<'_, ()> {
        self.accounts_writes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

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
        let _one_writer = self.accounts_writer();
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
        let _one_writer = self.accounts_writer();
        let def = self.get_connector(connector)?;
        let mut existing = self.get_connector_account(connector, account)?;
        let moved = existing.params != params;
        existing.label = label;
        existing.params = params;
        refuse_account_problems(&def, &existing)?;
        if moved {
            // A token granted for the old parameters — another tenant's,
            // say — is not this account's any more.
            self.forget_granted_token(&def, &mut existing)?;
        }
        self.write_connector_account(&existing)?;
        Ok(existing)
    }

    /// Make one account the connector's default; the others lose the mark.
    pub fn set_default_connector_account(
        &self,
        connector: &ConnectorId,
        account: AccountId,
    ) -> Result<ConnectorAccount, StoreError> {
        let _one_writer = self.accounts_writer();
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
        let _one_writer = self.accounts_writer();
        self.get_connector_account(connector, account)?;
        for field in SecretField::ALL.iter().copied() {
            self.identity
                .delete_secret(&secret_name(connector, account, field))?;
        }
        let path = self.paths.connector_account_file(connector, account);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        Ok(())
    }

    // --- secrets -------------------------------------------------------

    /// Whether `secrets` may be set on an account of `connector`: every
    /// field one the scheme uses, every value non-blank — judged whole,
    /// before any value is written, so a refusal leaves no half-set account
    /// behind and a caller can ask before creating the account at all.
    pub fn validate_connector_secrets(
        &self,
        connector: &ConnectorId,
        secrets: &BTreeMap<SecretField, String>,
    ) -> Result<(), StoreError> {
        let def = self.get_connector(connector)?;
        for (field, value) in secrets {
            if !def.auth.fields().contains(field) {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-connector-has-no-secret-field",
                    connector = connector.to_string(),
                    a0 = (def.auth.word()).to_string(),
                    a1 = (field.as_str()).to_string()
                )));
            }
            if value.trim().is_empty() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-secret-field-empty",
                    a0 = (field.as_str()).to_string()
                )));
            }
        }
        Ok(())
    }

    /// Set secret fields on an account — each replaced, never read back. A
    /// field the connector's scheme does not use, or a blank value, is
    /// refused by name before anything is written. The account's record
    /// learns which fields are set; the values go to the keystore and
    /// nowhere else.
    pub fn set_connector_secrets(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        secrets: &BTreeMap<SecretField, String>,
    ) -> Result<ConnectorAccount, StoreError> {
        let _one_writer = self.accounts_writer();
        self.set_secrets_held(connector, account, secrets)
    }

    /// [`Workspace::set_connector_secrets`], for a caller that already holds
    /// the accounts' writer.
    fn set_secrets_held(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        secrets: &BTreeMap<SecretField, String>,
    ) -> Result<ConnectorAccount, StoreError> {
        self.validate_connector_secrets(connector, secrets)?;
        let mut existing = self.get_connector_account(connector, account)?;
        for (field, value) in secrets {
            self.identity.set_secret(
                &secret_name(connector, account, *field),
                kept_secret(*field, value),
            )?;
            if !existing.auth.fields_set.contains(field) {
                existing.auth.fields_set.push(*field);
            }
        }
        existing.auth.fields_set.sort_by_key(|f| f.as_str());
        let client_moved = [SecretField::ClientId, SecretField::ClientSecret]
            .iter()
            .any(|f| secrets.contains_key(f));
        if client_moved && !secrets.contains_key(&SecretField::AccessToken) {
            let def = self.get_connector(connector)?;
            self.forget_granted_token(&def, &mut existing)?;
        }
        self.write_connector_account(&existing)?;
        Ok(existing)
    }

    /// Under a client-credentials scheme the access token is the platform's
    /// answer to one client and one account's parameters; when either
    /// changes, the token kept for the old ones leaves the keystore and the
    /// record, and the next call asks again. Nothing under another scheme:
    /// an OAuth2 token is the person's consent, not a secret's answer. The
    /// caller holds the accounts' writer and writes the record.
    fn forget_granted_token(
        &self,
        def: &Connector,
        account: &mut ConnectorAccount,
    ) -> Result<(), StoreError> {
        if !matches!(def.auth, AuthScheme::ClientCredentials { .. }) {
            return Ok(());
        }
        self.identity
            .delete_secret(&secret_name(&def.id, account.id, SecretField::AccessToken))?;
        account
            .auth
            .fields_set
            .retain(|f| *f != SecretField::AccessToken);
        account.auth.expires_at = None;
        account.auth.scope = None;
        Ok(())
    }

    /// Move one secret field to another name — the credential that changes
    /// field when a definition's scheme moves between `bearer` and `api_key`
    /// (the catalog's refresh). The value is copied under the new name and
    /// deleted under the old, the record's `fields_set` follows; nothing is
    /// read back by anyone. Answers whether a value moved — a field never set
    /// moves nothing and only leaves the record.
    pub(crate) fn move_connector_secret(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        from: SecretField,
        to: SecretField,
    ) -> Result<bool, StoreError> {
        let _one_writer = self.accounts_writer();
        let mut existing = self.get_connector_account(connector, account)?;
        let value = self
            .identity
            .secret(&secret_name(connector, account, from))?;
        let moved = match value {
            Some(v) if !v.trim().is_empty() => {
                self.identity
                    .set_secret(&secret_name(connector, account, to), &v)?;
                if !existing.auth.fields_set.contains(&to) {
                    existing.auth.fields_set.push(to);
                }
                // The record names both before the old value goes: stopped
                // here, it says more than is true and never less, and the
                // next start finishes the move
                // ([`Workspace::reconcile_connector_accounts`]).
                existing.auth.fields_set.sort_by_key(|f| f.as_str());
                self.write_connector_account(&existing)?;
                true
            }
            _ => false,
        };
        self.identity
            .delete_secret(&secret_name(connector, account, from))?;
        existing.auth.fields_set.retain(|f| *f != from);
        existing.auth.fields_set.sort_by_key(|f| f.as_str());
        self.write_connector_account(&existing)?;
        Ok(moved)
    }

    /// Bring every account's record back in step with its connector's scheme
    /// — what a refresh stopped half-way leaves behind, and what made an
    /// account impossible to edit: a field the scheme no longer reads.
    ///
    /// A `token` under an `api_key` scheme (or the reverse) is the same
    /// pasted secret under its old name: it is moved, unless the new field
    /// already holds one, and the old value goes. Any other field the scheme
    /// does not read leaves the record; its value stays in the keystore
    /// until the account is removed. Answers how many secrets moved.
    pub fn reconcile_connector_accounts(&self) -> Result<usize, StoreError> {
        let mut moved = 0;
        for def in self.list_connectors()? {
            let fields = def.auth.fields();
            for account in self.list_connector_accounts(&def.id)? {
                let strays: Vec<SecretField> = account
                    .auth
                    .fields_set
                    .iter()
                    .copied()
                    .filter(|f| !fields.contains(f))
                    .collect();
                for stray in strays {
                    let heir = match (stray, &def.auth) {
                        (SecretField::Token, AuthScheme::ApiKey { .. }) => {
                            Some(SecretField::ApiKey)
                        }
                        (SecretField::ApiKey, AuthScheme::Bearer) => Some(SecretField::Token),
                        _ => None,
                    };
                    match heir {
                        Some(to) => {
                            let held = self
                                .identity
                                .secret(&secret_name(&def.id, account.id, to))?
                                .is_some_and(|v| !v.trim().is_empty());
                            if held {
                                self.drop_secret_field(&def.id, account.id, stray, Some(to))?;
                            } else if self.move_connector_secret(&def.id, account.id, stray, to)? {
                                moved += 1;
                            }
                        }
                        None => self.drop_secret_field(&def.id, account.id, stray, None)?,
                    }
                    tracing::info!(
                        target: "bisa_store::connectors",
                        connector = %def.id,
                        account = %account.id,
                        field = stray.as_str(), // LCOV_EXCL_LINE a tracing event's field, evaluated under a subscriber only
                        "an account's record named a field its connector's scheme does not read; repaired"
                    );
                }
            }
        }
        Ok(moved)
    }

    /// Take one field off an account's record. With an `heir` — the field
    /// that already holds the same secret under the scheme's new name — the
    /// old value leaves the keystore and the record names the heir.
    fn drop_secret_field(
        &self,
        connector: &ConnectorId,
        account: AccountId,
        field: SecretField,
        heir: Option<SecretField>,
    ) -> Result<(), StoreError> {
        let _one_writer = self.accounts_writer();
        let mut existing = self.get_connector_account(connector, account)?;
        if let Some(heir) = heir {
            self.identity
                .delete_secret(&secret_name(connector, account, field))?;
            if !existing.auth.fields_set.contains(&heir) {
                existing.auth.fields_set.push(heir);
            }
        }
        existing.auth.fields_set.retain(|f| *f != field);
        existing.auth.fields_set.sort_by_key(|f| f.as_str());
        self.write_connector_account(&existing)
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
        let _one_writer = self.accounts_writer();
        let mut existing = self.set_secrets_held(connector, account, &secrets)?;
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
                SecretSource::Keyring // LCOV_EXCL_LINE: the OS keyring; every test runs on the file or the memory store
            } else {
                SecretSource::File
            },
        })
    }
}

/// An account's record must fit its connector: a label, and parameters the
/// definition declares.
/// The part of a secret that is kept: what a paste brought along with it — a
/// line end, a space around a key — is not the secret, and sent it is a
/// header no platform can read. A password keeps its own spaces and loses
/// only the line end.
fn kept_secret(field: SecretField, value: &str) -> &str {
    match field {
        SecretField::Password => value.trim_end_matches(['\r', '\n']),
        _ => value.trim(),
    }
}

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

    /// An account's record has two writers: the person's edit and the
    /// engine's token save. Each reads the record and writes it whole, so
    /// without one writer at a time an edit that read before a save and
    /// wrote after it took the save back.
    #[test]
    fn an_edit_and_a_token_save_at_once_never_undo_each_other() {
        let (_d, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        let mut oauth = def("example");
        oauth.auth = serde_json::from_value(serde_json::json!({
            "scheme": "oauth2",
            "authorization_url": "https://a.example/auth",
            "token_url": "https://a.example/token",
            "scopes": []
        }))
        .unwrap();
        let c = ws.create_connector(oauth).unwrap();
        for round in 0..10u64 {
            let a = ws
                .create_connector_account(NewConnectorAccount {
                    connector: c.id.clone(),
                    label: format!("round {round}"),
                    params: BTreeMap::new(),
                    default: false,
                })
                .unwrap();
            let saving = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let editor = {
                let (ws, c, saving) = (ws.clone(), c.id.clone(), saving.clone());
                std::thread::spawn(move || {
                    let mut n = 0u64;
                    while saving.load(std::sync::atomic::Ordering::SeqCst) {
                        n += 1;
                        ws.update_connector_account(&c, a.id, format!("edit {n}"), BTreeMap::new())
                            .unwrap();
                    }
                })
            };
            for at in 1..=8u64 {
                ws.set_connector_tokens(&c.id, a.id, "at", Some("rt"), Some(at), None)
                    .unwrap();
            }
            saving.store(false, std::sync::atomic::Ordering::SeqCst);
            editor.join().unwrap();
            let kept = ws.get_connector_account(&c.id, a.id).unwrap();
            assert_eq!(kept.auth.expires_at, Some(8), "round {round}: {kept:?}");
            assert_eq!(
                kept.auth.fields_set,
                vec![SecretField::AccessToken, SecretField::RefreshToken],
                "round {round}"
            );
        }
        // A save for an account that is gone does not bring it back.
        let gone = ws
            .create_connector_account(NewConnectorAccount {
                connector: c.id.clone(),
                label: "gone".into(),
                params: BTreeMap::new(),
                default: false,
            })
            .unwrap();
        ws.delete_connector_account(&c.id, gone.id).unwrap();
        assert!(ws
            .set_connector_tokens(&c.id, gone.id, "at", None, None, None)
            .is_err());
        assert!(ws.get_connector_account(&c.id, gone.id).is_err());
        assert_eq!(
            ws.connector_secret(&c.id, gone.id, SecretField::AccessToken)
                .unwrap(),
            None,
            "and leaves no secret behind"
        );
    }

    /// A refresh stopped half-way — the definition rewritten, the secret not
    /// yet moved, or moved and not yet let go — is finished at the next
    /// start, and an account is never left naming a field its scheme does
    /// not read, which no edit could then pass.
    #[test]
    fn an_account_left_behind_by_a_scheme_change_is_repaired() {
        let (_d, ws) = ws();
        let c = ws.create_connector(def("example")).unwrap();
        let account = |label: &str| {
            ws.create_connector_account(NewConnectorAccount {
                connector: c.id.clone(),
                label: label.into(),
                params: BTreeMap::new(),
                default: false,
            })
            .unwrap()
        };
        let token = |v: &str| BTreeMap::from([(SecretField::Token, v.to_string())]);
        let (before, midway) = (account("before the move"), account("mid-move"));
        ws.set_connector_secrets(&c.id, before.id, &token("tok-a"))
            .unwrap();
        ws.set_connector_secrets(&c.id, midway.id, &token("tok-b"))
            .unwrap();
        assert_eq!(
            ws.reconcile_connector_accounts().unwrap(),
            0,
            "nothing to do"
        );
        // The scheme moves to a key in a header; nothing else happened yet.
        let mut keyed = c.clone();
        keyed.auth = serde_json::from_value(serde_json::json!({
            "scheme": "api_key", "place": {"in": "header", "name": "X-Key"}
        }))
        .unwrap();
        ws.update_connector(keyed.clone()).unwrap();
        // One account got as far as the copy: both names hold the value.
        ws.identity
            .set_secret(&secret_name(&c.id, midway.id, SecretField::ApiKey), "tok-b")
            .unwrap();
        assert!(
            ws.update_connector_account(&c.id, before.id, "edited".into(), BTreeMap::new())
                .is_err(),
            "the record names a field the scheme does not read"
        );
        assert_eq!(ws.reconcile_connector_accounts().unwrap(), 1);
        for (id, value) in [(before.id, "tok-a"), (midway.id, "tok-b")] {
            let kept = ws.get_connector_account(&c.id, id).unwrap();
            assert_eq!(kept.auth.fields_set, vec![SecretField::ApiKey]);
            assert_eq!(
                ws.connector_secret(&c.id, id, SecretField::ApiKey)
                    .unwrap()
                    .as_deref(),
                Some(value)
            );
            assert_eq!(
                ws.connector_secret(&c.id, id, SecretField::Token).unwrap(),
                None
            );
            ws.update_connector_account(&c.id, id, "edited".into(), BTreeMap::new())
                .unwrap();
        }
        // Back to a bearer token: the key is the token under its old name.
        let mut bearer = keyed.clone();
        bearer.auth = AuthScheme::Bearer;
        ws.update_connector(bearer).unwrap();
        assert_eq!(ws.reconcile_connector_accounts().unwrap(), 2);
        let kept = ws.get_connector_account(&c.id, before.id).unwrap();
        assert_eq!(kept.auth.fields_set, vec![SecretField::Token]);
        // A scheme with no heir for the field: the record lets go of it.
        let mut basic = keyed;
        basic.auth = AuthScheme::Basic;
        ws.update_connector(basic).unwrap();
        assert_eq!(ws.reconcile_connector_accounts().unwrap(), 0);
        let kept = ws.get_connector_account(&c.id, before.id).unwrap();
        assert!(kept.auth.fields_set.is_empty());
        ws.update_connector_account(&c.id, before.id, "again".into(), BTreeMap::new())
            .unwrap();
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

        // What a paste brings along — a line end, a space — is not the
        // secret: kept, it is a header no platform reads.
        let pasted = BTreeMap::from([(SecretField::Token, "  xoxb-pasted\r\n".to_string())]);
        ws.set_connector_secrets(&c.id, a.id, &pasted).unwrap();
        assert_eq!(
            ws.connector_secret(&c.id, a.id, SecretField::Token)
                .unwrap()
                .as_deref(),
            Some("xoxb-pasted")
        );
        assert_eq!(kept_secret(SecretField::Password, " p w \n"), " p w ");
        assert_eq!(kept_secret(SecretField::ApiKey, "\tk\n"), "k");

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

    // added by the coverage pass: connectors.rs

    // --- the bare lines of the connectors module ---

    /// The connector folders: what is not a definition's or an account's
    /// file is skipped and said; a file or folder nobody may read is an I/O
    /// error by its path; a rebuild indexes every definition again.
    #[test]
    fn the_connector_folders_skip_strangers_and_name_what_they_cannot_read() {
        let (_d, ws) = ws();
        ws.install(crate::catalog::CatalogKind::Connector, "slack")
            .unwrap();
        let slack = ConnectorId::new("slack").unwrap();
        let account = ws
            .create_connector_account(NewConnectorAccount {
                connector: slack.clone(),
                label: "work".into(),
                params: BTreeMap::new(),
                default: true,
            })
            .unwrap();
        let connectors = ws.paths.connectors_dir();
        std::fs::write(connectors.join("Bad Name.json"), b"{}").unwrap();
        std::fs::write(connectors.join("README"), b"").unwrap();
        assert_eq!(ws.list_connectors().unwrap().len(), 1);
        let accounts = ws.paths.connector_accounts_dir(&slack);
        std::fs::write(accounts.join("Bad Name.json"), b"{}").unwrap();
        std::fs::write(accounts.join("README"), b"").unwrap();
        assert_eq!(ws.list_connector_accounts(&slack).unwrap().len(), 1);
        ws.rebuild_index().unwrap();
        assert_eq!(ws.get_connector(&slack).unwrap().id, slack);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sealed = |p: &std::path::Path| {
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o000)).unwrap()
            };
            let open = |p: &std::path::Path, mode: u32| {
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap()
            };
            let file = ws.paths.connector_file(&slack);
            sealed(&file);
            let unreadable = ws.get_connector(&slack);
            open(&file, 0o644);
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            let account_file = accounts.join(format!("{}.json", account.id));
            sealed(&account_file);
            let unreadable = ws.get_connector_account(&slack, account.id);
            open(&account_file, 0o644);
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            sealed(&accounts);
            let unreadable = ws.list_connector_accounts(&slack);
            open(&accounts, 0o755);
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
            sealed(&connectors);
            let unreadable = ws.list_connectors();
            open(&connectors, 0o755);
            assert!(matches!(unreadable, Err(StoreError::Io { .. })));
        }
    }

    /// Accounts: a blank label is refused by name; the default moves to the
    /// newest account that asks for it and the others stay as they are; a
    /// connector with no account has no default; a blank secret is refused.
    #[test]
    fn accounts_are_refused_by_name_and_the_default_moves_to_the_one_that_asks() {
        let (_d, ws) = ws();
        ws.install(crate::catalog::CatalogKind::Connector, "slack")
            .unwrap();
        let slack = ConnectorId::new("slack").unwrap();
        assert!(ws.default_connector_account(&slack).unwrap().is_none());
        let err = ws
            .create_connector_account(NewConnectorAccount {
                connector: slack.clone(),
                label: "   ".into(),
                params: BTreeMap::new(),
                default: true,
            })
            .unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        assert!(err.to_string().contains("label"), "{err}");
        let new = |label: &str, default: bool| NewConnectorAccount {
            connector: slack.clone(),
            label: label.into(),
            params: BTreeMap::new(),
            default,
        };
        let a = ws.create_connector_account(new("a", true)).unwrap();
        let b = ws.create_connector_account(new("b", false)).unwrap();
        let c = ws.create_connector_account(new("c", true)).unwrap();
        let marks: Vec<(String, bool)> = ws
            .list_connector_accounts(&slack)
            .unwrap()
            .into_iter()
            .map(|x| (x.label, x.default))
            .collect();
        assert_eq!(
            marks,
            [
                ("c".to_string(), true),
                ("a".to_string(), false),
                ("b".to_string(), false)
            ]
        );
        assert_eq!(
            ws.default_connector_account(&slack).unwrap().unwrap().id,
            c.id
        );
        let blank = BTreeMap::from([(SecretField::Token, "   ".to_string())]);
        let err = ws.set_connector_secrets(&slack, a.id, &blank).unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        assert!(ws
            .connector_secret(&slack, b.id, SecretField::Token)
            .unwrap()
            .is_none());
    }

    fn account_of(ws: &Workspace, c: &Connector) -> ConnectorAccount {
        ws.create_connector_account(NewConnectorAccount {
            connector: c.id.clone(),
            label: "work".into(),
            params: BTreeMap::new(),
            default: true,
        })
        .unwrap()
    }

    /// A key scheme with a second key takes a secret for each, and still
    /// refuses a field it does not read.
    #[test]
    fn a_second_key_is_set_like_the_first() {
        let (_d, ws) = ws();
        let mut two = def("metrics");
        two.auth = AuthScheme::ApiKey {
            place: bisa_core::KeyPlace::Header {
                name: "X-Api-Key".into(),
            },
            prefix: None,
            extra_keys: vec![bisa_core::ExtraKey {
                secret: SecretField::AppKey,
                label: "Application key".into(),
                place: bisa_core::KeyPlace::Header {
                    name: "X-App-Key".into(),
                },
                prefix: None,
            }],
        };
        let c = ws.create_connector(two).unwrap();
        let a = account_of(&ws, &c);
        let first = BTreeMap::from([(SecretField::ApiKey, "k-one".to_string())]);
        let half = ws.set_connector_secrets(&c.id, a.id, &first).unwrap();
        assert!(!half.is_connected(&c.auth), "one key of two is not enough");
        let second = BTreeMap::from([(SecretField::AppKey, " k-two\n".to_string())]);
        let whole = ws.set_connector_secrets(&c.id, a.id, &second).unwrap();
        assert!(whole.is_connected(&c.auth));
        assert_eq!(
            ws.connector_secret(&c.id, a.id, SecretField::AppKey)
                .unwrap()
                .as_deref(),
            Some("k-two"),
            "kept as the first is: without what a paste brought along"
        );
        let stray = BTreeMap::from([(SecretField::Token, "t".to_string())]);
        assert!(ws.set_connector_secrets(&c.id, a.id, &stray).is_err());

        // A key scheme with one key has no field for a second.
        let mut one = def("plain");
        one.auth = AuthScheme::ApiKey {
            place: bisa_core::KeyPlace::Header {
                name: "X-Api-Key".into(),
            },
            prefix: None,
            extra_keys: vec![],
        };
        let plain = ws.create_connector(one).unwrap();
        let b = account_of(&ws, &plain);
        assert!(ws.set_connector_secrets(&plain.id, b.id, &second).is_err());

        // Forgetting the account takes every key with it.
        ws.delete_connector_account(&c.id, a.id).unwrap();
        assert!(ws
            .connector_secret(&c.id, a.id, SecretField::AppKey)
            .unwrap()
            .is_none());
    }

    /// A client-credentials token is the platform's answer to one client and
    /// one account's parameters: when either changes, the token kept for the
    /// old ones goes, and the next call asks again.
    #[test]
    fn a_granted_token_does_not_outlive_the_client_it_was_granted_to() {
        let (_d, ws) = ws();
        let mut grant = def("meetings");
        grant.auth = serde_json::from_value(serde_json::json!({
            "scheme": "client_credentials",
            "token_url": "https://auth.example.com/token"
        }))
        .unwrap();
        grant.params = vec![bisa_core::ParamDef {
            name: bisa_core::InputName::new("tenant").unwrap(),
            label: "Tenant".into(),
            kind: bisa_core::ParamKind::Text,
            required: false,
            doc: "The tenant.".into(),
            default: None,
        }];
        let c = ws.create_connector(grant).unwrap();
        let a = account_of(&ws, &c);
        let client = BTreeMap::from([
            (SecretField::ClientId, "cid".to_string()),
            (SecretField::ClientSecret, "cs-one".to_string()),
        ]);
        let set = ws.set_connector_secrets(&c.id, a.id, &client).unwrap();
        assert!(set.is_connected(&c.auth), "the client alone connects");
        let refresh = BTreeMap::from([(SecretField::RefreshToken, "r".to_string())]);
        assert!(
            ws.set_connector_secrets(&c.id, a.id, &refresh).is_err(),
            "the grant has no refresh token"
        );

        let granted = |ws: &Workspace| {
            ws.set_connector_tokens(&c.id, a.id, "at-one", None, Some(99), Some("read".into()))
                .unwrap()
        };
        let holds_token = |ws: &Workspace| {
            let record = ws.get_connector_account(&c.id, a.id).unwrap();
            let kept = ws
                .connector_secret(&c.id, a.id, SecretField::AccessToken)
                .unwrap();
            assert_eq!(
                record.auth.fields_set.contains(&SecretField::AccessToken),
                kept.is_some(),
                "the record and the keystore agree"
            );
            assert_eq!(record.auth.expires_at.is_some(), kept.is_some());
            assert_eq!(record.auth.scope.is_some(), kept.is_some());
            kept.is_some()
        };
        let with = granted(&ws);
        assert_eq!(with.auth.expires_at, Some(99));
        assert!(holds_token(&ws));

        // The same label and parameters saved again change nothing.
        ws.update_connector_account(&c.id, a.id, "renamed".into(), BTreeMap::new())
            .unwrap();
        assert!(holds_token(&ws), "a label is not what the token answers");

        // Another secret for the client: the old client's token goes.
        let rotated = BTreeMap::from([(SecretField::ClientSecret, "cs-two".to_string())]);
        let after = ws.set_connector_secrets(&c.id, a.id, &rotated).unwrap();
        assert!(!holds_token(&ws));
        assert!(
            after.is_connected(&c.auth),
            "still connected: the client is set"
        );

        // Another tenant: the token asked for the old one goes.
        granted(&ws);
        ws.update_connector_account(
            &c.id,
            a.id,
            "renamed".into(),
            BTreeMap::from([("tenant".to_string(), serde_json::json!("other"))]),
        )
        .unwrap();
        assert!(!holds_token(&ws));

        // Under OAuth2 a new client secret leaves the tokens: they were the
        // person's consent, not the secret's answer.
        let mut oauth = def("mail");
        oauth.auth = serde_json::from_value(serde_json::json!({
            "scheme": "oauth2",
            "authorization_url": "https://auth.example.com/authorize",
            "token_url": "https://auth.example.com/token"
        }))
        .unwrap();
        let o = ws.create_connector(oauth).unwrap();
        let b = account_of(&ws, &o);
        ws.set_connector_tokens(&o.id, b.id, "at", Some("rt"), Some(99), None)
            .unwrap();
        ws.set_connector_secrets(&o.id, b.id, &rotated).unwrap();
        assert!(ws
            .connector_secret(&o.id, b.id, SecretField::AccessToken)
            .unwrap()
            .is_some());
    }
}
