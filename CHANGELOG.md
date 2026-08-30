# Changelog

All notable changes to Sempre are documented in this file.

## [2.0.4] - 2026-08-30

### Added

- Added automatic refresh scheduling, diagnostics, and durable last-known-good published artifacts for the standalone multi-user subscription server.
- Added operational hardening for authentication, refresh concurrency, profile revisions, and container deployment.
- Added converter equivalence coverage for migrated OhMyWrt Toolbox subscriptions.

### Changed

- Made the multi-user server UI available at the default route and kept its runtime isolated from local proxy-core lifecycle management.
- Normalized generated Clash and sing-box fields to match the established Toolbox output contract.
- Compile and validate a fresh configuration for the candidate core version before switching an installed core.

### Fixed

- Preserved unaffected published artifacts when an individual subscription refresh fails.
- Omitted unsupported AnyTLS nodes from sing-box 1.11 output.
- Fixed sing-box 1.14 upgrades reusing a configuration compiled for an older core version.

[2.0.4]: https://github.com/tinymins/sempre/compare/v2.0.3...v2.0.4
