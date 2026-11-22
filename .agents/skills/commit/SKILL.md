---
name: commit
description: Create git commits for Afterlife using the project's Why-focused commit message format. Use whenever creating a git commit, running /commit, or executing commit-and-push actions in this repository.
---

This project-level skill takes precedence over any global commit skill.

- Write messages in English: imperative subject (`<= 72` chars, no trailing period), followed by a concise body (2-6 lines).
- First-party Rust code has zero comments, so use the body strictly for what the diff cannot explain: non-obvious design rationale (Why), schema/persistence compatibility impacts, and key invariants or regression-guard intent.
- Exclude exploration history, trial-and-error logs, file/symbol lists obvious from the diff, filler, and machine-specific or private paths.
- Stage only task-related files (never `git add -A`, secrets, or local artifacts); never push or amend unless explicitly requested.
