# User manual: one full release journey

This manual walks one concrete person through one concrete job: **you** are shipping a version of
the desktop app you maintain — from signing in to holding all four platforms' artifacts. Every step
says three things: what you do, what you see, and what happens underneath; the last sections are
"where to look when it is stuck".

> Assumptions: a GitHub account; a repository with (or willing to have) GitHub Actions; a Cargo
> project inside it. The examples use this repository itself: `ixmoyren/github-action-console`.
> Vocabulary lives in [CONTEXT.md](../CONTEXT.md).

---

## 0. Build it and start it

```bash
cargo run
```

On Linux you also need the window-system and font development libraries (see
[README](../README.en.md#quick-start)). The first start:

- creates a SQLite file under the system data directory (`github-action-console/console.sqlite`:
  `~/.local/share/…` on Linux, `~/Library/Application Support/…` on macOS, `%APPDATA%\…` on
  Windows) to remember the repository you had open;
- logs the three facts about this binary (version, build target, packaging config). You can ask
  without opening a window:

```bash
cargo run -- --build-info
```

Downloads land in the **system Downloads directory** (macOS `~/Downloads`, the Windows Downloads
known folder, the XDG `XDG_DOWNLOAD_DIR` on Linux); when the system has nothing to say, the console
falls back to `<data dir>/github-action-console/downloads`.

For a proxy or a corporate network: the gear (Settings) in the corner of the sign-in page opens a
small window; enter the proxy there and save. That entry lives on the sign-in page, so changing it
after signing in means signing out first.

---

## 1. Sign in

Two ways, both on the sign-in page:

**A. Device Flow (recommended)**

1. Click "使用 OAuth 登录" (Sign in with OAuth).
2. Paste the **client id** of your own OAuth App (GitHub → Settings → Developer settings → OAuth
   Apps; it needs no callback URL — Device Flow does not use one).
3. The console asks GitHub for a one-time code and shows it together with the verification address:
   copy the code ("复制代码"), open the address ("打开验证地址"), type the code and authorize.
4. The screen waits ("等待授权中…") and shows how long the code stays valid. Once authorized you
   land on the repository list.

**B. Personal Access Token (fallback)**

Click "使用 Personal Access Token 登录", paste the token (classic: `repo` + `workflow`;
fine-grained: Contents, Actions and Workflows read/write). If the scopes are short, the status bar
says exactly what is missing.

**Underneath**: the token goes into the OS keyring only (macOS Keychain / Windows Credential
Manager / Linux Secret Service). Restarting the app keeps you signed in; the "登出" (sign out)
button clears it.

---

## 2. Pick a repository

- Filter by name ("按名称搜索仓库"), sort by last update or last push, page with "加载更多" and
  re-read the first page with "刷新".
- Each row: name, visibility (私有/公开), default branch, latest commit and its date.

Clicking a name opens its **workspace**. "← 返回仓库列表" goes back; the console remembers the
repository and returns to it on the next start.

---

## 3. Find the workflow that releases

The left column lists the repository's workflows; the right side shows the selected one:

- **An existing workflow** (say `.github/workflows/ci.yml`): click it and the YAML opens in a
  highlighted editor. "保存" commits it — "已保存并提交到默认分支。" means it wrote **a commit to
  the default branch**, not a pull request.
- **No release flow yet**: click "新建发布流" and the editor holds this repository's release
  template ([templates/github/workflows/release-target.yml](../templates/github/workflows/release-target.yml));
  look it over, adjust, then "保存" to write it as `.github/workflows/release-target.yml`.

To make the template useful as is, add a release manifest at `.github/release-console.yml` (format:
[README](../README.en.md#the-release-manifest)). Without one the template still runs: it builds the
four built-in targets (macos-arm / macos-intel / windows / linux) once each.

There is also a "新建工作流" form: file name, display name, runner OS and version, triggers (manual / push / pull_request / schedule), an optional container, and any number of jobs (id / name / command). "预览" shows the generated YAML; "推送" commits it to the default branch.
Handy for standing up a small CI from nothing. The form also carries "使用发布模板" (adopt the
release template): no form to fill, it writes the release flow into the repository — the same text
as "新建发布流 → 保存" above.

---

## 4. Run it once by hand (optional, recommended)

Click the ▶ next to a workflow: the console dispatches it on the default branch and opens the run
list drawer so you can watch it.

The drawer is a table: status / workflow / conclusion / branch / event / time, sortable by header,
filterable by branch ("按分支过滤"). The rightmost column offers, per row:

- still running → "取消" (cancel; jobs that have not finished stop with it);
- finished → "删除" (delete; logs and artifacts go with it).

A fresh run goes 排队中 → 进行中 → 已完成, and the console polls while you stay on the page.

---

## 5. Read one run: jobs, steps, logs, artifacts

Click the workflow name in a run row to open the **run detail**:

- **Jobs**: one block per job, with its steps shown in an editor — line numbers, selectable,
  copyable, one step per line (number, name, conclusion). "查看日志" reads that job's raw log.
- **Logs**: "在日志中搜索" filters, "复制日志" copies the filtered text to the clipboard, and
  "下载运行日志包" downloads the whole run's log zip.
- **构建产物 (build artifacts)**: what this run produced, each with a "下载" button.
- **发布事实 (release facts)**: this run's own inputs (version / target / config), whether it
  finished, what is downloadable, and whether anything was recorded as published.
- "在浏览器打开" jumps to the run on GitHub; "返回运行列表" goes back to the table.

---

## 6. Grab artifacts straight from the run list

You do not have to open the detail page. In the run table, **the status "已完成" is itself a
button** (tooltip: "点开看这次运行产出的构建产物。"). Clicking it:

1. asks GitHub what this run produced;
2. if there really are downloadable artifacts, a detail block expands **inside the table, right
   under that row**, listing each artifact's name, size (expired ones are marked) and a "下载"
   button; if there are none, the status bar says "这次运行没有可下载的构建产物。" and nothing opens;
3. click "下载": the button turns into a **progress bar + 取消**, and the status bar says
   "文件正在写入 <absolute path> 中";
4. once it finishes, the bar and the cancel button disappear and the row shows
   "文件已经保存到 <absolute path> 中" followed by a fresh "下载" button; a toast on the right
   names the same path;
5. changing your mind mid-flight: "取消" aborts the request — no half-written file is left behind.

---

## 7. The release board: ship with a channel tag

Click "← 发布看板" in the header. Here "publishing" stopped being "record something in the console"
and became **creating a tag in the repository**.

**What the board shows**

- Rows: the **release targets** from the manifest (this repository: web-arm / web-intel / windows /
  mas / linux-tar); targets with simulated steps are marked "模拟".
- Columns: the three **channel tags** — LTS / latest / dogfood.
- A cell: where that target stands in this tag's run — 待构建 / 构建中 / 已发布 / 失败 / 已取消 (a target with simulated steps stops at 待签名公证), with the claimed job's name below it and a
  "查看运行" button. With no run for that tag it says "还没有这次 tag 的运行".

**Shipping once**

1. **分支**: pick one of the repository's branches (read once when you enter the repository).
2. **提交**: changing the branch loads its recent commits; the dropdown shows `short sha · subject`.
   You may skip it — the branch's latest commit is the default.
3. **版本**: the dropdown holds the three channels (lts / latest / dogfood). Pick the one you are
   shipping to.
4. Click "创建". The console points that tag at the commit you picked — if the tag already exists it **moves** it (channel tags are meant to move: "what latest points at" is literally where the
   `latest` tag is).
5. The status bar says "已把通道 tag 指到这个提交。" and the console refreshes the run list.

**What happens after that push**

The tag is a push event, and it triggers the release workflow (the template listens with
`on.push.tags: ['**']` — any tag). Inside, `plan` expands the manifest into a matrix (one entry per
release target), each platform builds and packages, and the artifacts are attached to the Release of **the tag that triggered the run**.

The board claims that run by tag (for a tag push, `head_branch` is the tag name), reads its jobs and
matches them to the rows by job name — `target · platform/arch · artifact`. So you watch the
windows / macOS / Linux rows go from 构建中 to 已发布 or 失败; "查看运行" opens the run detail with
logs and artifacts.

**Where the version number comes from**

A channel tag is not a version. The workflow's rule: a tag that looks like a version (`v1.2.3`,
`1.2.3`) is used as one, otherwise the version comes from `Cargo.toml`. A `latest` build therefore
still produces `github-action-console-0.1.0-windows.msi`, attached to the `latest` Release;
Windows Installer gets its own legal `x.x.x.x` (`0.1.0.0`).

---

## 8. When it is stuck

**A tag was created but no run appeared**

Check, in order:

1. does `.github/workflows/release-target.yml` **at the tagged commit** listen for that tag? (The
   template now says `on.push.tags: ['**']`; older commits may still say `v*`.)
2. a push event fires when the tag is **created or moved** — an existing tag never triggers
   retroactively; move it onto a newer commit to fire another push;
3. are Actions enabled for the repository, and is the workflow on the default branch where GitHub
   can see it?

**"凭据权限不足" in the status bar**

The message names what is missing: `repo` + `workflow` for a classic PAT, or Contents / Actions /
Workflows read-write for a fine-grained one. Fix the token, sign out, sign in again.

**Rate limiting**

The status bar always shows "限流余量：N（reset time）". When you do hit the limit, the notice tells
you to retry later; wait for the reset.

**MSI / dmg / pkg packaging failed**

That belongs to CI; the console only watches. Open the run, find the failing job, read its log. Two
classic traps on this path — the Windows Installer code page (`LGHT0311`) and the 64-bit component
rule (`ICE80`) — are both written up in
[issue 04](../.scratch/multi-platform-release/issues/04-ci-build-chain.md).

**Signing, notarization, store submission**

There are no certificates or accounts here, so those steps are always marked **simulated**
(`::warning::[SIMULATED] …`) and a target with simulated steps stops at 待签名公证 — it never claims
已发布. That is by design, not a failure.

**Cannot sign in on Linux / credentials keep going invalid**

The token lives in the Secret Service (gnome-keyring / KWallet and friends). Minimal desktops and
containers may not have it; either install and start a keyring daemon, or sign in every time.

**Does a big download freeze the UI?**

No: downloads run on a background task, the UI stays responsive, the progress bar moves and you can
cancel. But be aware: GitHub's artifact download has no streaming API, so the whole file is read
into memory before it is written — a few hundred MB will cost that much memory.

---

## 9. What you make GitHub do

Writes are limited to these (everything else is read-only):

| Action                                                        | What it does on GitHub                                                      |
|---------------------------------------------------------------|-----------------------------------------------------------------------------|
| Saving a workflow / pushing a new one / adopting the template | writes **a commit to the default branch** (one `.github/workflows/*.yml`)   |
| ▶ next to a workflow                                          | one `workflow_dispatch`                                                     |
| "创建" on the board                                           | creates a tag (`POST /git/refs`); an existing tag is moved (`PATCH`, force) |
| "取消" / "删除" in the run list                               | `POST /actions/runs/{id}/cancel`, `DELETE /actions/runs/{id}`               |
| Downloading artifacts / log archives / release assets         | read-only                                                                   |

---

## 10. Read further

- Vocabulary: [CONTEXT.md](../CONTEXT.md)
- Decisions: [docs/adr](../docs/adr) — especially 0001 (trigger and observe, never build), 0003 (the manifest is the source of truth), 0007 (channels are tags)
- Specs and tickets: `.scratch/`
- This repository's own release flow:
  [.github/workflows/release-target.yml](../.github/workflows/release-target.yml) and
  [.github/release-console.yml](../.github/release-console.yml)
