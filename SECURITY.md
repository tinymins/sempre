# Security Policy

## Reporting

Please do not publish a suspected vulnerability before maintainers have had a
reasonable opportunity to investigate it. Report security issues through the
[private vulnerability reporting form](https://github.com/sempre-lab/sempre/security/advisories/new).

## Sensitive Data

Subscription URLs may contain access tokens. Sempre stores them in its
restricted state file and redacts URL paths and query parameters from normal
status output and logs. Bug reports must not include `.sempre/state.json` or
unredacted command output containing a subscription URL.

## Trust Boundary

Sempre downloads proxy core archives only from the core adapter's official
release source and requires a SHA-256 release digest. Subscription
configurations are untrusted input and must pass the selected core's validation
command before activation.
