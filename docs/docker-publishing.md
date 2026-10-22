# Docker Hub publishing

The `Release` GitHub Actions workflow publishes `ohmywrt/sempre` and
`ohmywrt/sempre-proxy` through the reusable Docker workflow. The server image
contains `sempre-server` and its Web UI, with no proxy core. PostgreSQL runs
separately.

## Repository configuration

Create the public `ohmywrt/sempre` and `ohmywrt/sempre-proxy` repositories on Docker Hub. In the GitHub
repository's Actions secrets, set:

| Secret | Value |
| --- | --- |
| `DOCKERHUB_USERNAME` | `ohmywrt` |
| `DOCKERHUB_TOKEN` | A Docker Hub personal access token with Read & Write permission |

Use a dedicated publishing token without Delete permission. Replace the secret
before the token expires. Never commit the token to this repository.

## Release triggers and tags

Push an existing-style `v*` release tag to trigger publishing. Version tags must
have the form `vX.Y.Z` or `vX.Y.Z-prerelease`, for example `v2.0.20-beta.12`.
Desktop and container builds run in parallel with validation under this one
release entry point. No image or installer is published until every required
check and build succeeds.

| Git tag | Docker Hub tags |
| --- | --- |
| `v2.0.20` | `ohmywrt/sempre:v2.0.20`, `ohmywrt/sempre:latest` |
| `v2.0.20-beta.12` | `ohmywrt/sempre:v2.0.20-beta.12`, `ohmywrt/sempre:beta` |

The same version and channel tags apply to `ohmywrt/sempre-proxy`. Ordinary
branch pushes do not publish images. The Docker workflow has no independent
tag trigger or manual publishing input.

AMD64 and ARM64 images build on native GitHub-hosted runners. The server image
must pass database migration, health, subscription-publishing, and core-free
process checks. Builds export OCI archives with provenance; runtime checks load
the image from that exact archive. After the release gate succeeds, the workflow
uploads the archived images by digest without rebuilding, preserves their
provenance, then publishes the version and channel manifests. BuildKit caches
are separate for each image and architecture.

## Checks, results, and retries

- **CI** runs on pull requests, main pushes, and manual requests. Shared changes
  run the complete platform matrix; site-only changes run site checks. Explicit
  documentation-only changes skip product checks. Manual CI runs validate all
  scopes. Website changes deploy main after the required checks pass.
- **Release** reuses complete successful CI evidence only for the exact commit,
  matching workflow definitions, and trusted main runs. It waits for an existing
  run, runs complete checks if evidence is missing, and blocks on failed or
  cancelled CI. Every release also runs a fresh dependency audit. Builds run in
  parallel with this validation.
- **Maintenance** runs the weekly Mihomo integration and dependency audit, with
  a manual entry point for diagnosis.

GitHub Release and Docker publish in parallel after the gate. Installers are
marked available as soon as GitHub Release succeeds; Pages then refreshes the
download manifests using current main website sources. The final summary lists
each channel separately, including skipped, cancelled, and failed states. A
partially published release is not reported as complete. Account-level GitHub
notification preferences remain separate from these workflow summaries.

Use GitHub Actions **Re-run failed jobs** on the original run to retry a failed
channel after investigating its cause. Required validation failures must first
be rerun successfully; they are never bypassed. Image archives are retained for
seven days, so an expired archive requires rebuilding through the release gate.
Rerunning a historical release can move `latest` or `beta` back to that version.
Do not rerun successful publishing jobs just to retry a different channel.

## Running the image

Create a PostgreSQL database and configure `DATABASE_URL`, `SEMPRE_PUBLIC_URL`,
and `SEMPRE_BIND=0.0.0.0:8787` for the container. Set `SEMPRE_PUBLIC_URL` to the
externally reachable base URL, since it is used in subscription links.
Publish container port 8787 to the desired host port.

Before the first start, explicitly initialize the database with the same image
and database configuration:

```sh
docker run --rm --env DATABASE_URL ohmywrt/sempre:v2.0.20-beta.12 migrate
```

The `migrate` command uses the `DATABASE_URL` exported in the host shell; ensure
the database hostname is reachable from the container, adding the appropriate
Docker network when PostgreSQL runs in another container. Run the image without
arguments to start the service. Database migration is an explicit operation;
pulling or publishing an image does not change a deployed database.
