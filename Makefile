# The manual is authored in docs/manual.md and converted to roff on demand.
# man/ is gitignored: a generated file that is not in version control cannot
# disagree with its source, so there is nothing to keep in step and no staleness
# gate to maintain. CI runs this same target, so there is exactly one code path.
.PHONY: man

man: man/mdmost.1

man/mdmost.1: docs/manual.md
	@mkdir -p man
	pandoc --standalone --to man docs/manual.md -o man/mdmost.1
