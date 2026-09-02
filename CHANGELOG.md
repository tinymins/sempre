# Changelog

All notable changes to Sempre are documented in this file.

## [2.0.6] - 2026-09-02

### Fixed

- Updated Windows managed DNS regression coverage to use the required sing-box 1.14 fixture, restoring full Windows CI verification without changing production behavior.

## [2.0.5] - 2026-09-02

### Added

- Added a device-level managed DNS frontend with domain-aware routing, health reporting, and transactional DNS takeover on macOS and Windows.
- Added explicit pending-change contracts, startup state migrations, and a global restart control that summarizes changes before applying them.
- Added sortable connection and data tables, selectable traffic-history ranges, service and core memory metrics, and local DNS results in network diagnostics.
- Added a dedicated routing-rule editor, streamlined proxy selection, and reorganized navigation for the local control plane.
- Added a complete management workspace for the standalone multi-user server while keeping its proxy-core lifecycle boundary intact.

### Changed

- Routed private CIDRs through the desktop TUN path and rebuilt generated core configurations when private-route settings change.
- Separated DNS frontend policy from gateway mode and kept device DNS policy independent from profile-owned core DNS settings.
- Separated traffic-statistics windows from retention policy and made historical ranges selectable without changing persisted retention.
- Expanded the English and Chinese architecture documentation with matching DNS, routing, and LAN gateway diagrams.

### Fixed

- Kept managed DNS available during macOS core recovery and waited for launchd service removal before completing uninstall operations.
- Kept the Windows UDP DNS frontend listening and routed managed DNS correctly through the Windows TUN frontend.
- Restored cross-platform DNS lint and Linux transparent-network checks, including distinct TProxy and redirect assertions.
- Preserved proxy-group order, Unicode values in JSONC editor fields, and profile ownership of core DNS settings.
- Restricted standalone-server custom-node updates to their owners.
- Restored proxy-node card layout, standardized empty states, and hid unavailable proxy providers.
- Improved IP attribution by preferring public-IP checks and exposing cached attribution in diagnostics.
- Made equal-total traffic-history rows deterministic across repeated queries and test runs.

[2.0.6]: https://github.com/tinymins/sempre/compare/v2.0.5...v2.0.6
[2.0.5]: https://github.com/tinymins/sempre/compare/v2.0.4...v2.0.5

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
