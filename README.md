# gitfoam

Dumb daemon that auto-mirrors your local git working tree to a disposable remote branch.

One commit. Rolling SHA. Force-pushed every tick. Built for running AI agents that mutate files constantly — so you never lose work and never babysit `git add / commit / push`.

## What it does

- Polls each registered repo every N ms (default 500).
- When the working tree is dirty, it builds an **orphan commit** of the current tree and pushes it to `target_branch` on the remote.
- Next tick: new orphan commit, same branch, force-push. The target branch always has **exactly one commit** on it — the live mirror.
- The source branch is never touched. Your index and working tree are never touched.
- Secrets are scanned before commit (regex + Shannon entropy). Hits are excluded from the commit and logged to `~/.gitfoam/blocked.log`.

## What it does NOT do

- **No auth handling.** `git push` must already work on your box (SSH agent, gh credential helper, PAT, whatever).
- No merging. No pulling. No rebasing. The target branch is disposable — review on GitHub, merge to your working branch manually when ready, delete the mirror whenever you want.
- No inotify fancy business — it's a dumb polling loop.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/The-Cloud-Clock-Work/gitfoam/main/install.sh | sh
```

Binary lands at `~/.local/bin/gitfoam`. Add that to your PATH if it isn't already.

## Usage

```sh
# Register a repo
gitfoam add /path/to/repo --target gitfoam/$(hostname)/dev

# Run the daemon (foreground — use systemd/tmux/nohup to keep it alive)
gitfoam daemon

# Other commands
gitfoam list
gitfoam status
gitfoam pause   /path/to/repo
gitfoam resume  /path/to/repo
gitfoam remove  /path/to/repo
```

## Config

Path: `~/.gitfoam.json` (override with `GITFOAM_CONFIG`)

```json
{
  "daemon": {
    "default_debounce_ms": 500,
    "default_commit_message": "gitfoam: live mirror"
  },
  "repos": [
    {
      "path": "/home/you/dev/myrepo",
      "source_branch": "dev",
      "target_branch": "gitfoam/laptop/dev",
      "remote": "origin",
      "debounce_ms": 500,
      "commit_message": "gitfoam: myrepo live mirror"
    }
  ]
}
```

Edit the file directly or use `gitfoam add` — both work. Restart the daemon to pick up changes.

## Naming the target branch

Recommended: `gitfoam/<hostname>/<source-branch>` so multiple machines don't collide when mirroring the same repo.

## Secrets

Blocked patterns (regex):

- AWS access keys (`AKIA...`, `ASIA...`)
- GitHub tokens (`ghp_`, `ghs_`, `gho_`, `ghu_`, `github_pat_`)
- Slack tokens (`xox[baprs]-...`)
- Private keys (`-----BEGIN ... PRIVATE KEY-----`)
- JWTs (`eyJ...`)
- Generic `api_key=`, `password=`, `secret=`, `token=` with 16+ char values
- OpenAI / Anthropic / Google API keys (`sk-...`, `sk-ant-...`, `AIza...`)

Plus Shannon entropy: any token ≥20 chars with entropy ≥4.5 bits/char is treated as suspicious.

A matching file is **excluded from the commit** for the session, logged to `~/.gitfoam/blocked.log`, and continues to be excluded until the daemon is restarted. The file in your working tree is untouched.

This is not a security boundary — it's a dumb net to catch the obvious stuff. Don't rely on it for compliance.

## systemd user unit

```sh
mkdir -p ~/.config/systemd/user
curl -fsSL https://raw.githubusercontent.com/The-Cloud-Clock-Work/gitfoam/main/systemd/gitfoam.service \
    > ~/.config/systemd/user/gitfoam.service
systemctl --user daemon-reload
systemctl --user enable --now gitfoam
journalctl --user -u gitfoam -f
```

## Uninstall

```sh
rm ~/.local/bin/gitfoam
rm -rf ~/.gitfoam ~/.gitfoam.json
```

## License

MIT
