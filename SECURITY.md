# Security Policy

## Reporting

Please do not publish a suspected vulnerability before maintainers have had a
reasonable opportunity to investigate it. Report security issues through the
[private vulnerability reporting form](https://github.com/tinymins/sempre/security/advisories/new).

## Sensitive Data

Subscription URLs may contain access tokens. Sempre stores them in its
restricted state file and redacts URL paths and query parameters from normal
status output and logs. Bug reports must not include `state.json` from the
system data directory, portable `.sempre/state.json`, or unredacted command
output containing a subscription URL.

Deployment bundles contain a full portable snapshot for another machine:
subscription catalogs, cached/generated configurations, managed core binaries,
and the installed custom UI. Treat exported bundle directories and ZIP files as
sensitive artifacts. The Web administrator password hash is intentionally
cleared during bundle export, so installed bundles start with an empty Web
password.

## Trust Boundary

Sempre downloads proxy core archives only from the core adapter's official
release source and requires a SHA-256 release digest. Subscription
configurations are untrusted input and must pass the selected core's validation
command before activation.

System-mode data and the installed service executable are writable only by
administrators and the operating-system service account. Portable mode trusts
the directory containing the downloaded executable and is intended only when
the user explicitly opts into that deployment model.
