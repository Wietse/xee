# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Xee is an XML Execution Engine in Rust: an almost-complete XPath 3.1 implementation, a partial XSLT 3.0 implementation, and a CLI tool (`xee`). XPath and XSLT share a single bytecode interpreter.

This repository (`Wietse/xee`) is an **independent fork** of `Paligo/xee`, maintained as the XPath engine behind `xbrlstd-rs` (`../xbrlstd-rs`). See "Fork status" below.

## Workspace layout

Cargo workspace; member crates are declared in the root `Cargo.toml`. The compilation pipeline flows through them:

```
xee-xpath-lexer  →  xee-xpath-ast  →  xee-xpath-compiler  →  xee-ir  →  xee-interpreter (bytecode VM)
                                                                ↑
                xee-xslt-ast  →  xee-xslt-compiler  ────────────┘
```

- `xee-xpath` — public Rust API for XPath. Start here for end-user usage.
- `xee-interpreter` — VM + XPath standard library (`src/library/`). The actual function implementations live here.
- `xee-xpath-macros` — the `#[xpath_fn(...)]` macro used to register library functions (modeled after pyo3).
- `xee-xpath-type`, `xee-schema-type`, `xee-name` — type/name support shared across crates.
- `xee-xpath-load` — loaders used by the test runner.
- `xee` — the CLI binary (XPath REPL, pretty-print, run queries).
- `xee-testrunner` — runs the W3C QT3 (XPath) and XSLT conformance suites vendored under `vendor/`.

External companion projects (not in this repo): `xot` (XML tree), `regexml` (regex), `xee-php` (PHP bindings).

## Common commands

Build, lint, test (CI enforces all three; `cargo clippy` runs with `-D warnings`):

```
cargo fmt --check --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Run a single Rust test:

```
cargo test -p xee-xpath <test_name>
```

The `./check` script runs `cargo test` plus both conformance suites in debug mode (slow, but catches bounds-check panics release mode hides).

## Conformance test workflow

The conformance suites are the primary regression safety net — over 20,000 XPath tests. The runner lives in `xee-testrunner` and is invoked from that directory:

```
# Regression check — runs only tests known to pass; CI also runs this (in debug mode).
cargo run --release -- check ../vendor/xpath-tests/

# Run all supported tests, including ones currently filtered out as failing.
cargo run --release -- all ../vendor/xpath-tests/

# After implementing a feature/fix: update the filter to include newly-passing tests,
# then re-run `check` to confirm Failed/Error/WrongE are all 0.
cargo run --release -- update ../vendor/xpath-tests/

# Zoom in on a specific test file or name substring.
cargo run --release -- -v all ../vendor/xpath-tests/fn/node-name.xml fn-node-name-1
```

The same commands work with `../vendor/xslt-tests/` for XSLT (XSLT runner is less complete; failures may be test-runner bugs, not implementation bugs).

The `vendor/xpath-tests/filters` file records which currently-failing tests are expected to fail. Do **not** regenerate it from scratch (`initialize` command) unless you have deliberately accepted regressions — diff against the previous version if you do.

CI runs conformance in **debug** mode on purpose: it catches arithmetic overflow / bounds-check panics that release mode silently optimizes away.

## Adding an XPath library function

1. Pick the right module under `xee-interpreter/src/library/` (e.g. `fn_.rs`, `math.rs`, `array.rs`, `map.rs`, `node.rs`, …).
2. Write the function with `#[xpath_fn(...)]` using the exact type signature from the [XPath 3.1 F&O spec](https://www.w3.org/TR/xpath-functions-31/). Example:

   ```rust
   #[xpath_fn("fn:node-name($arg as node()?) as xs:QName?", context_first)]
   ```

   - `context_first` synthesizes the zero-arg overload that uses the context item as `$arg`.
   - To access the interpreter's dynamic/static context or the `xot` tree, declare `context` and/or `interpreter` as the first parameters — the macro injects them.
   - Return `error::Result<T>` if the function may raise an XPath error.
   - The macro `.into()`s the return value to the declared XPath type; no return-type checking.
3. Register it in the module's `static_function_descriptions` via `wrap_xpath_fn!`.
4. If creating a new library module, wire it up in `xee-interpreter/src/library/mod.rs`.
5. Run `cargo test`, then the conformance suite, then `update` the filter and re-`check`.

## Tests for tricky cases

When a conformance failure is hard to debug, write a hand-rolled test in `xee-xpath/tests/` (XPath) or `xee-xslt-compiler/tests/` (XSLT) using the `xee-xpath` API directly — you can construct the context explicitly.

XSLT AST snapshot tests use `insta` in `xee-xslt-ast/tests/snapshot_tests.rs`. The AST→IR translation under test lives in `xee-xslt-compiler/src/ast_ir.rs`.

## Fork status

Since 2026-09-24 this fork no longer tracks upstream (`Paligo/xee`): no upstream PRs, no rebasing onto upstream, no upstream-review constraints. It forked from upstream `200b1e33` ("Fix clippy issues. (#152)"). Upstream fixes can still be cherry-picked when they are worth having.

**Trunk.** `main` is the only working branch. Commit to it directly, or use short-lived topic branches for larger work. Every commit is normal fork history; don't split commits so they can be upstreamed.

**Consumer.** `xbrlstd-rs` depends on this fork by git SHA (`xee-xpath`, `xee-interpreter`, `xee-xpath-macros`, `xee-xpath-ast` in `../xbrlstd-rs/Cargo.toml`, all pinned to the same `rev`). To ship a change, push `main` and bump that `rev` in `xbrlstd-rs`. **Never rewrite pushed history.** A pinned SHA that becomes unreachable breaks `xbrlstd-rs` builds.

**What the fork adds on top of upstream** (details in `git log 200b1e33..`):

- **`#[xpath_fn]` macro hardening.** Bad signatures and arities are compile errors with spans instead of macro panics. Context/interpreter injection uses explicit `#[xpath_context]` / `#[xpath_interpreter]` attributes, with a name-based fallback. Emitted code uses absolute `::xee_interpreter::*` paths, so the macro works from external crates. trybuild UI tests live in `xee-interpreter/tests/ui/`; regenerate the `.stderr` files after intentional message changes with `TRYBUILD=overwrite cargo test -p xee-interpreter --test macro_ui`.
- **Extension-function registry.** `StaticContextBuilder::add_function` / `add_functions` registers host functions next to the built-ins (built-ins take precedence and cannot be shadowed). Signatures use `Q{uri}local` notation. `build()` is fallible. `DynamicContext::user_data::<T>()` holds host state.
- **Typed node values.** `NodeTypedValueProvider` on `DynamicContext`: atomization asks the host for a node's typed value instead of always returning `xs:untypedAtomic`. `NodeNilledProvider` backs `fn:nilled`. xee itself stays schema-unaware; `instance of` deliberately does not consult the provider. The spec is `../xbrlstd-rs/docs/xee-typed-node-values-proposal.md`.
- **Semantics fixes.** NaN ordering comparisons for `xs:double`/`xs:float`. Left-to-right short-circuit `and`/`or`, lowered to conditionals at AST→IR, where upstream evaluates both operands eagerly.
- **Opt-in dialect.** `XPathDialect::XbrlFormula` (via `parse_with_dialect`) adds `INF`/`NaN` `xs:double` literals.
- **Public constructors.** Validated `xs:date`/`xs:time`/`xs:dateTime` atomics, plus `Name` re-exported so hosts can build `xs:QName`s.
- **Benchmarks.** `xee-xpath/benches/xpath.rs`, including `parse` benches that time document construction.

**Design boundary.** Keep XBRL semantics in `xbrlstd-rs` and generic hooks in xee (registry, providers, dialects). This is a design preference, not an upstream constraint. XBRL-specific code may live here when that is clearly the better home. Items still marked `pub #[doc(hidden)]` only for macro support can become proper public API.

**Legacy branches.** The old upstream-oriented branch stack was merged into `main` and deleted on 2026-09-24. The one exception is the remote `macro-hardening` branch, which stays because it is the source branch of the still-open upstream PR [Paligo/xee#151](https://github.com/Paligo/xee/pull/151); deleting it would close that PR. Its commits are already in `main`.

**License.** MIT. Keep `LICENSE-MIT` and `COPYRIGHT` intact.

## CI and tooling

- **Toolchain:** `rust-toolchain.toml` pins the one Rust version; rustup picks it up locally, and CI installs it with `rustup toolchain install --no-self-update`. It moves together with `xbrlstd-rs`'s pin, never ahead of it, because xee is compiled there with that toolchain.
- **Workspace:** `[workspace.package]` holds the edition and shared metadata, and sets `publish = false`. `[workspace.dependencies]` declares every shared dependency, including the internal crates. Members inherit with `{ workspace = true }`, so an upgrade moves one line.
- **Gate:** `.github/workflows/ci.yml` runs on pushes to `main`, on PRs, and by hand (`workflow_dispatch`): fmt, clippy `-D warnings`, build, test, and the XPath conformance `check` in debug mode. `.githooks/pre-push` runs the same gate minus conformance; change the two together. Activate the hook per clone with `git config core.hooksPath .githooks`.
- **Advisories:** `.github/workflows/audit.yml` runs `cargo audit` daily and on lockfile changes. It sits beside the gate, not in it.
- **Upgrades:** follow the rounds in `xbrlstd-rs` (the umbrella `upgrade-tooling` skill). A lockfile step here must also pass a conformance run whose per-test verdicts match those on `main`, since `check` only catches regressions in tests already known to pass.
- **Releases:** none. There is no crates.io publishing; the fork is consumed by git SHA.
