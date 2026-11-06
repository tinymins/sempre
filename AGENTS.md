# Project Context and Stack

Sempre is a cross-platform Rust service and CLI that manages proxy cores, generated configurations, persisted state, native services, and the authenticated Web control plane on Windows, Linux, `OpenWrt`, and macOS. The same workspace also contains a core-free multi-user subscription server.

- Rust 1.95 or newer implements the CLI, daemon, shared converter, multi-user server, core lifecycle, platform integration, and build tooling.
- Bun 1.3.14 drives repository orchestration and the frontend toolchains.
- `ui/` is the official React, TypeScript, Vite, and Tailwind CSS v4 control UI.
- `site/` is an independent TypeScript and Vite static website; it does not use the React component library or control-plane application architecture.

## Architecture and Ownership

- `rust/crates/sempre-client` owns the production CLI, local daemon, authenticated Web API, and platform orchestration.
- `rust/crates/sempre-manager` owns core lifecycle, supervision, deployment rollback, and subscription rules shared by the CLI and API. Put shared rules here, not in `sempre-client`.
- `sempre-manager/src/fetch_proxy.rs` owns client subscription proxies, and `fetch_proxy_rules.rs` owns the proxy rules for Sempre's own downloads. Proxies run only for a download. A request whose chosen proxy is unavailable fails and never connects directly.
- `rust/crates/sempre-service` owns native service integration: systemd, procd, launchd, and Windows services.
- `rust/crates/sempre-transparent` owns host capture: nftables, policy routing, system DNS, and `OpenWrt` gateway capture.
- `rust/crates/sempre-network` owns passive host inventory and routing. `rust/crates/sempre-diagnostics` owns active probes and traces, so the gateway path has no HTTP client.
- `rust/crates/sempre-control-plane` owns Web API configuration, authentication, and endpoints. `rust/crates/sempre-core-control` is the client for a running core's own API.
- `rust/crates/sempre-http` is the only place that builds an HTTP client, because it installs the TLS provider.
- `rust/crates/sempre-converter` owns pure subscription parsing and configuration compilation. It must remain free of HTTP, persistence, environment, and process-management side effects.
- `rust/crates/sempre-server` owns the core-free multi-user service, authentication, persistence, fetching, published artifacts, memberships, and shares. It must never install or start a proxy core.
- `rust/crates/sempre-state` owns local filesystem layout and persistent state. `rust/crates/sempre-build` owns release artifact assembly.
- `ui/` consumes the versioned control-plane API and owns authenticated control experiences. `site/` is public presentation and must remain independent of daemon state and APIs.
- Keep canonical state and configuration ownership at the existing domain boundary. Do not duplicate business rules across the CLI, API, UI, or individual core adapters.
- `sempre_state::openwrt_host()` is the only `OpenWrt` check, and `Layout::openwrt` carries its result. On `OpenWrt`, Sempre maintains only sing-box and its routing rules: remote profiles deploy the published configuration unmodified apart from the local dashboard path and its download URL, Sempre downloads that dashboard through the proxy rules, default download URLs name the original sites rather than mirrors because the proxy rules choose the route, local profiles compile to the same TProxy shape, capture rules outlive core restarts, and `sempre-manager/src/openwrt_limits.rs` refuses features that would manage more of the host. Every installed file, including the executable and core, lives under `/etc/sempre`, runtime files live in `/tmp/sempre`, and only the init script and firewall include are registered with the system.
- For platform-sensitive changes, inspect the Windows, Linux, macOS, Unix, and fallback implementations that share the affected contract. Do not make a single-platform fix that silently breaks another supported target.

## UI Boundaries

- In the React application under `ui/`, business code must use an existing control from `@acme/components` instead of rendering or styling a native replacement.
- Native control implementations belong only in `ui/src/components/acme`. If the library lacks a required control, add the smallest reusable implementation there before using it in a page or feature.
- Use Tailwind CSS v4 for new `ui/` styling while preserving global styles, theme tokens, keyframes, third-party overrides, and CSS that Tailwind cannot express cleanly.
- The standalone static site under `site/` does not load the React component library and is outside these component and Tailwind rules.

## Approvals

These rules also bind sub-agents, scripts, and tools; delegation never bypasses them.

- Before modifying files, data, configuration, or services, propose a concrete plan with material assumptions and tradeoffs, then wait for explicit confirmation. Analysis-only requests stay read-only. After confirmation, finish the confirmed scope without re-asking; re-confirm only when the scope or design changes materially.
- Get explicit approval for: a new branch (exact name and starting ref); a new worktree (purpose, exact path, and starting ref); the first SSH or other remote connection to a host, except routine `git fetch`, `pull`, and `push` transport; any backup copy; new test cases; and compatibility logic for old versions, interfaces, or data formats.
- Never push, tag, or trigger GitHub Actions unless explicitly requested.

## Implementation Rules

- Published migration files are immutable, including comments, imports, and formatting. Never put test modules, test functions, fixtures, or test-only imports inside a migration file. Put tests in separate files and register new migration checksums in `rust/crates/sempre-state/tests/migration_policy.rs`; behavioral changes require a new migration. Do not update frozen checksums to make an existing migration edit pass validation.
- Prefer the smallest root-cause solution that follows the existing architecture; if a local fix would weaken a boundary, propose the cleaner design first. Add abstractions only for real domain boundaries, meaningful duplication, or safer changes, and add no speculative features, configuration, flexibility, or impossible error handling.
- Every changed line must trace to the requested outcome: touch only required files and lines, match the existing style, and remove imports, variables, functions, and files made unused by the change. Report unrelated dead code and adjacent bugs without repairing, building, deploying, or releasing them.

## Code Organization

- Keep every handwritten source file at or below 500 physical lines. This limit includes handwritten tests and applies to Rust, TypeScript, TSX, JavaScript, JSX, CSS, and other handwritten source formats.
- Generated files, lockfiles, build artifacts, and third-party code are excluded from the 500-line limit.
- Existing files above 500 lines are grandfathered but may only shrink. A change must not increase their physical line count.
- When a task would otherwise grow an oversized file, place the new responsibility in a cohesive module instead of extending the existing file. Do not perform unrelated splitting or refactoring.

## Discovery and Tool Efficiency

- Begin with the existing feature owner and its tests. Use at most two targeted repository-search batches before selecting the implementation boundary; after that, open only files needed by the next edit or verification decision.
- Keep output bounded with a narrower `rg`, line range, or test name, and batch related reads and status checks instead of repeating unchanged polls. Reuse existing update-task, API, and UI contracts before designing new layers, and stop exploring adjacent release, deployment, or runtime behavior once the requested path is proven.

## Verification

- Define verifiable success criteria before implementation and use the narrowest meaningful checks while iterating.
- Verify behavior with existing automated tests, focused checks, or repeatable steps. Do not add test cases by default, including new test files or modules, test functions, table rows, fixtures, or snapshots; only when the user asks, or existing verification cannot prove the result, state the need, scope, and count and get approval first. Never delete, skip, or weaken existing tests to make them pass.
- For Rust changes, default to named test modules or filters plus affected-target Clippy/checks; touching a crate does not by itself require every test in that crate. Run the full crate only when the change alters a shared crate-wide contract or no reliable focused filter exists.
- For `ui/` or `site/`, run the relevant Vitest files and the owning package's lint/type check. Do not expand from component tests to unrelated page or browser suites without a behavior-specific reason.
- Before reporting a completed code change, run repository `bun run lint` and `bun run tsc` once on the final code state. These are final static gates, not commands to repeat after every edit.
- Run full `bun run rust:lint` and `bun run rust:test` only when the change crosses shared Rust contracts, workspace configuration, foundational crates, or multiple independently owned crates, or when the user explicitly requests full local validation. When a commit, tag, or push is meant to trigger GitHub Actions, CI owns the full matrix; do not delay dispatch for a redundant local run unless the user requested a local artifact.
- Run a full local `bun run build` only when the user requested a local release artifact or the final artifact itself is the acceptance surface. For build-tooling or archive changes, prefer focused packaging tests when they prove the changed contract. Inspect the build entry point first and do not invoke a wrapper that repeats already completed verification.
- For user-visible UI changes, verify the rendered layout and interaction in a browser when available, but only after confirming the exact Sempre dev server, owning process, working directory, and served source revision. Otherwise accept focused component/DOM tests and report that limitation; do not probe unrelated ports or installed stale UI.
- If a full suite produces no task-relevant progress for five minutes, or waits on another task's build lock, stop and report or move it to an isolated worktree/CI rather than repeatedly polling it.
- Documentation-only changes require content review and `git diff --check`; do not run product builds or test suites for them.

## Concurrency and Git

- Start modifying tasks by recording the base commit and checking the current branch, worktree status, tracked remote ref, and task-owned files. Multiple agents may work concurrently: never modify unrelated files, and when another task's uncommitted changes are present, use an existing approved worktree or request approval for a new one.
- Do not run full-workspace Cargo commands in a shared dirty checkout. They validate other tasks' code, contend on build locks, and can turn unrelated failures into false blockers.
- Treat a request to commit, push, tag, or trigger GitHub Actions as a delivery request, and resolve the branch, next tag, workflow trigger, and remote transport near the start.
- Before advancing shared `main`, compare the recorded base, current `main`, and `origin/main`. If either ref advanced, keep the task commit isolated and rebuild a linear result on the latest base in an approved worktree, or report the pending integration. Never report a commit as integrated while `main...origin/main` is diverged; state `ahead`/`behind` explicitly.
- Validate, then end every completed task with a local commit containing exactly its own changes; never leave finished work uncommitted for another agent or a later turn. If validation fails, do not commit; report the failure and leave the changes for correction.
- Split independent requests or independently verifiable changes into separate commits. In a shared worktree, stage only task-owned files and leave parallel-task changes for their owners to commit.
- Use English Conventional Commit titles in the form `<type>(<scope>): <outcome>`, for example `feat(core): add runtime reload support`. Use a concise lowercase scope and a specific English outcome. Supported types are `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`, and `revert`.

## Development Servers

- Use the repository-level `bun start` process for local development. It runs the Rust development daemon through Cargo Watch and both frontend projects through Vite.
- Do not restart running backend or frontend development processes after code changes. Cargo Watch rebuilds and restarts the Rust process, while the `ui/` and `site/` Vite servers use HMR.
