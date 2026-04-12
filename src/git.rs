use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Run git in `repo` with extra env, return stdout on success.
fn git_with_env(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null());
    let out = cmd.output()
        .with_context(|| format!("spawning git {:?}", args))?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    git_with_env(repo, args, &[])
}

pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--git-dir"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Return list of changed/untracked files in the working tree.
/// Format: porcelain v1 lines stripped of the 2-char status prefix.
pub fn dirty_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let out = git(repo, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    let mut files = Vec::new();
    for line in out.lines() {
        if line.len() < 4 {
            continue;
        }
        // Handle rename: "R  old -> new"
        let rest = &line[3..];
        let file = if let Some(idx) = rest.find(" -> ") {
            &rest[idx + 4..]
        } else {
            rest
        };
        // Strip quotes git adds for special chars
        let clean = file.trim_matches('"');
        files.push(repo.join(clean));
    }
    Ok(files)
}

pub fn is_dirty(repo: &Path) -> Result<bool> {
    let out = git(repo, &["status", "--porcelain"])?;
    Ok(!out.trim().is_empty())
}

/// Build an orphan commit from the current working tree (respecting .gitignore),
/// excluding the given pathspec-excluded files (skipped secrets). Returns the commit SHA.
///
/// Uses a private index file so the user's real index is untouched.
pub fn build_mirror_commit(
    repo: &Path,
    index_file: &Path,
    excludes: &[PathBuf],
    message: &str,
) -> Result<String> {
    // Start from HEAD's tree so unchanged files are present in the index.
    // If HEAD doesn't exist (empty repo), init empty index.
    let have_head = git(repo, &["rev-parse", "--verify", "HEAD"]).is_ok();

    // Remove stale index file
    if index_file.exists() {
        std::fs::remove_file(index_file).ok();
    }

    let idx_str = index_file.to_string_lossy().to_string();
    let env = [("GIT_INDEX_FILE", idx_str.as_str())];

    if have_head {
        git_with_env(repo, &["read-tree", "HEAD"], &env)?;
    }

    // Stage everything (respects .gitignore)
    git_with_env(repo, &["add", "-A", "--", "."], &env)?;

    // Remove excluded files from the index (secrets)
    for ex in excludes {
        let rel = match ex.strip_prefix(repo) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let _ = git_with_env(
            repo,
            &["rm", "--cached", "--ignore-unmatch", "-f", "--", &rel],
            &env,
        );
    }

    // Write tree
    let tree = git_with_env(repo, &["write-tree"], &env)?;
    if tree.is_empty() {
        return Err(anyhow!("write-tree produced empty output"));
    }

    // Create orphan commit (no parent) — target branch always has ONE commit.
    let author_env = [
        ("GIT_AUTHOR_NAME", "gitfoam"),
        ("GIT_AUTHOR_EMAIL", "gitfoam@localhost"),
        ("GIT_COMMITTER_NAME", "gitfoam"),
        ("GIT_COMMITTER_EMAIL", "gitfoam@localhost"),
    ];
    let commit = {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).args(["commit-tree", &tree, "-m", message]);
        for (k, v) in author_env {
            cmd.env(k, v);
        }
        let out = cmd.output().context("commit-tree")?;
        if !out.status.success() {
            return Err(anyhow!(
                "commit-tree failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    Ok(commit)
}

/// Update a local branch ref to point at `commit` (no checkout).
pub fn update_branch_ref(repo: &Path, branch: &str, commit: &str) -> Result<()> {
    let refname = format!("refs/heads/{}", branch);
    git(repo, &["update-ref", &refname, commit])?;
    Ok(())
}

/// Get current commit of a local branch, if it exists.
pub fn branch_commit(repo: &Path, branch: &str) -> Option<String> {
    let refname = format!("refs/heads/{}", branch);
    git(repo, &["rev-parse", "--verify", &refname]).ok()
}

/// Push local branch to remote, force-with-lease.
/// For orphan rolling commits, `force-with-lease` can reject (no ancestor).
/// Fall back to plain force in that case.
pub fn push_mirror(repo: &Path, remote: &str, branch: &str) -> Result<()> {
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let lease = git(repo, &["push", "--force-with-lease", remote, &refspec]);
    if lease.is_ok() {
        return Ok(());
    }
    // Fall back to --force (target branch is owned by daemon, operator accepts this)
    git(repo, &["push", "--force", remote, &refspec])?;
    Ok(())
}

pub fn current_branch(repo: &Path) -> Result<String> {
    git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
}
