# Integrations

Configuration fragments that make **mdmost** the thing that opens a Markdown file.

One of them a package can install for you; the other two it cannot. The desktop
entry belongs to the operating system, so the deb and the rpm install it system-wide
and every user on the machine gets it. The terminal fragments belong to a terminal,
and no terminal here has a drop-in directory a package could use — WezTerm and kitty
each read one file owned by you — so those are yours to copy.

| Fragment | deb, rpm | Homebrew, tarball |
| -------- | -------- | ----------------- |
| `xdg/mdmost.desktop` | installed to `/usr/share/applications` | copy it yourself |
| `wezterm/md-open.lua` | example in `/usr/share/doc/mdmost/examples` | copy it yourself |
| `kitty/open-actions.conf` | example in `/usr/share/doc/mdmost/examples` | copy it yourself |

Homebrew keeps all three under `$(brew --prefix mdmost)/share/mdmost/integrations`.
It installs no desktop entry: on macOS there is no such mechanism to install into,
and on Linux its prefix is not on the desktop session's `XDG_DATA_DIRS`, so a file
manager would never look there.

`mdmost`(1), under **DEFAULT MARKDOWN VIEWER**, explains what each of these does and
why the two mechanisms involved are not interchangeable.

## What is here

- `wezterm/md-open.lua` — Clicking a `file://…md` link in WezTerm opens it in a new
  window running mdmost. Copy it next to your `wezterm.lua` and add
  `require 'md-open'`; WezTerm's `package.path` already covers that directory.

- `kitty/open-actions.conf` — The same, for kitty. Copy it to
  `~/.config/kitty/open-actions.conf`, or append the stanza to the file you have.

- `xdg/mdmost.desktop` — The Linux desktop file association, for double-clicking a
  `.md` file in a file manager. Installing it only offers mdmost as a choice; to make
  it the one a double-click uses:

  ```sh
  xdg-mime default mdmost.desktop text/markdown
  ```

  Installing from Homebrew or the tarball instead? Copy it to
  `~/.local/share/applications/`, run `update-desktop-database` on that directory,
  then the command above.

Two terminals are deliberately absent. iTerm2 has no per-scheme hook — its Semantic
History fires on any Cmd-clicked filename, so it needs a dispatch script of your own
rather than a fragment. Terminal.app offers nothing to configure at all.

## Set the path to mdmost

Every fragment has to name the binary. WezTerm and kitty both execute it directly
instead of going through a shell, so it is looked up on the *terminal's* `PATH` — and
a terminal launched from the macOS Dock inherits a bare `PATH` with no Homebrew
prefix in it. The Lua module searches the usual prefixes; the kitty stanza has the
path written in, so change it to the output of:

```sh
command -v mdmost
```
