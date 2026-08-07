# AGENTS.md

## Rules for AI agents and contributors

Always run these checks before finishing any task that touches Rust code,
and before creating a commit:

```sh
npm run fmt:rust   # cargo fmt --check (bắt buộc)
npm run lint:rust  # cargo clippy -D warnings
npm run test:rust  # chạy fmt check + cargo test
```

Use the npm scripts above instead of calling `cargo` directly. Direct `cargo`
calls bypass the formatting rule.

## Enforcement

- `npm run fmt:rust` fail nếu code không pass `rustfmt` (dùng trong npm scripts).
- Pre-commit hook (husky) chạy `npm run fmt:rust` tự động trước mỗi commit.
- `npm run build:app` và `npm run test:rust` chạy `fmt:rust` trước khi build/test.

Note: require `cargo` in `PATH` (`~/.cargo/bin` or via `rustup`).