# Docker Hub publishing

The `Docker Hub` GitHub Actions workflow publishes `ohmywrt/sempre`. The image
contains `sempre-server` and its Web UI, with no proxy core. PostgreSQL runs
separately.

## Repository configuration

Create the public `ohmywrt/sempre` repository on Docker Hub. In the GitHub
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
The desktop release workflow continues to run independently on the same tag.

| Git tag | Docker Hub tags |
| --- | --- |
| `v2.0.20` | `ohmywrt/sempre:v2.0.20`, `ohmywrt/sempre:latest` |
| `v2.0.20-beta.12` | `ohmywrt/sempre:v2.0.20-beta.12`, `ohmywrt/sempre:beta` |

To publish an existing tag manually, run the `Docker Hub` workflow and supply
its exact name in the `tag` input. This builds that tag's commit and updates its
channel tag, so rerunning an older release also moves `latest` or `beta` back to
that release. Container checks come from the workflow run's commit, so corrections
to the publishing checks also apply when republishing an older release tag.
The image source remains that release tag's commit. Ordinary branch pushes do
not publish images.

AMD64 and ARM64 images build on native GitHub-hosted runners. Each image must
pass database migration, health, subscription-publishing, and core-free process
checks before its digest is uploaded. Only after both architectures succeed
does the workflow publish the version and channel manifests. BuildKit caches
are separate for each architecture; the upload reuses the verified build.

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
