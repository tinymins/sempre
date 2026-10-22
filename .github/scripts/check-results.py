"""Enforce selected jobs; only complete validation may produce release evidence."""
import hashlib
import json
import os
from pathlib import Path


def fingerprint():
    digest = hashlib.sha256()
    for directory in ['.github/workflows', '.github/scripts']:
        for path in sorted(Path(directory).glob('*')):
            if path.is_file():
                digest.update(str(path).encode())
                digest.update(path.read_bytes())
    return digest.hexdigest()


if __name__ == '__main__':
    results = {name: job['result'] for name, job in json.loads(os.environ['RESULTS']).items()}
    selected = {
        'rust': ['rust', 'format', 'transparent-netns', 'service', 'server-container', 'vulnerability'],
        'frontend': ['frontend'],
        'site': ['site'],
    }
    required = [job for scope, jobs in selected.items()
                if os.environ[f'CHECK_{scope.upper()}'] == 'true' for job in jobs]
    if os.environ['CHECK_AUDIT'] != 'true' and 'vulnerability' in required:
        required.remove('vulnerability')
    expected = {job for jobs in selected.values() for job in jobs}
    valid = set(results) == expected and all(
        result == ('success' if job in required else 'skipped')
        for job, result in results.items()
    )
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
        print(f'## Required checks — {os.environ["GITHUB_SHA"]}\n', file=summary)
        print('| Check | Result | Required |\n| --- | --- | --- |', file=summary)
        for job, result in results.items():
            print(f'| {job} | {result} | {job in required} |', file=summary)
        print(f'\n[Run logs]({os.environ["GITHUB_SERVER_URL"]}/{os.environ["GITHUB_REPOSITORY"]}'
              f'/actions/runs/{os.environ["GITHUB_RUN_ID"]})', file=summary)
    if not valid:
        raise SystemExit('Required checks failed, were cancelled, or unexpectedly skipped')
    if set(required) == expected:
        proof = {'sha': os.environ['GITHUB_SHA'], 'fingerprint': fingerprint(), 'results': results}
        (Path(os.environ['RUNNER_TEMP']) / 'complete-ci.json').write_text(json.dumps(proof))
