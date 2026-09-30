-- repo-infra: man-lua v1
--
-- Turn "- `term`: description" bullet lists into definition lists, for the man
-- writer (D23). build/man.mk runs it.
--
-- This departs on purpose from the mdmost filter it was ported from. mdmost
-- matched a bold term followed by an em dash. The house style has no em dashes
-- anywhere (owner ruling, 2026-09-23), so a term here is an inline code span
-- followed directly by a colon, which is plain ASCII Markdown.
--
-- docs/manual.md has two readers. The man writer emits .TP, the hanging indent
-- every man page uses for options, only for a definition list. GitHub Flavored
-- Markdown has no definition lists, so the source is an ordinary bullet list,
-- which GitHub renders properly, and this filter rewrites it before the man
-- writer sees it. The rewrite works on the document tree, not on the generated
-- roff: bullet lists emit .IP too (as ".IP \[bu] 2"), so replacing .IP by .TP
-- in the output would flatten every real bullet in the page.
--
-- A list converts only when every item's first paragraph opens with an inline
-- code span followed directly by ":". A space before the colon does not match,
-- and neither does a bullet that merely mentions code. One item of another
-- shape leaves the whole list alone, so a list is never half converted. The
-- term is the code span set in bold, and the colon and the space after it are
-- dropped from the definition.
--
-- repo-infra owns this file. Change the asset in repo-infra, not this copy.

-- Split an item's inlines into (term, description) at the colon after a
-- leading code span. Returns nil when the item does not have that shape.
local function split_term(inlines)
  if #inlines < 2 or inlines[1].t ~= "Code" then
    return nil
  end
  local after = inlines[2]
  if after.t ~= "Str" or after.text:sub(1, 1) ~= ":" then
    return nil
  end
  local description = {}
  local rest = after.text:sub(2)
  if rest ~= "" then
    description[1] = pandoc.Str(rest)
  end
  for j = 3, #inlines do
    description[#description + 1] = inlines[j]
  end
  -- Drop the space that followed the colon, so the definition starts at a word.
  if description[1] and (description[1].t == "Space" or description[1].t == "SoftBreak") then
    table.remove(description, 1)
  end
  return { pandoc.Strong({ inlines[1] }) }, description
end

function BulletList(el)
  local items = {}
  for _, blocks in ipairs(el.content) do
    if #blocks == 0 or (blocks[1].t ~= "Para" and blocks[1].t ~= "Plain") then
      return nil -- not our shape; leave the list untouched
    end
    local term, description = split_term(blocks[1].content)
    if not term then
      return nil
    end
    local definition = { pandoc.Para(description) }
    for k = 2, #blocks do
      definition[#definition + 1] = blocks[k]
    end
    items[#items + 1] = { term, { definition } }
  end
  return pandoc.DefinitionList(items)
end
