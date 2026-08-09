# Repository Instructions

These instructions apply to the entire repository.

## Branch and remote

- Perform implementation work directly on the `main` branch.
- Push every completed implementation step to `origin/main`.
- The canonical remote is `git@github.com:xiaobo121388/MCDevHelper.git`.
- Never force-push or rewrite history that has already been pushed.

## Commit discipline

- An implementation step is a coherent group of file changes that can be built, tested, or otherwise verified independently.
- Read-only inspection, tasks with no file changes, and generated build caches do not require commits.
- Before committing, inspect the diff, run the checks relevant to the step, and stage only files that belong to that step.
- Use Conventional Commits with a concise subject, for example: `feat(core): add component discovery`, `fix(ui): handle empty libraries`, `test(mcp): cover tool errors`, or `chore(repo): update tooling`.
- Do not create empty commits, commit secrets, commit build artifacts, or include unrelated user changes.
- Push immediately after each commit with `git push origin main`. A step is not complete until the push succeeds.

## Push failures

- Do not begin the next implementation step while the current commit is unpushed.
- For an ordinary non-fast-forward rejection in a clean worktree, fetch and rebase onto `origin/main`, rerun relevant checks, then retry the push.
- If authentication, authorization, or merge conflicts block the push, stop and report the problem instead of forcing or guessing.

