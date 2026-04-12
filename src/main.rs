mod config;
mod daemon;
mod git;
mod secrets;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{Config, RepoEntry};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gitfoam", version, about = "Auto-mirror local git state to a disposable remote branch")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialise config and/or register the current repo with sensible defaults
    Init {
        /// Path to register (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Run the daemon in the foreground
    Daemon,
    /// Register a repo to be mirrored
    Add {
        /// Path to the git repo
        path: PathBuf,
        /// Target branch name (will be created/force-pushed on remote)
        #[arg(long)]
        target: String,
        /// Source branch (informational, default = current branch)
        #[arg(long)]
        source: Option<String>,
        /// Remote name
        #[arg(long, default_value = "origin")]
        remote: String,
        /// Debounce ms (default 500)
        #[arg(long)]
        debounce_ms: Option<u64>,
        /// Commit message override
        #[arg(long)]
        message: Option<String>,
    },
    /// Remove a repo from the config
    Remove {
        path: PathBuf,
    },
    /// List configured repos
    List,
    /// Show daemon/repo status
    Status,
    /// Pause mirroring for a repo
    Pause { path: PathBuf },
    /// Resume mirroring for a repo
    Resume { path: PathBuf },
    /// Print the config file path
    ConfigPath,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { path } => init(path),
        Cmd::Daemon => daemon::run(),
        Cmd::Add {
            path,
            target,
            source,
            remote,
            debounce_ms,
            message,
        } => add_repo(path, target, source, remote, debounce_ms, message),
        Cmd::Remove { path } => remove_repo(path),
        Cmd::List => list_repos(),
        Cmd::Status => status(),
        Cmd::Pause { path } => set_paused(path, true),
        Cmd::Resume { path } => set_paused(path, false),
        Cmd::ConfigPath => {
            println!("{}", Config::path().display());
            Ok(())
        }
    }
}

fn add_repo(
    path: PathBuf,
    target: String,
    source: Option<String>,
    remote: String,
    debounce_ms: Option<u64>,
    message: Option<String>,
) -> Result<()> {
    let canon = fs::canonicalize(&path)
        .with_context(|| format!("canonicalizing {}", path.display()))?;
    if !git::is_git_repo(&canon) {
        anyhow::bail!("{} is not a git repository", canon.display());
    }
    let source = match source {
        Some(s) => s,
        None => git::current_branch(&canon).unwrap_or_else(|_| "HEAD".into()),
    };

    let mut cfg = Config::load()?;
    if let Some(idx) = cfg.find_index(&canon) {
        cfg.repos[idx].source_branch = source;
        cfg.repos[idx].target_branch = target;
        cfg.repos[idx].remote = remote;
        cfg.repos[idx].debounce_ms = debounce_ms;
        cfg.repos[idx].commit_message = message;
        cfg.repos[idx].paused = false;
    } else {
        cfg.repos.push(RepoEntry {
            path: canon.clone(),
            source_branch: source,
            target_branch: target,
            remote,
            debounce_ms,
            commit_message: message,
            paused: false,
        });
    }
    cfg.save()?;
    println!("added {}", canon.display());
    Ok(())
}

fn remove_repo(path: PathBuf) -> Result<()> {
    let mut cfg = Config::load()?;
    if let Some(idx) = cfg.find_index(&path) {
        let removed = cfg.repos.remove(idx);
        cfg.save()?;
        println!("removed {}", removed.path.display());
    } else {
        anyhow::bail!("{} not in config", path.display());
    }
    Ok(())
}

fn list_repos() -> Result<()> {
    let cfg = Config::load()?;
    if cfg.repos.is_empty() {
        println!("no repos configured");
        return Ok(());
    }
    for r in &cfg.repos {
        let pause = if r.paused { " [paused]" } else { "" };
        println!(
            "{}{}\n  source: {}\n  target: {} @ {}\n  debounce: {}ms",
            r.path.display(),
            pause,
            r.source_branch,
            r.target_branch,
            r.remote,
            cfg.debounce_for(r)
        );
    }
    Ok(())
}

fn status() -> Result<()> {
    let cfg = Config::load()?;
    println!("config: {}", Config::path().display());
    println!("repos: {}", cfg.repos.len());
    for r in &cfg.repos {
        let dirty = git::is_dirty(&r.path).unwrap_or(false);
        println!(
            "  {} → {} [{}]{}",
            r.path.display(),
            r.target_branch,
            if dirty { "dirty" } else { "clean" },
            if r.paused { " [paused]" } else { "" }
        );
    }
    Ok(())
}

fn init(path: PathBuf) -> Result<()> {
    let config_path = Config::path();
    let config_existed = config_path.exists();

    // Load (or create) config
    let mut cfg = Config::load()?;

    // Resolve path — allow "." to mean current directory
    let raw = if path.as_os_str() == "." {
        std::env::current_dir().context("reading current directory")?
    } else {
        path.clone()
    };

    // If the target path is not a git repo (or doesn't exist), this is a
    // config-only init: write the file (if missing) and tell the user to cd
    // into a repo and run `gitfoam init` there.
    let canon = fs::canonicalize(&raw).ok();
    let is_repo = canon.as_deref().map(git::is_git_repo).unwrap_or(false);

    if !is_repo {
        if !config_existed {
            cfg.save()?;
            println!("created {}", config_path.display());
        } else {
            println!("config already exists at {}", config_path.display());
        }
        println!();
        println!("next step:");
        println!("  cd into any git repo and run `gitfoam init` to register it");
        println!("  repeat in as many repos as you want, then edit {} to tune",
            config_path.display());
        return Ok(());
    }

    let canon = canon.unwrap();

    // Derive sensible defaults
    let source_branch = git::current_branch(&canon).unwrap_or_else(|_| "HEAD".into());
    let hostname = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                })
        })
        .unwrap_or_else(|| "host".into());
    let target_branch = format!("gitfoam/{}/{}", hostname, source_branch);

    let already = cfg.find_index(&canon).is_some();
    if let Some(idx) = cfg.find_index(&canon) {
        // Only fill in missing fields — never clobber operator tuning
        if cfg.repos[idx].target_branch.is_empty() {
            cfg.repos[idx].target_branch = target_branch.clone();
        }
        if cfg.repos[idx].source_branch.is_empty() {
            cfg.repos[idx].source_branch = source_branch.clone();
        }
    } else {
        cfg.repos.push(RepoEntry {
            path: canon.clone(),
            source_branch: source_branch.clone(),
            target_branch: target_branch.clone(),
            remote: "origin".into(),
            debounce_ms: None,
            commit_message: None,
            paused: false,
        });
    }
    cfg.save()?;

    if !config_existed {
        println!("created {}", config_path.display());
    }
    if already {
        println!("already registered: {}", canon.display());
    } else {
        println!("registered: {}", canon.display());
        println!("  source: {}", source_branch);
        println!("  target: {}", target_branch);
        println!("  remote: origin");
    }
    println!();
    println!("run more:");
    println!("  cd ~/other-repo && gitfoam init   # register another");
    println!("  $EDITOR {}", config_path.display());
    println!("  gitfoam daemon                    # start mirroring");
    Ok(())
}

fn set_paused(path: PathBuf, paused: bool) -> Result<()> {
    let mut cfg = Config::load()?;
    match cfg.find_mut(&path) {
        Some(r) => {
            r.paused = paused;
        }
        None => anyhow::bail!("{} not in config", path.display()),
    }
    cfg.save()?;
    println!("{} {}", if paused { "paused" } else { "resumed" }, path.display());
    Ok(())
}
