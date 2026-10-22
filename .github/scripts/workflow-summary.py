"""Report channel outcomes and fail closed on required jobs, including skips."""
import json
import os

jobs = json.loads(os.environ['RESULTS'])
optional = set(filter(None, os.environ.get('OPTIONAL', '').split(',')))
valid = all(job['result'] == 'success' or (name in optional and job['result'] == 'skipped')
            for name, job in jobs.items())
with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
    print(f'## {os.environ["TITLE"]}: {"success" if valid else "incomplete / failed"}\n', file=summary)
    print(f'Commit: `{os.environ["GITHUB_SHA"]}`\n', file=summary)
    print('| Stage / channel | Result |\n| --- | --- |', file=summary)
    for name, job in jobs.items():
        print(f'| {name} | {job["result"]} |', file=summary)
    if os.environ['TITLE'] == 'Release' and jobs.get('publish', {}).get('result') == 'success':
        print('\nGitHub installers are available; other channels have the states shown above.', file=summary)
    print(f'\n[Logs]({os.environ["GITHUB_SERVER_URL"]}/{os.environ["GITHUB_REPOSITORY"]}'
          f'/actions/runs/{os.environ["GITHUB_RUN_ID"]})', file=summary)
if not valid:
    raise SystemExit('Required stages did not all succeed; see the summary for partial completion')
