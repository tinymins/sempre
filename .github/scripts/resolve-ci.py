"""Reuse only successful, complete CI evidence from this repository and exact SHA."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('check_results', Path(__file__).with_name('check-results.py'))
checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checks)
sha = os.environ['GITHUB_SHA']
repository = os.environ['GITHUB_REPOSITORY']


def api(path):
    return json.loads(subprocess.check_output(['gh', 'api', path], text=True))


# A main push and its tag may arrive together. Allow the CI event to be registered.
runs = []
for attempt in range(4):
    data = api(f'repos/{repository}/actions/workflows/ci.yml/runs?head_sha={sha}&per_page=100')
    runs = [run for run in data['workflow_runs'] if run['head_sha'] == sha
            and run['event'] in ('push', 'workflow_dispatch')
            and run['head_branch'] == 'main'
            and run['head_repository']['full_name'] == repository]
    if runs or attempt == 3:
        break
    time.sleep(20)
reuse = False
if runs:
    run = max(runs, key=lambda candidate: candidate['id'])
    deadline = time.monotonic() + 60 * 60
    result = None
    while True:
        # Query the current attempt: a failed attempt must not borrow an earlier
        # successful result. Website deployment is deliberately outside this gate.
        run = api(f'repos/{repository}/actions/runs/{run["id"]}')
        jobs = api(f'repos/{repository}/actions/runs/{run["id"]}'
                   f'/attempts/{run["run_attempt"]}/jobs?per_page=100')['jobs']
        matching = [job for job in jobs if job['name'] == 'Checks / Required checks result']
        if len(matching) > 1:
            raise SystemExit('CI contains ambiguous required-check result jobs')
        result = matching[0] if matching else None
        if result and result['status'] == 'completed':
            if result['conclusion'] != 'success':
                raise SystemExit(f'Required CI checks failed, were cancelled, or skipped: {result["html_url"]}')
            break
        if run['status'] == 'completed':
            if run['conclusion'] != 'success':
                raise SystemExit(f'CI failed before completing required checks: {run["html_url"]}')
            break
        if time.monotonic() >= deadline:
            raise SystemExit(f'CI checks {run["html_url"]} did not finish within one hour')
        time.sleep(60)
    name = f'complete-ci-{sha}-{run["run_attempt"]}'
    artifacts = api(f'repos/{repository}/actions/runs/{run["id"]}/artifacts?per_page=100')['artifacts']
    if result and result['conclusion'] == 'success' and any(artifact['name'] == name and not artifact['expired'] for artifact in artifacts):
        destination = Path(os.environ['RUNNER_TEMP']) / f'ci-proof-{run["id"]}'
        subprocess.run(['gh', 'run', 'download', str(run['id']), '--repo', repository,
                        '--name', name, '--dir', str(destination)], check=True)
        proof = json.loads((destination / 'complete-ci.json').read_text())
        expected = {'rust', 'frontend', 'server-container', 'vulnerability'}
        reuse = (proof.get('sha') == sha and proof.get('fingerprint') == checks.fingerprint()
                 and set(proof.get('results', {})) == expected
                 and all(result == 'success' for result in proof['results'].values()))
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
        print(f'CI: [{run["id"]}]({run["html_url"]}); complete evidence reused: {reuse}', file=summary)
with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
    print(f'reuse={str(reuse).lower()}', file=output)
