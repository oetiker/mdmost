// repo-infra: workflow-lib v5
'use strict';

// The bracketed form, per Keep a Changelog. mdmost used a bare '## Unreleased';
// supporting both would make this a guessing machine, so the odd one out is
// migrated instead.
const UNRELEASED_HEADING = '## [Unreleased]';
const RELEASE_RE = /^## (\d+\.\d+\.\d+) - (\d{4}-\d{2}-\d{2})\s*$/;

// Where a '## ' block ends: at the next '## ', or at the end of the file.
function blockEnd(lines, start) {
  for (let i = start + 1; i < lines.length; i += 1) {
    if (lines[i].startsWith('## ')) return i;
  }
  return lines.length;
}

function findUnreleased(lines) {
  const start = lines.findIndex((l) => l.trim() === UNRELEASED_HEADING);
  if (start === -1) return null;
  return { start, end: blockEnd(lines, start) };
}

function unreleasedBlock(text) {
  const lines = text.split('\n');
  const range = findUnreleased(lines);
  if (!range) {
    throw new Error(`CHANGES.md has no '${UNRELEASED_HEADING}' heading`);
  }
  return lines.slice(range.start + 1, range.end).join('\n');
}

// The changelog gate's decision. A text is null when the file does not exist
// at that commit. A base without the heading counts as an empty block, because
// the pull request that converts a repository onto the standard is the one
// that introduces '## [Unreleased]'; a head without it cannot be judged.
function gateVerdict(baseText, headText) {
  const missing = `CHANGES.md has no '${UNRELEASED_HEADING}' heading; add it and an entry under it.`;
  const blockOf = (text) => (text === null || !findUnreleased(text.split('\n'))
    ? null
    : unreleasedBlock(text));
  const head = blockOf(headText);
  if (head === null) return { ok: false, message: missing };
  const base = blockOf(baseText);
  const unchanged = base === null ? isEmpty(head) : base === head;
  if (unchanged) {
    return {
      ok: false,
      message: `This pull request adds nothing under '${UNRELEASED_HEADING}' in `
        + 'CHANGES.md. Add an entry describing the change, or label the '
        + "pull request 'no-changelog' if it genuinely needs none.",
    };
  }
  return { ok: true, message: 'CHANGES.md [Unreleased] was updated.' };
}

function isEmpty(block) {
  return block
    .split('\n')
    .filter((l) => l.trim() !== '' && !l.trim().startsWith('### '))
    .length === 0;
}

// Split a block into its '### ' subsections, with each body trimmed.
function subsections(block) {
  const out = [];
  let current = null;
  for (const line of block.split('\n')) {
    if (line.trim().startsWith('### ')) {
      current = { heading: line.trim(), body: [] };
      out.push(current);
    } else if (current) {
      current.body.push(line);
    }
  }
  return out.map((s) => ({ heading: s.heading, body: s.body.join('\n').trim() }));
}

function roll(text, version, date) {
  const lines = text.split('\n');
  const range = findUnreleased(lines);
  if (!range) {
    throw new Error(`CHANGES.md has no '${UNRELEASED_HEADING}' heading`);
  }

  const block = lines.slice(range.start + 1, range.end).join('\n');
  // A heading that appears twice (two branches each brought a skeleton) is
  // one subsection in the release, its entries in the order they were written.
  const kept = [];
  for (const s of subsections(block).filter((s) => s.body !== '')) {
    const earlier = kept.find((k) => k.heading === s.heading);
    if (earlier) {
      earlier.body += '\n' + s.body;
    } else {
      kept.push({ ...s });
    }
  }
  if (kept.length === 0) {
    throw new Error(`'${UNRELEASED_HEADING}' is empty - nothing to release`);
  }

  const skeleton = [UNRELEASED_HEADING, '', '### New', '', '### Changed', '', '### Fixed', ''];
  const released = [`## ${version} - ${date}`];
  for (const s of kept) {
    released.push(s.heading, s.body, '');
  }

  return [
    ...lines.slice(0, range.start),
    ...skeleton,
    ...released,
    ...lines.slice(range.end),
  ].join('\n');
}

function latestRelease(text) {
  for (const line of text.split('\n')) {
    const m = RELEASE_RE.exec(line);
    if (m) return { version: m[1], date: m[2] };
  }
  return null;
}

function notesFor(text, version) {
  const lines = text.split('\n');
  const start = lines.findIndex((l) => {
    const m = RELEASE_RE.exec(l);
    return m !== null && m[1] === version;
  });
  if (start === -1) {
    throw new Error(`CHANGES.md has no section for ${version}`);
  }
  return lines.slice(start + 1, blockEnd(lines, start)).join('\n').trim();
}

module.exports = { unreleasedBlock, gateVerdict, isEmpty, roll, latestRelease, notesFor };
