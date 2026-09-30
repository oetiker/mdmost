# The manual is authored in docs/manual.md and converted to roff on demand by
# build/man.mk (owned by repo-infra). man/ is gitignored: a generated file that
# is not in version control cannot disagree with its source. CI runs this same
# target, so there is exactly one code path.
MAN_NAME = mdmost
include build/man.mk
