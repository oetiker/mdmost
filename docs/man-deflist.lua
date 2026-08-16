--- Turn "term — description" bullets into a definition list, for the man writer.
---
--- # Why this exists
---
--- `docs/manual.md` has two audiences with incompatible tastes. A pandoc definition
--- list is what makes the man writer emit `.TP`, the hanging indent every man page
--- uses for options and keys — but GitHub Flavored Markdown has no definition lists
--- at all, so on the web every entry renders as a paragraph followed by a paragraph
--- starting with a literal `:`. That is the whole reference half of the manual, and
--- the web is where the manual claims its audience.
---
--- So the source is written as an ordinary bullet list, which GitHub and mdmost both
--- render properly, and this filter rewrites it to a definition list before the man
--- writer sees it. The transformation is on the document AST, not on generated roff:
--- a text substitution cannot work, because bullet lists emit `.IP` too (as
--- `.IP \[bu] 2`) and a blanket `.IP` → `.TP` would flatten every real bullet in the
--- document.
---
--- # What it matches
---
--- A bullet list where **every** item begins `**term** — `. The em dash immediately
--- after the bold run is the discriminator, and it is deliberately a narrow one:
--- ordinary prose bullets that merely start with a bold phrase — of which this manual
--- has two, under RENDERING — do not match and are left as bullets. One item that
--- fails the shape leaves the whole list alone, so a list is never half-converted.

--- Splits an item's inlines into (term, description) at the em dash after the term.
--- Returns nil when the item is not of the "**term** — description" shape.
local function split_term(inlines)
  if #inlines == 0 or inlines[1].t ~= "Strong" then
    return nil
  end
  -- The separator sits within a couple of inlines of the term: Strong, Space, Str.
  for i = 2, math.min(#inlines, 3) do
    if inlines[i].t == "Str" and inlines[i].text == "\u{2014}" then
      local description = {}
      for j = i + 1, #inlines do
        description[#description + 1] = inlines[j]
      end
      -- Drop the space that followed the dash, so the definition starts at a word.
      if description[1] and description[1].t == "Space" then
        table.remove(description, 1)
      end
      return inlines[1].content, description
    end
  end
  return nil
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
