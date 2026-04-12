use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_DEBOUNCE_MS: u64 = 500;
pub const DEFAULT_COMMIT_MESSAGE: &str = "gitfoam: live mirror";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DaemonDefaults {
    #[serde(default)]
    pub default_debounce_ms: Option<u64>,
    #[serde(default)]
    pub default_commit_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub source_branch: String,
    pub target_branch: String,
    #[serde(default = "default_remote")]
    pub remote: String,
    #[serde(default)]
    pub debounce_ms: Option<u64>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub paused: bool,
}

fn default_remote() -> String {
    "origin".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonDefaults,
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("GITFOAM_CONFIG") {
            return PathBuf::from(p);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".gitfoam.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Config::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config = serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let raw = serde_json::to_string_pretty(self)?;
        fs::write(&path, raw)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn debounce_for(&self, repo: &RepoEntry) -> u64 {
        repo.debounce_ms
            .or(self.daemon.default_debounce_ms)
            .unwrap_or(DEFAULT_DEBOUNCE_MS)
    }

    pub fn message_for(&self, repo: &RepoEntry) -> String {
        repo.commit_message
            .clone()
            .or_else(|| self.daemon.default_commit_message.clone())
            .unwrap_or_else(|| DEFAULT_COMMIT_MESSAGE.into())
    }

    pub fn find_mut(&mut self, path: &Path) -> Option<&mut RepoEntry> {
        let canon = fs::canonicalize(path).ok()?;
        self.repos.iter_mut().find(|r| {
            fs::canonicalize(&r.path).map(|c| c == canon).unwrap_or(false)
        })
    }

    pub fn find_index(&self, path: &Path) -> Option<usize> {
        let canon = fs::canonicalize(path).ok()?;
        self.repos.iter().position(|r| {
            fs::canonicalize(&r.path).map(|c| c == canon).unwrap_or(false)
        })
    }
}
