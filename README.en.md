# GitHub Action Console

A cross-platform desktop console: sign in to GitHub, pick a repository, watch workflows and runs,
trigger builds, read logs, grab build artifacts, and move a version along the LTS / latest /
dogfood channels with **channel tags**.

> Status: early (0.1.0). The UI copy is Chinese.
> 中文版见 [README.md](README.md); a step-by-step walkthrough lives in the
> [user manual](docs/user-manual.en.md).

## What it puts in one window

Shipping a desktop app used to mean juggling three disconnected places: Actions for runs and logs,
Releases for artifacts and download links, and your own memory (or a spreadsheet) for which version
each channel currently points at. This console connects that path:

- **Sign-in and credentials**: GitHub Device Flow (your own OAuth App client id), with a Personal
  Access Token as fallback. The token goes into the OS keyring only (macOS Keychain / Windows
  Credential Manager / Linux Secret Service); restarts stay signed in, signing out clears it.
- **Repositories**: your repositories (private ones included), searchable by name, sortable by last
  update or last push, loaded page by page.
- **Workflows**: list a repository's workflows, open one to read (and edit) its YAML with syntax
  highlighting, save to commit it to the default branch; plus a form that generates a workflow and a
  one-click "adopt the release template" entry.
- **Runs**: a run list (status / conclusion / branch / event / time, sortable, filterable by branch,
  cancellable while running, deletable once done); a run's detail shows every job's steps, the raw
  logs (searchable, copyable), its build artifacts (downloadable) and what that run produced.
- **Release board**: rows are the release targets from the manifest, columns are the channel tags (LTS / latest / dogfood). Pick a branch → pick a commit → pick a version (channel) → **Create**,
  and the console creates (or moves) the tag of that name in the repository. Pushing the tag starts
  the release workflow; the board claims that run by tag and shows, row by row, how each windows /
  macOS / Linux job is doing.
- **Downloads**: build artifacts, run-log archives and release assets land in the system's Downloads
  directory, with a progress bar and a cancel button while they run, and a toast afterwards that
  names the **absolute path** the file was saved to.
- **Facts you can always see**: the status bar shows the signed-in user and the GitHub rate-limit
  budget; `--build-info` reports this binary's own version, build target and packaging config (CI's
  headless self-check reads the same text).

## What it does not do

- **It builds nothing and uploads nothing** ([ADR-0001](docs/adr/0001-ci-builds-and-uploads-release-assets.md)).
  Building, packaging and uploading belong to GitHub Actions; the console triggers and observes.
- Signing, notarization and App Store submission have no certificates or accounts here — they are
  labelled **simulated** and never reported as done.
- It is not a general GitHub client: it deals with this repository's workflows, runs, artifacts and
  channels.
- It does not decide your release policy: which channel points at which commit is a click you make.
- This repository does not ship a license file yet.

## Quick start

```bash
cargo run                     # open the window
cargo run -- --build-info     # no window: just say which build this is
cargo test --all --workspace  # tests
```

Requirements: Rust (edition 2024). On Linux you also need the window-system and font development
libraries; CI installs:

```bash
sudo apt-get install -y --no-install-recommends \
  pkg-config clang libclang-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-dev libxcb1-dev libxcb-randr0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxcb-xkb-dev libfontconfig1-dev libssl-dev
```

Signing in needs GitHub credentials, one of:

- **Device Flow**: the client id of your own OAuth App (typed once on the sign-in page).
- **PAT**: a classic token with `repo` + `workflow`, or a fine-grained token with Contents, Actions
  and Workflows read/write.

Proxy or intranet: the gear (Settings) in the corner of the **sign-in page** takes a proxy address —
it lives on that page, so changing it after signing in means signing out first.

## The release manifest

Which release targets a repository has and how each one is packaged comes from
`.github/release-console.yml` at the repository root ([ADR-0003](docs/adr/0003-repo-release-manifest-is-source-of-truth.md)). This repository's own copy:

```yaml
version: 1

targets:
  windows:
    platform: windows          # macos / windows / linux
    arch: x64                  # arm64 / x64 / universal
    distribution: github-releases
    packaging: windows-msi     # names a config under packaging
    simulated:                 # optional: these steps are simulated here
      - code-signing

packaging:
  windows-msi:
    workflow: release-target.yml
    inputs:
      package: msi             # dmg / msi / pkg / tar.gz
```

A repository without a manifest still works: the console falls back to four built-in targets (macos-arm / macos-intel / windows / linux). Target names may contain letters, digits, dots and
dashes only.

## The release workflow template

[templates/github/workflows/release-target.yml](templates/github/workflows/release-target.yml) is
this repository's release script *and* the text the console writes into other repositories when you
adopt the template (`include_str!` of the same file,
[ADR-0006](docs/adr/0006-the-build-script-is-the-template.md)). In short:

- **Two triggers**: `workflow_dispatch` (pick target / version / packaging config by hand) and
  `push: tags: ['**']` (any tag: channel tags and version tags alike).
- **One manifest, one matrix**: `plan` reads the manifest and `Cargo.toml` and emits one entry per
  release target, with `fail-fast: false`.
- **Version rule**: a dispatch input wins; a tag that looks like a version (`v1.2.3` / `1.2.3`) is
  used as one, otherwise the version comes from `Cargo.toml` (so a build from the `latest` channel
  tag still produces `…-0.1.0-…` files). Windows Installer gets a separate legal `x.x.x.x`.
- **Artifacts**: per target, `cargo build --release` → a `--build-info` self-check → packaging (dmg / msi / unsigned pkg / tar.gz / zip) → `upload-artifact`.
- **Release assets**: on a tag push they are attached to the Release of **the tag that triggered the
  run** (a channel tag's assets are the build that channel currently points at).
- Workflow name, target names and artifact names all come from the manifest and Cargo.toml, so
  another Cargo repository can use it as is.

## Repository layout

```
src/app/       use cases: auth, repositories, workspace, runs, downloads, release board (no GPUI)
src/github/    the GitHubGateway port and its octocrab adapter (the single seam)
src/store.rs   SQLite: the remembered repository and local caches
src/ui/        GPUI Kit views: shell / repositories / workspace / run_detail / downloads / status
src/release.rs the release model: manifest parsing, state derivation (pure functions)
src/app_info.rs which build this is: version / build target / packaging config
templates/     the release workflow template (the one this repository uses too)
packaging/     per-platform packaging files (e.g. the WiX definition for Windows)
docs/          ADRs and the user manual; CONTEXT.md is the glossary, .scratch/ the specs
```

## Development

- Tests: `cargo test --all --workspace` (pure functions; the app layer against scripted fake
  gateways; the UI rendered headlessly through `gpui_kit::test`).
- [CONTEXT.md](CONTEXT.md) is the vocabulary; decisions worth remembering go into
  [docs/adr](docs/adr).
- Specs and tickets live in `.scratch/`; read the matching `spec.md` before picking up a thread.

## Known edges

- The board does not show *which commit* a channel tag points at (only each target's job state).
- Downloads are not streamed: the whole artifact is read into memory before it is written, which is
  why there is a progress bar and a cancel button — large files still pass through memory.
- The local channel-pointer machinery (ADR-0005) is still in the code but unused by the UI; it is
  being removed now that channels are tags.
- The window no longer shows the app's own version / target / packaging config; use `--build-info`
  or the startup log.
