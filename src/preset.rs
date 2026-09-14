//! Presets: named groups of skills, `<root>/.skills-meta/presets/<name>.toml`.

use crate::paths::meta_dir;
use crate::util::write_atomic;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PRESET_DIR: &str = "presets";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    /// Agents this preset targets; empty means every configured agent.
    #[serde(default)]
    pub agents: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PresetStore {
    pub dir: PathBuf,
}

impl PresetStore {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: meta_dir(root).join(PRESET_DIR),
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.toml"))
    }

    /// Check the stored spelling, even on a case-insensitive filesystem.
    pub fn contains_name(&self, name: &str) -> Result<bool> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let filename = format!("{name}.toml");
        for entry in entries {
            if entry?.file_name() == filename.as_str() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn load(&self, name: &str) -> Result<Option<Preset>> {
        let path = self.path(name);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let p: Preset = toml::from_str(&text)
                    .with_context(|| format!("invalid preset: {}", path.display()))?;
                Ok(Some(p))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn list(&self) -> Result<Vec<Preset>> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        let mut names: Vec<String> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.strip_suffix(".toml").map(|s| s.to_string())
            })
            .collect();
        names.sort();
        for n in names {
            if let Some(p) = self.load(&n)? {
                out.push(p);
            }
        }
        Ok(out)
    }

    pub fn save(&self, preset: &Preset) -> Result<()> {
        if !crate::util::valid_skill_key(&preset.name) {
            bail!("invalid preset name: {}", preset.name);
        }
        let text = toml::to_string_pretty(preset)?;
        write_atomic(&self.path(&preset.name), text.as_bytes())
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let path = self.path(name);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!("no such preset: {name}"),
            Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
        }
    }

    /// Move a preset to a new name. The name is inside the file as well as on
    /// it, so this is a save under the new name followed by removing the old
    /// file rather than a rename of the file itself. That order also means a
    /// crash between the two leaves both copies, which is easy to see and fix,
    /// where the other order could leave none. Only the preset file moves;
    /// deployment state is not stored in the preset.
    pub fn rename(&self, old: &str, new: &str) -> Result<Preset> {
        let mut p = self
            .load(old)?
            .with_context(|| format!("no such preset: {old}"))?;
        let aliases_old = old != new
            && old.eq_ignore_ascii_case(new)
            && self.path(new).exists()
            && !self.contains_name(new)?;
        if self.path(new).exists() && !aliases_old {
            bail!("preset {new} already exists");
        }
        p.name = new.to_string();
        // `save` refuses an invalid name, and does so before the old file
        // goes, so a bad name loses nothing.
        self.save(&p)?;
        // Atomic replacement can retain the existing filename's spelling.
        // Rename that entry explicitly; removing `old` would delete the result.
        if aliases_old {
            std::fs::rename(self.path(old), self.path(new))?;
        } else {
            self.remove(old)?;
        }
        Ok(p)
    }
}
