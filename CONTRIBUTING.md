# Contributing to Fyr

Thanks for contributing.
This guide is intentionally short and points to canonical docs for details.

## 1. Before You Start

Prerequisites:
- Rust 1.70+
- Node.js 18+ (CI uses Node 20)
- npm 9+

Helpful references:
- Project overview and validation commands: [README.md](README.md)
- Development governance and module map: [AGENTS.md](AGENTS.md)
- Full dev workflow and release process: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)

## 2. Branch and PR Flow

- Use `dev` as the integration branch.
- Open feature PRs into `dev`.
- Promote with a PR from `dev` to `main`.
- Stable releases are tagged from `main` as `vX.Y.Z`.

## 3. Required Validation Before PR

Run from repo root:

```bash
cargo test --workspace --all-targets
cargo check -p server
(cd crates/ui/frontend && npm ci && npm run build)
(cd docs/build && npm ci && npm run build && npm run verify:kiwix)
```

## 4. Documentation Update Rules

When behavior changes, update docs in the same change:
- User-facing behavior: [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- Technical behavior: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)
- Onboarding/quickstart changes: [README.md](README.md)

Canonical docs are limited to:
- [README.md](README.md)
- [AGENTS.md](AGENTS.md)
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)

## 5. Scope and Licensing Notes

- Keep Fyr naming consistent (`Fyr`, `FYR_HOST`, `FYR_PORT`).
- Respect module ownership in [AGENTS.md](AGENTS.md).
- Source code is MIT licensed (see [LICENSE](LICENSE)).
- Embedded Kiwix assets in `public/kiwix-static/` retain upstream copyleft licenses.
