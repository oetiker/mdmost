-- Open Markdown file:// links in a new WezTerm window running mdmost.
--
-- Copy this file next to your wezterm.lua -- WezTerm's package.path already
-- covers that directory -- and require it:
--
--   require 'md-open'
--
-- Requiring it is what installs the handler; there is nothing else to call.
--
-- This needs nothing from the operating system. OpenLinkAtMouseCursor emits the
-- `open-uri` event BEFORE it hands the URI to the OS opener, and returning false
-- from the handler suppresses that hand-off. So there is no Launch Services
-- registration on macOS, no application bundle, and no xdg-mime default on Linux:
-- the terminal that draws the link is also the one that opens it, and these same
-- lines behave identically on both platforms.
local wezterm = require 'wezterm'

local M = {}

-- mdmost has to be named by absolute path. SpawnCommand executes argv directly
-- rather than through a shell, so it resolves against WezTerm's own PATH -- and a
-- WezTerm started from the macOS Dock inherits the bare launchd PATH, which has no
-- Homebrew prefix in it. Put `command -v mdmost` at the front of this list if it
-- lives somewhere else; the bare name at the end keeps the handler working wherever
-- PATH does happen to be right.
local CANDIDATES = {
  '/opt/homebrew/bin/mdmost', -- Homebrew, Apple Silicon
  '/usr/local/bin/mdmost',    -- Homebrew, Intel
  '/usr/bin/mdmost',          -- deb, rpm
  '/bin/mdmost',
}

-- io.open rather than wezterm.glob. glob is an async function: it yields, so
-- calling it from a module chunk raises "attempt to yield across a C-call
-- boundary" and every candidate silently looks absent -- which is
-- indistinguishable from mdmost not being installed. io.open is plain Lua and
-- works in any context.
local function exists(path)
  local handle = io.open(path, 'r')
  if handle then
    handle:close()
    return true
  end
  return false
end

local function first_present(paths)
  for _, path in ipairs(paths) do
    if exists(path) then
      return path
    end
  end
  return 'mdmost'
end

local MDMOST = first_present(CANDIDATES)

local MARKDOWN_EXT = {
  md = true,
  markdown = true,
  mkd = true,
  mdown = true,
}

-- Turns file:///a/b.md into /a/b.md, and returns nil for every URI that is not a
-- Markdown file on this machine.
local function md_path(uri)
  local rest = uri:match '^file://(.*)$'
  if not rest then
    return nil
  end

  -- Strip a fragment or query before decoding. A '#' or '?' that genuinely belongs
  -- to a filename arrives percent-encoded, so anything literal at this point is a
  -- URI delimiter rather than a character to keep.
  rest = rest:gsub('[#?].*$', '')

  -- An empty authority and 'localhost' both mean this machine. A real hostname
  -- means someone else's filesystem, where there is no local file to page.
  local host, path = rest:match '^([^/]*)(/.*)$'
  if not path or (host ~= '' and host ~= 'localhost') then
    return nil
  end

  path = path:gsub('%%(%x%x)', function(hex)
    return string.char(tonumber(hex, 16))
  end)

  local ext = path:match '%.(%a+)$'
  if not ext or not MARKDOWN_EXT[ext:lower()] then
    return nil
  end
  return path
end

wezterm.on('open-uri', function(window, pane, uri)
  local path = md_path(uri)
  if not path then
    -- Not ours. Returning no value leaves WezTerm to open it as it always would,
    -- which is what keeps http and https going to the browser.
    return
  end

  -- Run in the document's own directory, so relative links and images resolve the
  -- way they do for anything else reading the file.
  local dir = path:match '^(.*)/[^/]*$'
  if dir == nil or dir == '' then
    dir = '/'
  end

  window:perform_action(
    wezterm.action.SpawnCommandInNewWindow {
      args = { MDMOST, path },
      cwd = dir,
    },
    pane
  )
  return false
end)

-- Exposed so the URI matching can be exercised without a GUI.
M.md_path = md_path
M.mdmost = MDMOST

return M
