# The manual is authored in docs/manual.md and converted to roff on demand.
# man/ is gitignored: a generated file that is not in version control cannot
# disagree with its source, so there is nothing to keep in step and no staleness
# gate to maintain. CI runs this same target, so there is exactly one code path.
#
# The Lua filter is what lets one source serve both audiences: the manual is
# written as bullet lists, which GitHub renders properly, and the filter turns
# them into a definition list so the man writer emits .TP. See its own header.
.PHONY: man

man: man/mdmost.1

man/mdmost.1: docs/manual.md docs/man-deflist.lua
	@mkdir -p man
	pandoc --standalone --to man --lua-filter docs/man-deflist.lua \
	  docs/manual.md -o man/mdmost.1
