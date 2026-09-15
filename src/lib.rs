//! Core library for `skills`: scanning a central skills directory, keeping
//! per-skill metadata as TOML files, and deploying skills to agents via
//! symlinks. Both the CLI and the TUI are thin layers over this crate.

pub mod agents;
pub mod config;
pub mod dict;
pub mod hash;
pub mod history;
pub mod meta;
pub mod ops;
pub mod paths;
pub mod preset;
pub mod reconcile;
pub mod repository;
pub mod search;
pub mod skill;
pub mod util;

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Everything an operation needs: the root, the loaded config, and stores.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub project: Option<PathBuf>,
    /// Project associated with the transient directory inventory.
    pub inventory_project: Option<PathBuf>,
    pub inventory_products: Option<std::collections::BTreeSet<String>>,
    pub config: config::Config,
    pub meta: meta::MetaStore,
    pub presets: preset::PresetStore,
    pub preset_migration: Option<preset::MigrationReport>,
}

impl Workspace {
    pub fn open(root: &Path) -> Result<Self> {
        // Library callers need the same canonical root as the CLI. In
        // particular, macOS /var and /private/var can name the same directory.
        let root = paths::resolve_root(Some(root))?;
        let config = config::Config::load(&root)?;
        let presets = preset::PresetStore::new(&root);
        let preset_migration = presets.migrate_legacy_tags(&config)?;
        Ok(Self {
            meta: meta::MetaStore::new(&root),
            presets,
            preset_migration,
            root,
            project: None,
            inventory_project: None,
            inventory_products: None,
            config,
        })
    }

    /// Open an isolated project store; never consult the global root pointer.
    pub fn open_local(project: &Path, create: bool) -> Result<Self> {
        use anyhow::Context;
        let project = std::fs::canonicalize(project).context("project directory does not exist")?;
        anyhow::ensure!(project.is_dir(), "project is not a directory");
        let root = project.join(".agents/skills");
        paths::ensure_local_path(&project, &root)?;
        if create {
            std::fs::create_dir_all(&root)?;
        }
        // Do not load global defaults even when no local config exists yet.
        let root = paths::resolve_root(Some(&root))?;
        let mut ws = Self {
            meta: meta::MetaStore::new(&root),
            presets: preset::PresetStore::new(&root),
            preset_migration: None,
            root,
            project: Some(project),
            inventory_project: None,
            inventory_products: None,
            config: config::Config::local_default(),
        };
        ws.config = ws.load_config()?;
        ws.preset_migration = ws.presets.migrate_legacy_tags(&ws.config)?;
        Ok(ws)
    }

    pub fn load_config(&self) -> Result<config::Config> {
        let Some(project) = &self.project else {
            let config = config::Config::load(&self.root)?;
            return Ok(config);
        };
        let mut config = if config::Config::exists(&self.root) {
            config::Config::load(&self.root)?
        } else {
            config::Config::local_default()
        };
        // An omitted agents table also means local defaults.
        if config::Config::exists(&self.root) {
            let text = std::fs::read_to_string(config::Config::path(&self.root))?;
            let doc: toml::Value = toml::from_str(&text)?;
            if doc.get("agents").is_none() {
                config.agents = agents::defaults(true);
            }
        }
        for agent in &mut config.agents {
            let path = project.join(agent.skills_path());
            paths::ensure_local_path(project, &path)?;
            agent.skills_dir = path.to_string_lossy().into_owned();
        }
        Ok(config)
    }

    /// Read the current filesystem using this workspace's configuration
    /// snapshot. Callers explicitly load and validate configuration before
    /// replacing `config`; scanning never discards in-memory scope overrides.
    pub fn scan(&self) -> Result<reconcile::Snapshot> {
        reconcile::scan(&self.root, &self.config)
    }

    /// Fresh source/destination inventory for link planning, without unrelated
    /// baseline verification. Never use this snapshot to display content health.
    /// As with `scan`, configuration comes from `self.config`, not another file
    /// read. Write entry points remain responsible for fresh-state validation.
    pub fn scan_for_links(&self) -> Result<reconcile::Snapshot> {
        reconcile::scan_for_links(&self.root, &self.config)
    }

    pub fn skill_path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}
