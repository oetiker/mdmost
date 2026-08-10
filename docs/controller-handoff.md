# Controller Handoff — mdmost trunk

> This is the **trunk** worktree's handoff. The live work is not here.
> Run `git worktree list` first. Detail is in git and in the docs named below.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-10   Reason: three branches merged into trunk
Worktree / branch: main checkout `/home/oetiker/checkouts/mdmost` @ `main`

## Read this instead

**The live workstream is `/scratch/oetiker/claude-worktrees/mdmost-provenance` @
`code-provenance`, and its `docs/controller-handoff.md` is the real starter
pack.** Go there. It carries the mission, the lessons, the constraints and the
open questions. This file exists so a session that lands in the trunk checkout
does not mistake it for the current state.

Verify before trusting either file:

```bash
git worktree list
git log --oneline main..code-provenance     # what trunk is missing
git merge-base --is-ancestor code-provenance main && echo "provenance is merged; find its successor"
```

## What trunk contains, as of `7693b00`

Three finished workstreams were merged on 2026-08-10. All four gates green at
that commit — fmt, `clippy --all-targets -- -D warnings`, `cargo test` (959
tests / 30 suites), and `cargo check --target x86_64-pc-windows-msvc`.
**Re-derive; do not inherit.**

- **publishing** — `ci.yml`, `release.yml` for five targets, `LICENSE-MIT`,
  `CHANGES.md`, `man/mdmost.1`, the Homebrew formula and the README install
  section. **One task of that plan is deliberately unexecuted**: the demo
  recording, which needs the copy features still being built on
  `code-provenance`.
- **table-zebra** — the zebra stripe stops at a table's vertical rules; inline
  code is a colour rather than a raised box; a cell past 30 display columns
  earns the table gap rows even with nothing wrapped; **and every block anchors
  at the same left margin, with no centring anywhere.**
- **mermaid-entities** — mermaid labels decode `&lt;` and friends plus
  mermaid's own `#…;` codes; a `;` inside a `|…|` edge label is label text; an
  edge label is drawn once rather than once per rank it crosses.

The three source worktrees (`mdmost-publishing`, `mdmost-tablezebra`,
`mdmost-entities`) are merged, tombstoned and safe to remove.

## The two things trunk must not lose

- **Nothing has ever been pushed and there is no git remote** as of this commit
  (`git remote -v`). Creating `github.com/oetiker/mdmost`, adding
  `CRATES_IO_TOKEN` and granting Actions write permission are the **owner's**
  steps. Both CI workflows are inert until then, so the local
  `cargo check --target x86_64-pc-windows-msvc` is the only Windows detector.
- **The release workflow has never run.** What was rehearsed locally is the
  changelog rewrite, the formula marker rewrite, and the deb/rpm builds. The
  matrix builds, the crates.io publish, the GitHub release and the Homebrew
  commit are unexercised: the first real run is the first test.
