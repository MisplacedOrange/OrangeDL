# Contributing to OrangeDL

Patches and contributions are welcome. Please follow the guidelines below.

## Getting Started

1. Fork the repository and clone your fork locally.
2. Install dependencies with `npm ci`.
3. Run the app in development mode with `npm run tauri dev`.
4. Create a branch for your change: `git checkout -b my-feature`.

## Code Review

All submissions require review. We use GitHub pull requests for this purpose.

## Pull Request Etiquette

- Write clear commit messages that describe what changed and why.
- Keep PRs focused — one logical change per PR makes review faster.
- Reference any related issue numbers in the PR description.
- Ensure `cargo clippy` and `cargo fmt` pass before opening a PR.

## Reporting Issues

When filing a bug, include:
- Your OS version and build type (installer vs. source build).
- Steps to reproduce the issue.
- Any relevant console output or log snippets.
