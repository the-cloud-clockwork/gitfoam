use crate::config::{Config, RepoEntry};
use crate::git;
use crate::secrets;
use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub fn run() -> Result<()> {
    let cfg = Config::load()?;
    if cfg.repos.is_empty() {
        log::warn!("no repos configured — edit {} or use `gitfoam add`", Config::path().display());
    }
    let (tx, rx) = mpsc::channel::<()>();

    // Ctrl-C handler: best-effort, just exit.
    let handler_tx = tx.clone();
    ctrlc_like(move || {
        let _ = handler_tx.send(());
    });

    let mut handles = Vec::new();
    for repo in cfg.repos.clone() {
        if repo.paused {
            log::info!("skipping paused repo {}", repo.path.display());
            continue;
        }
        let cfg = cfg.clone();
        let h = thread::spawn(move || {
            if let Err(e) = repo_loop(&cfg, &repo) {
                log::error!("repo {} loop ended: {:#}", repo.path.display(), e);
            }
        });
        handles.push(h);
    }

    // Main thread blocks until signal; workers run forever on their own.
    let _ = rx.recv();
    log::info!("shutting down");
    Ok(())
}

fn repo_loop(cfg: &Config, repo: &RepoEntry) -> Result<()> {
    let debounce = Duration::from_millis(cfg.debounce_for(repo));
    let msg = cfg.message_for(repo);
    let path = repo.path.clone();

    if !git::is_git_repo(&path) {
        log::error!("{} is not a git repo, skipping", path.display());
        return Ok(());
    }

    // Per-repo private index file so we never touch the user's real index.
    let index_file = index_path_for(&path);
    let mut blocked: HashSet<PathBuf> = HashSet::new();
    let mut last_tree: Option<String> = None;

    log::info!(
        "watching {} → {} (debounce {}ms)",
        path.display(),
        repo.target_branch,
        debounce.as_millis()
    );

    loop {
        let start = Instant::now();
        match tick(repo, &msg, &index_file, &mut blocked, &mut last_tree) {
            Ok(Some(commit)) => {
                log::info!(
                    "{} → {} @ {}",
                    path.display(),
                    repo.target_branch,
                    &commit[..commit.len().min(12)]
                );
            }
            Ok(None) => {}
            Err(e) => {
                log::warn!("{} tick error: {:#}", path.display(), e);
            }
        }
        let elapsed = start.elapsed();
        if elapsed < debounce {
            thread::sleep(debounce - elapsed);
        }
    }
}

fn tick(
    repo: &RepoEntry,
    msg: &str,
    index_file: &PathBuf,
    blocked: &mut HashSet<PathBuf>,
    last_tree: &mut Option<String>,
) -> Result<Option<String>> {
    // Fast path: working tree clean AND we've already pushed at least once → skip
    if !git::is_dirty(&repo.path)? && last_tree.is_some() {
        return Ok(None);
    }

    // Scan dirty files for secrets and add new hits to blocked set
    let dirty = git::dirty_files(&repo.path).unwrap_or_default();
    for f in &dirty {
        if blocked.contains(f) {
            continue;
        }
        match secrets::scan_file(f) {
            Ok(Some(reason)) => {
                log::warn!("BLOCKED {} ({})", f.display(), reason);
                append_blocked_log(f, &reason);
                blocked.insert(f.clone());
            }
            _ => {}
        }
    }

    let excludes: Vec<PathBuf> = blocked.iter().cloned().collect();

    let commit = git::build_mirror_commit(&repo.path, index_file, &excludes, msg)?;

    // Dedup: if the new commit's tree is the same as the last push, skip push.
    // Commit SHA changes each time even for same tree because of timestamps (we set none → same SHA).
    // But to be safe we compare against the last commit sha.
    if let Some(prev) = last_tree.as_deref() {
        if prev == commit {
            return Ok(None);
        }
    }

    git::update_branch_ref(&repo.path, &repo.target_branch, &commit)?;
    git::push_mirror(&repo.path, &repo.remote, &repo.target_branch)?;
    *last_tree = Some(commit.clone());
    Ok(Some(commit))
}

fn index_path_for(repo: &std::path::Path) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join(".gitfoam");
    let _ = std::fs::create_dir_all(&dir);
    let hash = simple_hash(repo.to_string_lossy().as_bytes());
    dir.join(format!("index-{:016x}", hash))
}

fn simple_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn append_blocked_log(path: &std::path::Path, reason: &str) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let log_path = PathBuf::from(home).join(".gitfoam").join("blocked.log");
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{}\t{}\t{}", ts, reason, path.display());
    }
}

/// Install a Ctrl-C handler without adding a crate dep. Best-effort.
fn ctrlc_like<F: FnOnce() + Send + 'static>(_f: F) {
    // Intentionally a no-op: keeping zero extra deps. SIGINT terminates the
    // process; workers are daemon threads so they die with it. Ctrl-C just
    // exits. If user wants graceful shutdown later, wire up signal-hook.
}
