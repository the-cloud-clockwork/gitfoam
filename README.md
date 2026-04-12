# gitfoam

Auto-mirror your working tree to a throwaway branch on GitHub. One rolling commit, force-pushed every half second. Never lose work from an AI agent again.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/The-Cloud-Clock-Work/gitfoam/main/install.sh | sh
```

Drops a static binary at `~/.local/bin/gitfoam`. Make sure that's on your `PATH`.

## Quick start

```sh
cd /path/to/your/repo
gitfoam add . --target gitfoam/$(hostname)/$(git branch --show-current)
gitfoam daemon
```

That's it. Edit files, watch them appear on the mirror branch in GitHub within ~500ms. Merge into your real branch whenever you want via PR.

## How it works

- Watches the repo every 500ms.
- When dirty → builds an orphan commit of the current tree → force-pushes to your target branch.
- Target branch always has **exactly one commit**. No history, no conflicts, no merges.
- Your working branch, your index, and your `git status` are never touched.
- Secrets (AWS keys, GitHub PATs, JWTs, private keys, high-entropy strings) are excluded from the commit and logged to `~/.gitfoam/blocked.log`.

## Commands

```sh
gitfoam add <path> --target <branch>   # register a repo
gitfoam list                           # show configured repos
gitfoam status                         # show dirty/clean + paused state
gitfoam pause <path>                   # stop mirroring
gitfoam resume <path>                  # resume mirroring
gitfoam remove <path>                  # unregister
gitfoam daemon                         # run in foreground
```

Config lives at `~/.gitfoam.json`. Edit it directly if you prefer — takes effect on daemon restart.

## Run in the background

```sh
mkdir -p ~/.config/systemd/user
curl -fsSL https://raw.githubusercontent.com/The-Cloud-Clock-Work/gitfoam/main/systemd/gitfoam.service \
    > ~/.config/systemd/user/gitfoam.service
systemctl --user enable --now gitfoam
journalctl --user -u gitfoam -f
```

Or just `nohup gitfoam daemon &` — your call.

## Not included

- **Auth.** gitfoam shells out to `git push`. Your SSH agent / credential helper / PAT must already work.
- **Merging, pulling, rebasing.** The target branch is disposable. Review on GitHub, merge manually, delete whenever.

## Uninstall

```sh
rm ~/.local/bin/gitfoam ~/.gitfoam.json
rm -rf ~/.gitfoam
```

MIT. Source: [github.com/The-Cloud-Clock-Work/gitfoam](https://github.com/The-Cloud-Clock-Work/gitfoam)
