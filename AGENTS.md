# Project Context and Stack

Sempre is a cross-platform Rust service and CLI that manages proxy cores, generated configurations, persisted state, native services, and the authenticated Web control plane on Windows, Linux, and macOS. The same workspace also contains a core-free multi-user subscription server.

- Rust 1.95 or newer implements the CLI, daemon, shared converter, multi-user server, core lifecycle, platform integration, and build tooling.
- Bun 1.3.14 drives repository orchestration and the frontend toolchains.
- `ui/` is the official React, TypeScript, Vite, and Tailwind CSS v4 control UI.
- `site/` is an independent TypeScript and Vite static website; it does not use the React component library or control-plane application architecture.

## Architecture and Ownership

- `rust/crates/sempre-client` owns the production CLI, local daemon, authenticated Web API, core lifecycle, native service integration, and platform orchestration.
- `rust/crates/sempre-converter` owns pure subscription parsing and configuration compilation. It must remain free of HTTP, persistence, environment, and process-management side effects.
- `rust/crates/sempre-server` owns the core-free multi-user service, authentication, persistence, fetching, published artifacts, memberships, and shares. It must never install or start a proxy core.
- `rust/crates/sempre-state` owns local filesystem layout and persistent state. `rust/crates/sempre-build` owns release artifact assembly.
- `ui/` consumes the versioned control-plane API and owns authenticated control experiences. `site/` is public presentation and must remain independent of daemon state and APIs.
- Keep canonical state and configuration ownership at the existing domain boundary. Do not duplicate business rules across the CLI, API, UI, or individual core adapters.
- For platform-sensitive changes, inspect the Windows, Linux, macOS, Unix, and fallback implementations that share the affected contract. Do not make a single-platform fix that silently breaks another supported target.

## UI Boundaries

- In the React application under `ui/`, business code must use an existing control from `@acme/components` instead of rendering or styling a native replacement.
- Native control implementations belong only in `ui/src/components/acme`. If the library lacks a required control, add the smallest reusable implementation there before using it in a page or feature.
- Use Tailwind CSS v4 for new `ui/` styling while preserving global styles, theme tokens, keyframes, third-party overrides, and CSS that Tailwind cannot express cleanly.
- The standalone static site under `site/` does not load the React component library and is outside these component and Tailwind rules.
- Test business behavior with automated tests. For user-visible UI changes, also verify the actual rendered layout and interaction in a browser when that check is available and relevant.

## Implementation Rules

- Surface material assumptions and tradeoffs before implementation. Ask when ambiguity could materially change the result.
- Prefer the smallest root-cause solution that follows the existing architecture. If a local fix would weaken a boundary, propose the cleaner design first.
- Add abstractions only for real domain boundaries, meaningful duplication, or safer changes. Do not add speculative features, configuration, flexibility, or impossible error handling.
- Touch only files and lines required by the task, preserve unrelated work, and match the existing style.
- Remove imports, variables, functions, and files made unused by the change. Report unrelated dead code without modifying it.
- Every changed line must trace to the requested outcome. Multiple agents may be working concurrently, so never modify unrelated files or modules.

## Code Organization

- Keep every handwritten source file at or below 500 physical lines. This limit includes handwritten tests and applies to Rust, TypeScript, TSX, JavaScript, JSX, CSS, and other handwritten source formats.
- Generated files, lockfiles, build artifacts, and third-party code are excluded from the 500-line limit.
- Existing files above 500 lines are grandfathered but may only shrink. A change must not increase their physical line count.
- When a task would otherwise grow an oversized file, place the new responsibility in a cohesive module instead of extending the existing file. Do not perform unrelated splitting or refactoring.

## Verification

- Define verifiable success criteria before implementation and use the narrowest meaningful checks while iterating.
- After every code change, run both `bun run lint` and `bun run tsc` before reporting completion.
- Run focused tests for the affected behavior. For Rust changes, run `bun run rust:lint` and `bun run rust:test`; use platform-native CI for OS-specific service and network behavior.
- Run the relevant Vitest suite for `ui/` or `site/` behavior, and perform browser verification when rendered appearance or interaction is part of the requested outcome.
- Run `bun run build` for release, packaged-resource, export, bundling, or cross-platform artifact changes.
- Documentation-only changes require content review and `git diff --check`; do not run product builds or test suites for them.

## Git Workflow

- Every completed user-requested task must end with a local Git commit containing all changes produced by that task. Do not report a task complete while leaving its finished work uncommitted for another agent or a later turn.
- Split multiple requests or independently verifiable changes into separate commits. Never combine unrelated tasks in one commit, even when they were implemented in the same run.
- In a shared worktree, stage and commit only files owned by the current task. If parallel-task changes appear, identify them from the diff and recent history, leave them untouched, and require the owning task to commit them before it reports completion.
- Validate before committing. If validation fails, do not commit; report the failure and leave the changes available for correction.
- Stage only in-scope files and preserve unrelated worktree changes.
- Use English Conventional Commit titles in the form `<type>(<scope>): <outcome>`, for example `feat(core): add runtime reload support`. Use a concise lowercase scope and a specific English outcome. Supported types are `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`, and `revert`.
- Never push unless explicitly requested.

## Development Servers

- Use the repository-level `bun start` process for local development. It runs the Rust development daemon through Cargo Watch and both frontend projects through Vite.
- Do not restart running backend or frontend development processes after code changes. Cargo Watch rebuilds and restarts the Rust process, while the `ui/` and `site/` Vite servers use HMR.
