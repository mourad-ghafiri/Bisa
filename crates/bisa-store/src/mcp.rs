//! The MCP server registry: named, tagged, reusable — and **local**.
//!
//! An MCP server has no GEP kind: a peer cannot run `npx some-server` on your
//! behalf, so what travels is the id. Truth is a file and an index row.
//!
//! **Nothing is deleted while something points at it** ([`crate::usage`]).
//! The reserved name is refused by [`McpServer::validate`], once, at the door
//! — and again at launch, so a hand-edited truth file cannot smuggle it in.
//!
//! Truth: `mcp/<id>.json`; `mcp_servers` index table plus its tag rows.
//!
//! What leaves this module is the truth: the node masks every `env` and
//! `headers` value on the way out ([`bisa_core::McpServerConfig::masked`]),
//! and `update_mcp` fills a masked value back from the file — so a value is
//! written once, by the person, and never read back over the wire.

use crate::error::StoreError;
use crate::workspace::{now_secs, Workspace};
use bisa_core::tags::TagEntity;
use bisa_core::{AgentId, McpId, McpServer, McpServerConfig, Tags, RESERVED_MCP_NAME};

/// Fields a caller supplies when registering a server.
#[derive(Clone, Debug)]
pub struct NewMcp {
    pub id: McpId,
    pub description: String,
    pub tags: Tags,
    pub transport: McpServerConfig,
}

impl Workspace {
    fn write_mcp(&self, def: &McpServer) -> Result<(), StoreError> {
        def.validate()?;
        let path = self.paths.mcp_file(&def.id);
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(def)?)?;
        // No snapshot, no bus event: this object does not leave the machine.
        self.index_mcp(def)
    }

    pub(crate) fn index_mcp(&self, def: &McpServer) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.upsert_mcp(
            def.id.as_str(),
            &def.name,
            &def.description,
            def.transport.kind(),
            def.enabled,
            def.created_at,
        )?;
        idx.set_tags(TagEntity::Mcp, def.id.as_str(), def.tags.as_slice())
    }

    pub fn create_mcp(&self, new: NewMcp) -> Result<McpServer, StoreError> {
        if self.paths.mcp_file(&new.id).exists() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-mcp-server-already-exists",
                a0 = (new.id).to_string()
            )));
        }
        let def = McpServer {
            id: new.id,
            name: new.transport.name().to_string(),
            description: new.description,
            tags: new.tags,
            transport: new.transport,
            enabled: true,
            created_at: now_secs(),
        };
        self.write_mcp(&def)?;
        Ok(def)
    }

    pub fn get_mcp(&self, id: &McpId) -> Result<McpServer, StoreError> {
        let path = self.paths.mcp_file(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::DefinitionNotFound {
                    kind: "mcp server",
                    id: id.to_string(),
                }
            } else {
                StoreError::io(path.display().to_string(), e)
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::unreadable(&path, "mcp server", e))
    }

    pub fn list_mcps(&self) -> Result<Vec<McpServer>, StoreError> {
        let dir = self.paths.mcp_dir();
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
            let Ok(id) = McpId::new(stem) else {
                tracing::warn!("mcp/{name}: not a server id, skipping");
                continue;
            };
            if let Some(server) =
                crate::workspace::tolerated("mcp server", stem, self.get_mcp(&id))?
            {
                out.push(server);
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Replace a server's fields. The id is immutable, as with skills. A
    /// secret value that comes back masked (`McpServerConfig::MASK`, what
    /// every read answers) keeps the stored one — an edit that touches
    /// nothing keeps everything.
    pub fn update_mcp(&self, def: McpServer) -> Result<McpServer, StoreError> {
        let existing = self.get_mcp(&def.id)?;
        let transport = McpServerConfig::unmasked_from(&existing.transport, def.transport);
        let def = McpServer {
            name: transport.name().to_string(),
            transport,
            created_at: existing.created_at,
            ..def
        };
        self.write_mcp(&def)?;
        Ok(def)
    }

    /// Remove a server, and refuse while any agent still carries it.
    pub fn remove_mcp(&self, id: &McpId) -> Result<(), StoreError> {
        self.get_mcp(id)?;
        self.refuse_if_used(crate::usage::UsageKind::Mcp, id.as_str())?;
        let path = self.paths.mcp_file(id);
        std::fs::remove_file(&path).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        let idx = self.idx();
        idx.delete_mcp(id.as_str())?;
        idx.clear_tags(TagEntity::Mcp, id.as_str())
    }

    /// Resolve an agent's MCP reference list into launch configs. Unknown ids
    /// are skipped with a warning, disabled ones quietly, the reserved name is
    /// refused even from a hand-edited file.
    pub fn mcp_configs(&self, agent: &AgentId, ids: &[McpId]) -> Vec<McpServerConfig> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.get_mcp(id) {
                Ok(def) if !def.enabled => {
                    tracing::debug!(target: "bisa_store::mcp", %agent, server = %id, "the MCP server is disabled and is not mounted")
                }
                Ok(def) if def.transport.name() == RESERVED_MCP_NAME => {
                    tracing::warn!(target: "bisa_store::mcp", %agent, server = %id, "the MCP server claims the reserved name and is refused")
                }
                Ok(def) => out.push(def.transport),
                Err(e) => tracing::warn!(
                    target: "bisa_store::mcp",
                    %agent,
                    server = %id,
                    "an MCP server the agent names does not resolve and is left out of this session: {e}"
                ),
            }
        }
        out
    }

    pub(crate) fn reindex_mcps(&self) -> Result<(), StoreError> {
        for def in self.list_mcps()? {
            self.index_mcp(&def)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use std::collections::BTreeMap;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    fn http(id: &str, name: &str) -> NewMcp {
        NewMcp {
            id: McpId::new(id).unwrap(),
            description: String::new(),
            tags: Tags::default(),
            transport: McpServerConfig::Http {
                name: name.into(),
                url: "http://127.0.0.1:1".into(),
                headers: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn a_server_is_registered_once_and_a_session_mounts_the_enabled_ones_under_their_own_names() {
        let (_dir, ws) = ws();
        let agent = AgentId::new("dev").unwrap();
        let a = ws.create_mcp(http("a", "a")).unwrap();
        assert!(matches!(
            ws.create_mcp(http("a", "a")),
            Err(StoreError::Invalid(_))
        ));
        let mut off = ws.create_mcp(http("b", "b")).unwrap();
        off.enabled = false;
        ws.update_mcp(off).unwrap();
        // The reserved name is refused at the door; a hand-edited file that
        // claims it is refused again at launch.
        let reserved = McpServer {
            id: McpId::new("c").unwrap(),
            name: RESERVED_MCP_NAME.into(),
            transport: McpServerConfig::Http {
                name: RESERVED_MCP_NAME.into(),
                url: "http://127.0.0.1:1".into(),
                headers: BTreeMap::new(),
            },
            ..a.clone()
        };
        std::fs::write(
            ws.paths().mcp_file(&reserved.id),
            serde_json::to_vec(&reserved).unwrap(),
        )
        .unwrap();
        let mounted = ws.mcp_configs(
            &agent,
            &[
                a.id.clone(),
                McpId::new("b").unwrap(),
                reserved.id.clone(),
                McpId::new("nope").unwrap(),
            ],
        );
        assert_eq!(mounted.len(), 1, "{mounted:?}");
        assert_eq!(mounted[0].name(), "a");
        // The listing skips what is not a server file, and a rebuild
        // indexes every file again.
        std::fs::write(ws.paths().mcp_dir().join("notes.txt"), b"x").unwrap();
        std::fs::write(ws.paths().mcp_dir().join("Not An Id.json"), b"{}").unwrap();
        let mut listed: Vec<String> = ws
            .list_mcps()
            .unwrap()
            .iter()
            .map(|m| m.id.to_string())
            .collect();
        listed.sort();
        assert_eq!(listed, ["a", "b", "c"]);
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_mcps().unwrap().len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn a_registry_folder_nobody_may_read_is_an_io_error_by_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, ws) = ws();
        ws.create_mcp(http("a", "a")).unwrap();
        let dir = ws.paths().mcp_dir();
        let was = std::fs::metadata(&dir).unwrap().permissions();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let listed = ws.list_mcps();
        let one = ws.get_mcp(&McpId::new("a").unwrap());
        std::fs::set_permissions(&dir, was).unwrap();
        assert!(matches!(listed, Err(StoreError::Io { .. })), "{listed:?}");
        assert!(matches!(one, Err(StoreError::Io { .. })), "{one:?}");
    }
}
