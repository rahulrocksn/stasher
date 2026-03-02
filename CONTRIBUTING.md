# Contributing to Stasher

Thank you for your interest in contributing. This document outlines the process for submitting changes.

---

## Before You Start

- Check the [open issues](https://github.com/rahulrocksn/stasher/issues) to see if your bug or feature is already being tracked.
- For significant changes, open an issue first to discuss the approach before writing code.
- Small fixes (typos, documentation, minor bugs) can go directly to a pull request.

---

## Workflow

1. Fork the repository and create a branch from `main`.
2. Use a descriptive branch name: `feat/your-feature` or `fix/issue-description`.
3. Make your changes. Keep commits focused — one logical change per commit.
4. Write or update tests where applicable.
5. Ensure linting and tests pass before submitting: `cargo clippy && cargo test`.
6. Submit a pull request against `main` with a clear title and description.

---

## Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
feat: add support for X
fix: resolve crash when Y is null
docs: update setup instructions
chore: bump dependency versions
refactor: simplify auth flow
test: add unit tests for Z
```

---

## Code Style

- Avoid unnecessary abstractions. Write code that is direct and easy to follow.
- Do not leave debug logs, commented-out code, or placeholder variables in PRs.
- If adding a dependency, explain why in the PR description.
- All exported functions should be typed (TypeScript projects).

---

## Pull Request Review

- All PRs require at least one review before merging.
- Address requested changes promptly.
- Do not force-push to a PR branch after review has started — it makes re-reviewing harder.

---

## Reporting Bugs

When filing a bug report, include:
- A clear description of the problem
- Steps to reproduce
- Expected vs. actual behavior
- Your environment (OS, Node version, etc.)

---

## Security Issues

Do not report security vulnerabilities in a public issue. See [SECURITY.md](SECURITY.md) for the responsible disclosure process.
