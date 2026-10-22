"""Choose conservative validation scopes from the event's exact change range."""
import json
import os
import subprocess

with open(os.environ['GITHUB_EVENT_PATH'], encoding='utf-8') as source:
    event = json.load(source)
full = os.environ['GITHUB_EVENT_NAME'] == 'workflow_dispatch'
if 'pull_request' in event:
    base = event['pull_request']['base']['sha']
    head = event['pull_request']['head']['sha']
    revision = f'{base}...{head}'
else:
    base = event.get('before', '')
    head = os.environ['GITHUB_SHA']
    revision = f'{base}..{head}'
    full = full or not base or set(base) == {'0'}
files = [] if full else subprocess.check_output(
    ['git', 'diff', '--name-only', '-z', revision], text=True
).split('\0')
rust = frontend = full
deploy = full
for path in filter(None, files):
    deploy = deploy or path.startswith(('site/', 'installer/', '.github/release-notes/')) or path == '.github/workflows/pages.yml'
    if path.startswith(('site/', '.github/release-notes/')):
        frontend = True
    elif path.startswith('docs/') or path in ('README.md', 'README.zh-CN.md', 'AGENTS.md', 'LICENSE'):
        pass
    else:
        # Installer, embedded UI, Rust, workflows, scripts, locks and unknown files
        # affect shared contracts. Run a complete set that releases can reuse.
        rust = frontend = True
with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
    for name, enabled in [('rust', rust), ('frontend', frontend), ('deploy', deploy)]:
        print(f'{name}={str(enabled).lower()}', file=output)
