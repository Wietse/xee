# xee

This file is the instruction file for every agent harness working in this repository, and the only copy of its workflow contract. `CLAUDE.md` imports it and adds the notes that apply to Claude Code alone; nothing else lives there.

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

## Orient to the task

The workflow in this file follows `xbrlstd-rs`'s (`../xbrlstd-rs/AGENTS.md`), adapted to a fork that `xbrlstd-rs` pins by SHA and that has no PRs.

- Confirm the working directory, `git status`, branch and HEAD before editing. Other sessions work in this repository through their own worktrees and local branches (`git worktree list`): leave their branches and changes alone.
- xee keeps no plans index, ADR set or debt register of its own. Its records live in `xbrlstd-rs`, the consumer: a decision that binds the fork or the API `xbrlstd-rs` uses is an `xbrlstd-rs` ADR (`../xbrlstd-rs/docs/decisions.md`; ADR24 is the fork itself, ADR54 the typed node values); deferred work and known divergences go in `../xbrlstd-rs/docs/technical-debt.md`, security findings on `../xbrlstd-rs/docs/security-dashboard.md`, and upgrade work into the open `xbrlstd-rs` upgrade round as `xee: …` rows (the umbrella `upgrade-tooling` skill). A commit message is never the only record of a decision or a deferred defect.
- xee is public; `xbrlstd-rs` is not. Commit messages, code comments, tests and test data in xee carry no `xbrlstd-rs` internals or security exposure, and no `xbrlstd-rs` data, such as filings or documents produced from them. A claim about XPath or XSLT semantics rests on the specifications and the suites.
- Carry authorized work through implementation, verification and review, and make routine internal choices yourself. Surface before implementing: a change to the public API `xbrlstd-rs` uses (the four crates under *Consumer* below), and a deliberate departure from a specification. When an `xbrlstd-rs` task needs an xee change, surface it and agree on it first: the fork change is a change of its own.
- Integrating onto `main` and pushing are approved per change, never carried over (*Delivery*).

## Specification work

- Read the normative text before implementing or judging a rule, and cite it from the text, not from memory: XPath 3.1, XQuery and XPath Functions and Operators 3.1 (F&O), the XDM 3.1 data model, Serialization 3.1 and XSLT 3.0 (https://www.w3.org/TR/). F&O 3.1 and Serialization 3.1 are cached in `../xee-json-state/specs/`. Check which version a rule belongs to: F&O 3.1 differs from 1.0 and 3.0 in places, and QT3 marks a test that applies to some versions only with a `spec` dependency (`XP31+`, `XQ10+`, …).
- The vendored suites are the oracle after the specification: an expected result settles a reading the text leaves open. A suite test that contradicts the text is a finding, not a target: it goes or stays in the filter with a `# reason` comment citing the section (*Conformance test workflow*).
- Neither upstream `Paligo/xee`, another processor, nor this fork's own comments and commit messages establish what a specification requires.

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

The conformance suites are the primary regression safety net: over 20,000 XPath tests and about 14,600 XSLT tests. Both are part of the gate. The runner lives in `xee-testrunner` and is invoked from that directory:

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

The `vendor/xpath-tests/filters` and `vendor/xslt-tests/filters` files record which currently-failing tests are expected to fail; they ratchet. `update` only removes tests that now pass and never adds one. Run it in the same commit as the change that made those tests pass, and read the filter diff before committing: each test that leaves a filter is a claim the reviewer checks. A test that starts failing is a regression to fix, not one to filter, with one exception: a test whose expected result contradicts the specification text, which a spec-correct change now fails. Add that test to the filter by hand, with a `# reason` citing the section, and surface it like any other departure from the suite. `update` cannot do it: it leaves a test set's filter untouched while any test fails that the filter does not list. A reason must not contain `#`, since the parser keeps only the text between the first `#` and the next one; `update` keeps the reason. Do **not** regenerate a filter from scratch (`initialize` command) unless Wietse has agreed to accept the regressions — diff against the previous version if you do.

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
5. Run `cargo test`, then both conformance suites, then `update` the filters and re-`check`.

## Tests for tricky cases

When a conformance failure is hard to debug, write a hand-rolled test in `xee-xpath/tests/` (XPath) or `xee-xslt-compiler/tests/` (XSLT) using the `xee-xpath` API directly — you can construct the context explicitly.

XSLT AST snapshot tests use `insta` in `xee-xslt-ast/tests/snapshot_tests.rs`. The AST→IR translation under test lives in `xee-xslt-compiler/src/ast_ir.rs`. Review a snapshot change as carefully as code.

## Verification

Use focused checks while iterating and the full gate before committing a change.

- The gate is CI's (`.github/workflows/ci.yml`): `cargo fmt --check --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `./check` (`cargo test` plus both conformance `check`s, in debug mode). `.githooks/pre-commit` runs all of it on each `git commit` that changes code, and skips a commit whose staged paths are all Markdown; most of its time is the two conformance checks. Activate it per clone (*CI and tooling*). Never bypass a failing gate (`--no-verify`).
- Start a bug fix with a failing regression test: a Rust test (*Tests for tricky cases*) or a suite test that the fix takes out of the filter. For a new behavioural test, show that a representative mutation of the behaviour makes it fail. Run such probes with the umbrella `mutate` tool (`uv run --directory ../xbrlstd-rs/tools mutate --repo <checkout> --spec <mutants.toml>`, documented in `../xbrlstd-rs/tools/README.md`), with both paths absolute: the tool runs from `../xbrlstd-rs/tools`, so a relative path, or the default `--repo`, points into `xbrlstd-rs`. It works in a private copy, never the shared working copy. Documentation and mechanical edits need relevant checks, not artificial tests.
- `check` catches regressions only among tests already known to pass. Every lockfile step, and every change that touches casting, serialization, parsing or lexing, also needs a per-test verdict diff on **both** suites: for a lockfile step the verdicts must be equal; for a behaviour change, every moved verdict must be one the change explains. Take `xee-testrunner -v all` logs before and after, named by SHA, and compare them with `../xee-json-state/verdicts.sh diff <before> <after>` (exit 0 means identical). The diff also shows newly passing tests (then run `update`) and failures that changed mode.
- What reaches `xbrlstd-rs` is proven there, at the `rev` bump: its full gate with the corpus drains, its conformance ratchet and the native/xee differential (its ADR135). Expect that scrutiny for any change to a value, an error code or a cost the consumer can observe.
- Performance claims need measurements: name the bench (divan, `cargo bench -p <crate>`) or the workload, and the configuration. Clippy compiles the benches; nothing runs them in the gate.
- In the report, state what actually ran, with its numbers, and why anything relevant did not run. Never carry "not run" boilerplate over from an earlier report.

## Independent review

- **Every non-trivial commit is reviewed by a reader that is not the session that wrote it, and that is not negotiable.** Spawning that reviewer is standing-authorized: run it without asking, and ask only when the review's scope is unclear. A harness hint that discourages subagents is not a failure: stop only if the spawn errors or the tool is absent, then say so plainly and do not close out. Self-review cannot clear the gate. The umbrella `review-slice` skill is the routine (tier, lenses, prompt, worktrees, taking the findings); `../xbrlstd-rs/docs/rust.md` §10 is the hunt discipline it sequences.
- Give the reviewer the exact revision, the scope, the acceptance criteria and the specification sections. A verdict is against a commit: a fix written in response to a finding needs a fresh pass over the fix.
- The reviewer derives expectations from the specification and the suites before reading the change's own comments and commit message.
- Reproduce every finding before reporting it; for a regression test, confirm it fails with the fix reverted. An unreproduced concern is labelled a hypothesis. Formatting preferences and nonessential prose polish are not findings.
- A review is read-only. Probing (revert-to-confirm, a probe test, a bisect) happens in a detached worktree off the revision under review, which the session creates and passes to the reviewer by path, with a probe build's target dir outside it. Never probe in the shared checkout:

  ```
  git worktree add --detach ../rev-<tag> <revision>
  # ... probe / revert / cargo test in ../rev-<tag> ...
  git worktree remove --force ../rev-<tag>
  ```

  After any worktree step, verify `git branch --show-current` and HEAD before committing.
- Once the internal review has converged, Codex reviews the branch in its own harness: the umbrella `implement-slice` skill §7b (`external-review start <checkout> <branch> --against main`). The local-only rule in `xbrlstd-rs` and in `review-slice` exists because of `xbrlstd-rs`'s confidential corpus, which xee does not hold; no `xbrlstd-rs` corpus content goes into a prompt or a probe.
- Turn reviews into lints: when a finding is something a rustc or clippy lint could have caught, enabling that lint is part of the fix. Prefer `#[expect(lint, reason = "...")]` to `#[allow]`, so a suppression surfaces once its reason is gone.

## Delivery

- `main` is the trunk and the only branch that is pushed. Every change is a short-lived **local** branch off `main`, never pushed; give it a worktree of its own (`git worktree add -b <branch> ../xee-<topic> main`) when the main checkout is in use. Before committing on a detached HEAD, attach the work to a branch without changing its base. Commit the implementation at the end of implementation, without waiting for a signal, so the reviewer has a stable tree; review fixes go in follow-up commits on the same branch. Put the gate evidence in each commit message: there is no PR to carry it.
- Integrate onto `main` only with Wietse's explicit approval, given per branch. Squash is the default close-out: one commit whose message says what the change does, with its gate and review evidence and a correction of anything a branch commit's message got wrong. Fast-forward is the exception, on his explicit call, when each commit is independently gate-green and its message is a record worth keeping. Only `git commit` runs the pre-commit hook: a commit that `git rebase` or `git am` makes, or a `cherry-pick`, `revert` or `merge` that needs no conflict resolution, never meets it, so before a fast-forward close-out of a branch that holds such commits, run the gate (fmt, clippy, `./check`) on each of them; the hook itself skips a checkout with nothing staged. Verify branch and HEAD before committing or integrating.
- Push `main` only on Wietse's word. The push is the step that cannot be taken back (*Consumer* below); it bypasses the `main` ruleset as admin. A change reaches `xbrlstd-rs` only through a `rev` bump there, which is a reviewed change of its own.
- Update the affected records in the same change: *What the fork adds* below when the change adds or alters something the fork offers on top of upstream, the filters with the change that explains them, `vendor/README.md` when a suite moves, and this file when a convention changes. When a change takes a decision or defers a defect, its `xbrlstd-rs` record (an ADR, a technical-debt entry, a security-dashboard row) is committed on an `xbrlstd-rs` branch before the xee branch is integrated onto `main`, and the handoff names that branch. Push xee `main` only once that record has landed on `xbrlstd-rs` `master`.
- A convention change binds at landing time: a branch in flight conforms when it lands. A change whose migration is not obvious states the rule for in-flight branches in its commit message.
- This file describes the present: edit it in place when a rule changes, and leave the history to git.
- In the handoff, state what changed, what was checked and what did not run, material limitations, and what still awaits review or approval. Distinguish measured results from assumptions.

## Fork status

Since 2026-09-24 this fork no longer tracks upstream (`Paligo/xee`): no upstream PRs, no rebasing onto upstream, no upstream-review constraints, and no splitting of commits so they can be upstreamed. It forked from upstream `200b1e33` ("Fix clippy issues. (#152)"). Upstream fixes can still be cherry-picked when they are worth having.

**Trunk.** `main` is the trunk and the only branch that is pushed; there is a single developer, so there are no pushed topic branches or GitHub PRs. How a change gets there is *Delivery* above.

**Consumer.** `xbrlstd-rs` depends on this fork by git SHA (`xee-xpath`, `xee-interpreter`, `xee-xpath-macros`, `xee-xpath-ast` in `../xbrlstd-rs/Cargo.toml`, all pinned to the same `rev`). To ship a change, push `main` and bump that `rev` in `xbrlstd-rs`. **Never rewrite pushed history.** A pinned SHA that becomes unreachable breaks `xbrlstd-rs` builds.

**What the fork adds on top of upstream** (details in `git log 200b1e33..`):

- **`#[xpath_fn]` macro hardening.** Bad signatures and arities are compile errors with spans instead of macro panics. Context/interpreter injection uses explicit `#[xpath_context]` / `#[xpath_interpreter]` attributes, with a name-based fallback. Emitted code uses absolute `::xee_interpreter::*` paths, so the macro works from external crates. trybuild UI tests live in `xee-interpreter/tests/ui/`; regenerate the `.stderr` files after intentional message changes with `TRYBUILD=overwrite cargo test -p xee-interpreter --test macro_ui`.
- **Extension-function registry.** `StaticContextBuilder::add_function` / `add_functions` registers host functions next to the built-ins (built-ins take precedence and cannot be shadowed). Signatures use `Q{uri}local` notation. `build()` is fallible. `DynamicContext::user_data::<T>()` holds host state.
- **Typed node values.** `NodeTypedValueProvider` on `DynamicContext`: atomization asks the host for a node's typed value instead of always returning `xs:untypedAtomic`. `NodeNilledProvider` backs `fn:nilled`. xee itself stays schema-unaware; `instance of` deliberately does not consult the provider. The design record is ADR54 in `xbrlstd-rs` (`../xbrlstd-rs/docs/decisions/adr-54-host-typed-node-values.md`); the original proposal is archived at `../xbrlstd-rs/docs/archive/xee-typed-node-values-proposal.md`.
- **Semantics fixes.** NaN ordering comparisons for `xs:double`/`xs:float`. Left-to-right short-circuit `and`/`or`, lowered to conditionals at AST→IR, where upstream evaluates both operands eagerly.
- **Opt-in dialect.** `XPathDialect::XbrlFormula` (via `parse_with_dialect`) adds `INF`/`NaN` `xs:double` literals.
- **Public constructors.** Validated `xs:date`/`xs:time`/`xs:dateTime` atomics, plus `Name` re-exported so hosts can build `xs:QName`s.
- **Benchmarks** (divan). `xee-xpath/benches/xpath.rs`, including `parse` benches that time document construction. `xee-json/benches/tokenizer.rs` times the tokenizer against serde_json on synthetic inputs of about 1 MiB each (numbers, a large object, strings with and without escapes, deep nesting, string decoding and borrowing): `cargo bench -p xee-json`, or `cargo test -p xee-json --benches` to run each bench once. The inputs and their guards (both parsers accept each input, the decode and borrow pairs agree) are defined once in `xee-json/benches/inputs/mod.rs`; `xee-json/tests/bench_inputs.rs` runs every guard, so `cargo test` covers them without `--benches`. serde_json is a dev-dependency of `xee-json` only.
- **Parse and compile cost.** Upstream re-parsed, cloned the parsed subtree, or copied what it had lowered so far at each nesting level or list item, so parsing and compiling took time or memory exponential or quadratic in an expression's nesting or length. Here the grammar does linear work, and lowering to IR holds memory linear in both, apart from the names the renaming generates. `xee-xpath-ast/tests/parse_allocations.rs` (allocations) and `xee-xpath/tests/compile_allocations.rs` (bytes allocated, or the most held at once) measure the shapes that were not linear, at three sizes. Still super-linear: the variable renaming, in time and memory in the bindings of one name, since the names it generates grow by one `*` each (every placeholder binds the same name), and in time in the number of variables in scope; lowering's time, as it appends each nested expression's bindings to its parent's (quadratic in nesting, and in the length of an arrow chain); the bytecode compiler's scope lookups; and converting very long integer literals. Nesting depth is bounded by nothing but the stack, and the parsed tree nests one level per operator of a chain, so a chain is as deep as it is long: on a 2 MiB thread in release, lowering `1 + 1 + …` overflows from about 550 terms; a union, intersect or except chain is lowered in a loop, and first overflows when the parsed tree is dropped, from about 11,900 operands, and from about 16,300 already in the variable renaming, before lowering. Evaluating a comma-separated sequence, or an XSLT sequence constructor, takes memory linear in its length and nesting, where upstream held and copied every intermediate concatenation (`xee-xpath/tests/evaluation_allocations.rs`, `xee-xslt-compiler/tests/evaluation_allocations.rs`). A flat sequence takes time n log n, as its items are joined the way a binary counter carries; a nested one still copies its inner sequence at each level, time quadratic in the nesting. A union or intersect chain is one node set of its operands, joined two at a time the same way, and an intersect and except chain is the intersection of the operands it keeps, except the union of those it removes (`xee-xpath/tests/evaluation_allocations.rs`, `xee-xpath/tests/node_set_chains.rs`). Upstream held every intermediate result of such a chain, and hashed and sorted everything before it at each union or `except` step: memory and time quadratic in its length. Here about log2(n) intermediate results are held at once, each no larger than the operands it joins, so memory is linear in the chain's length for operands of bounded size, such as single nodes, and m log n for n operands of the same m nodes; a union or except chain of single nodes takes time n log² n. A parenthesized chain is an operand of its own, so a union nested in parentheses still sorts its inner result at each level, time quadratic in the nesting.
- **Checked bytecode fields.** Every index, jump displacement and arity the bytecode builder writes into a fixed-width instruction field is checked, and so is the arity `fn:apply` passes on at run time: a program that does not fit is refused with XPDY0130 (an implementation limit) instead of panicking or being silently miscompiled. The builder checks when the program is compiled, so it also refuses an oversized part that would never run. Static function ids are bounded where functions are registered (`ExtensionFunctionLimitExceeded`, untested), not by the builder.

**Design boundary.** Keep XBRL semantics in `xbrlstd-rs` and generic hooks in xee (registry, providers, dialects). This is a design preference, not an upstream constraint. XBRL-specific code may live here when that is clearly the better home. Items still marked `pub #[doc(hidden)]` only for macro support can become proper public API.

**Legacy branches.** The old upstream-oriented branch stack was merged into `main` and deleted on 2026-09-24. The one exception is the remote `macro-hardening` branch, which stays because it is the source branch of the still-open upstream PR [Paligo/xee#151](https://github.com/Paligo/xee/pull/151); deleting it would close that PR. Its commits are already in `main`.

**License.** MIT. Keep `LICENSE-MIT` and `COPYRIGHT` intact.

## CI and tooling

- **Toolchain:** `rust-toolchain.toml` pins the one Rust version; rustup picks it up locally, and CI installs it with `rustup toolchain install --no-self-update`. It moves together with `xbrlstd-rs`'s pin, never ahead of it, because xee is compiled there with that toolchain.
- **Workspace:** `[workspace.package]` holds the edition and shared metadata, and sets `publish = false`. `[workspace.dependencies]` declares every shared dependency, including the internal crates. Members inherit with `{ workspace = true }`, so an upgrade moves one line.
- **Gate:** `.github/workflows/ci.yml` runs on pushes to `main`, on PRs, and by hand (`workflow_dispatch`): fmt, clippy `-D warnings`, build, test, and the XPath and XSLT conformance `check`s, both in debug mode. `./check` runs the same tests and both suites locally. `.githooks/pre-commit` runs the same gate on each `git commit` that changes code; change the two together. Activate the hook per clone with `git config core.hooksPath .githooks`.
- **Fuzzing:** `fuzz/` is its own cargo workspace (outside the gate, so stable never builds libfuzzer) with `xee-json` targets `robustness` and `differential` (against serde_json). Run from the root: `cargo +nightly fuzz run <target> fuzz/corpus/<target> vendor/xpath-tests/misc/JSONTestSuite/test_parsing -- -max_total_time=300 -max_len=4096` (the writable corpus first, so `vendor/` is never written). Its replay tests, fmt and clippy run on stable with `--manifest-path fuzz/Cargo.toml`.
- **Advisories:** `.github/workflows/audit.yml` runs `cargo audit` daily and on lockfile changes. It sits beside the gate, not in it.
- **Upgrades:** follow the rounds in `xbrlstd-rs` (the umbrella `upgrade-tooling` skill). A lockfile step also needs the per-test verdict diff on both suites (*Verification*).
- **Releases:** none. There is no crates.io publishing; the fork is consumed by git SHA.
