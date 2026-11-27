#!/usr/bin/env python3
"""Check design-pack coverage and preserved source bytes, not product behavior."""
from pathlib import Path
import hashlib
import json
import re

root = Path(__file__).resolve().parents[3]
req_text = (root / 'docs/REQUIREMENTS.md').read_text(encoding='utf-8')
req_rows = re.findall(r'^\| (REQ-[A-Z]+-\d+) \| (Required|Deferred|Proposed) \| (WP-\d+) \|', req_text, re.M)
assert req_rows, 'missing requirements'
ids = {r[0] for r in req_rows}
assert len(ids) == len(req_rows), 'duplicate requirement IDs'
tasks = (root / 'docs/work-packages.md').read_text(encoding='utf-8')
task_sections = dict(re.findall(r'^## (WP-\d+) [^\n]*\n(.*?)(?=^## |\Z)', tasks, re.M | re.S))
accept = (root / 'docs/verification/ACCEPTANCE_REGISTER.md').read_text(encoding='utf-8')
for rid, _scope, wp in req_rows:
    assert rid in task_sections.get(wp, ''), (rid, wp)
    assert f'| AT-{rid[4:]} | {rid} |' in accept, rid
manifest = json.loads((root / 'docs/reference/GO_ARCHIVE_MANIFEST.json').read_text(encoding='utf-8'))
for f in manifest['files']:
    data = (root / 'original' / f['path']).read_bytes()
    assert len(data) == f['bytes'] and hashlib.sha256(data).hexdigest() == f['sha256'], f['path']
print(f'PASS: {len(ids)} requirements, {len({r[2] for r in req_rows})} work packages, '
      f'{len(manifest["files"])} archived Go files verified.')
