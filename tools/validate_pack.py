#!/usr/bin/env python3
"""Check design-pack coverage and preserved source bytes, not product behavior."""
from pathlib import Path
import hashlib
import json
import re

root = Path(__file__).resolve().parents[1]
req_text = (root / 'docs/REQUIREMENTS.md').read_text(encoding='utf-8')
req_rows = re.findall(r'^\| (REQ-[A-Z]+-\d+) \| (Required|Deferred|Proposed) \| (WP-\d+) \|', req_text, re.M)
assert req_rows, 'missing requirements'
ids = {r[0] for r in req_rows}
assert len(ids) == len(req_rows), 'duplicate requirement IDs'
tasks = (root / 'TASKS.md').read_text(encoding='utf-8')
task_sections = dict(re.findall(r'^## (WP-\d+) [^\n]*\n(.*?)(?=^## |\Z)', tasks, re.M | re.S))
accept = (root / 'docs/verification/ACCEPTANCE_REGISTER.md').read_text(encoding='utf-8')
for rid, _scope, wp in req_rows:
    assert rid in task_sections.get(wp, ''), (rid, wp)
    assert f'| AT-{rid[4:]} | {rid} |' in accept, rid
for match in re.finditer(r'\]\(([^)]+)\)', (root / 'START_HERE.md').read_text(encoding='utf-8')):
    path = match.group(1).split('#')[0]
    assert (root / path).is_file(), path
manifest = json.loads((root / 'docs/reference/GO_ARCHIVE_MANIFEST.json').read_text(encoding='utf-8'))
for f in manifest['files']:
    data = (root / 'original' / f['path']).read_bytes()
    assert len(data) == f['bytes'] and hashlib.sha256(data).hexdigest() == f['sha256'], f['path']
local_count = 0
for f in manifest.get('local_only', []):
    path = root / 'original' / f['path']
    if path.is_file():
        data = path.read_bytes()
        assert len(data) == f['bytes'] and hashlib.sha256(data).hexdigest() == f['sha256'], f['path']
        local_count += 1
print(f'PASS: {len(ids)} requirements, {len({r[2] for r in req_rows})} work packages, '
      f'{len(manifest["files"])} archive files and {local_count} local-only references verified. '
      'START_HERE links resolve.')
print('Planning checks only. No application, compatibility, proof or runtime tests executed by this tool.')
