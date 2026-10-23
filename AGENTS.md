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
- `xee-regex` — XPath 3.1 regular expressions (F&O 3.1 §5.6 over the XSD 1.1 grammar, `Regex::new`), the engine under `xee-interpreter`'s regex functions and fixed cast patterns (*What the fork adds*), and XSD 1.1's own dialect, the one of an `xs:pattern` facet (`Regex::xsd`, no flags, anchored at both ends, its unknown block names listed by `Regex::unknown_blocks`), which nothing in xee calls: the front end (flags, parser, Unicode tables, compiler), two matchers that give the same answers (a PikeVM, each search linear in the input, and a backtracker for back-references) behind a prefilter, and F&O's four operations, under a step budget and a memory ceiling per call (`Budget::over_budget`, on a budget made for one call, tells a refusal at the budget from one at the ceiling); `Regex::heap_bytes` reports what a compiled pattern keeps, each allocation at `chunk`'s size. Its `src/tables.rs` is generated from the Unicode Character Database at the toolchain's Unicode version by `tests/generate_tables.rs`; a toolchain bump that moves that version fails its tests until the tables are regenerated.
- `xot` — the XML tree xee evaluates over: parsing, traversal, manipulation and serialization. Imported from a fork of upstream `faassen/xot` (*Fork status*).

External companion projects (not in this repo): `xee-php` (PHP bindings). `regexml`, upstream's regex engine, is no longer used.

## Orient to the task

The workflow in this file follows `xbrlstd-rs`'s (`../xbrlstd-rs/AGENTS.md`), adapted to a fork that `xbrlstd-rs` pins by SHA and that has no PRs.

- Confirm the working directory, `git status`, branch and HEAD before editing. Other sessions work in this repository through their own worktrees and local branches (`git worktree list`): leave their branches and changes alone.
- xee keeps no plans index, ADR set or debt register of its own; `TODO.md` lists known xee work that is not scheduled, a stopgap until it is decided whether xee needs a register. Its records live in `xbrlstd-rs`, the consumer: a decision that binds the fork or the API `xbrlstd-rs` uses is an `xbrlstd-rs` ADR (`../xbrlstd-rs/docs/decisions.md`; ADR24 is the fork itself, ADR54 the typed node values); deferred work and known divergences go in `../xbrlstd-rs/docs/technical-debt.md`, security findings on `../xbrlstd-rs/docs/security-dashboard.md`, and upgrade work into the open `xbrlstd-rs` upgrade round as `xee: …` rows (the umbrella `upgrade-tooling` skill). A commit message is never the only record of a decision or a deferred defect.
- xee is public; `xbrlstd-rs` is not. Commit messages, code comments, tests and test data in xee carry no `xbrlstd-rs` internals or security exposure, and no `xbrlstd-rs` data, such as filings or documents produced from them. A claim about XPath or XSLT semantics rests on the specifications and the suites.
- Carry authorized work through implementation, verification and review, and make routine internal choices yourself. Surface before implementing: a change to the public API `xbrlstd-rs` uses (the crates under *Consumer* below, `xot` among them), and a deliberate departure from a specification. When an `xbrlstd-rs` task needs an xee change, surface it and agree on it first: the fork change is a change of its own.
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

- The gate is CI's (`.github/workflows/ci.yml`): `cargo fmt --check --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `./check` (`cargo test` plus both conformance `check`s, in debug mode). `.githooks/pre-commit` runs all of it on each `git commit` that changes code, and skips a commit whose staged paths are all Markdown; most of its time is the two conformance checks. Its first step, *action pins*, refuses a workflow `uses` key that is not pinned to a commit with its version tag in a trailing comment, and a key spelled in a way its grep does not read (xbrlstd-rs ADR220, "The workflows' own actions"); the weekly Dependabot pull request (`.github/dependabot.yml`) is the notice that a pin can move, and the move lands as any change does. Activate it per clone (*CI and tooling*). Never bypass a failing gate (`--no-verify`).
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

**Consumer.** `xbrlstd-rs` depends on this fork by git SHA (`xee-xpath`, `xee-interpreter`, `xee-xpath-macros`, `xee-xpath-ast` and, from the `rev` bump that takes `xot/`, `xot`, in `../xbrlstd-rs/Cargo.toml`, all pinned to the same `rev`). To ship a change, push `main` and bump that `rev` in `xbrlstd-rs`. **Never rewrite pushed history.** A pinned SHA that becomes unreachable breaks `xbrlstd-rs` builds.

**xot.** `xot/`, the XML tree library, is a workspace member like the others, under this repository's toolchain and *Delivery*; the advisory audit covers its dependencies, not `xot` itself (*CI and tooling*). The gate's test step builds it with the features the other members ask for (`proptest`, `serde`), and its clippy step with all of them, so its `icu` tests (three, and a doctest) run only under `cargo test -p xot --all-features`: run that too for a change in `xot/`. The commit "Import xot into the repository, without its history" added it as `Wietse/xot`'s `3cab3560` (upstream `faassen/xot`'s 0.31.2 release plus a lint-fix commit and a rustfmt pass on two test files), tree for tree; the history before it is in `Wietse/xot`, archived read-only. As with xee, upstream is not tracked. A commit from upstream or from `Wietse/xot`'s branches is ported with `git format-patch` there and `git am -3 --directory=xot/ --exclude=xot/Cargo.lock --exclude=xot/CHANGES.md` here. A port that changes xot's dependencies inherits the shared ones from the workspace and updates the root `Cargo.lock`, the only lockfile, which makes it a lockfile step (*Verification*). `xot/CHANGES.md` (upstream's through 0.31.2, plus `Wietse/xot`'s entry for its lint fixes) and `xot/release.toml` are not maintained here; changes made here are recorded in git, as xee's are.

**What the fork adds on top of upstream** (details in `git log 200b1e33..`):

- **`#[xpath_fn]` macro hardening.** Bad signatures and arities are compile errors with spans instead of macro panics. Context/interpreter injection uses explicit `#[xpath_context]` / `#[xpath_interpreter]` attributes, with a name-based fallback. Emitted code uses absolute `::xee_interpreter::*` paths, so the macro works from external crates. trybuild UI tests live in `xee-interpreter/tests/ui/`; regenerate the `.stderr` files after intentional message changes with `TRYBUILD=overwrite cargo test -p xee-interpreter --test macro_ui`.
- **Extension-function registry.** `StaticContextBuilder::add_function` / `add_functions` registers host functions next to the built-ins (built-ins take precedence and cannot be shadowed). Signatures use `Q{uri}local` notation. `build()` is fallible. `DynamicContext::user_data::<T>()` holds host state.
- **Typed node values.** `NodeTypedValueProvider` on `DynamicContext`: atomization asks the host for a node's typed value instead of always returning `xs:untypedAtomic`. `NodeNilledProvider` backs `fn:nilled`. xee itself stays schema-unaware; `instance of` deliberately does not consult the provider. The design record is ADR54 in `xbrlstd-rs` (`../xbrlstd-rs/docs/decisions/adr-54-host-typed-node-values.md`); the original proposal is archived at `../xbrlstd-rs/docs/archive/xee-typed-node-values-proposal.md`.
- **Semantics fixes.** NaN ordering comparisons for `xs:double`/`xs:float`. Left-to-right short-circuit `and`/`or`, lowered to conditionals at AST→IR, where upstream evaluates both operands eagerly.
- **Opt-in dialect.** `XPathDialect::XbrlFormula` (via `parse_with_dialect`) adds `INF`/`NaN` `xs:double` literals.
- **Public constructors.** Validated `xs:date`/`xs:time`/`xs:dateTime` atomics, plus `Name` re-exported so hosts can build `xs:QName`s.
- **Benchmarks** (divan). `xee-xpath/benches/xpath.rs`, including `parse` benches that time document construction. `xee-json/benches/tokenizer.rs` times the tokenizer against serde_json on synthetic inputs of about 1 MiB each (numbers, a large object, strings with and without escapes, deep nesting, string decoding and borrowing): `cargo bench -p xee-json`, or `cargo test -p xee-json --benches` to run each bench once. The inputs and their guards (both parsers accept each input, the decode and borrow pairs agree) are defined once in `xee-json/benches/inputs/mod.rs`; `xee-json/tests/bench_inputs.rs` runs every guard, so `cargo test` covers them without `--benches`. serde_json is a dev-dependency of `xee-json` only. `xee-xpath/benches/regex.rs` times the regular-expression functions through the XPath API on synthetic inputs: short value checks, each evaluation compiling its pattern, the same checks 1,000 times in one evaluation, text blocks of about 1 KB to 1 MB through all four functions, and shapes whose cost grows faster than the input on a backtracking matcher: `cargo bench -p xee-xpath --bench regex`, or `cargo test -p xee-xpath --bench regex` to run each once. Every bench stops after 20 samples or 2 seconds. The workloads and their answers, computed from the text by scans of their own rather than by a regular expression, are defined once in `xee-xpath/benches/regex_inputs/mod.rs`; each bench checks its answer before timing it, and `xee-xpath/tests/regex_bench_inputs.rs` checks every workload under `cargo test`, the text workloads on the blocks up to 100 KB, and each block's size. The text sizes stop within a call's budget of steps: the costliest text workload a byte, `fn:analyze-string` of word pairs, is refused with XPDY0130 from about 1.5 MB of the bench's text, which that test pins.
- **Parse and compile cost.** Upstream re-parsed, cloned the parsed subtree, or copied what it had lowered so far at each nesting level or list item, so parsing and compiling took time or memory exponential or quadratic in an expression's nesting or length. Here the grammar does linear work, and lowering to IR holds memory linear in both, apart from the names the renaming generates. `xee-xpath-ast/tests/parse_allocations.rs` (allocations) and `xee-xpath/tests/compile_allocations.rs` (bytes allocated, or the most held at once) measure the shapes that were not linear, at three sizes. Still super-linear: the variable renaming, in time and memory in the bindings of one name, since the names it generates grow by one `*` each (every placeholder binds the same name), and in time in the number of variables in scope; lowering's time, as it appends each nested expression's bindings to its parent's (quadratic in nesting, and in the length of an arrow chain); the bytecode compiler's scope lookups; and converting very long integer literals. Nesting depth is bounded by nothing but the stack, and the parsed tree nests one level per operator of a chain, so a chain is as deep as it is long: on a 2 MiB thread in release, lowering `1 + 1 + …` overflows from about 550 terms; a union, intersect or except chain is lowered in a loop, and first overflows when the parsed tree is dropped, from about 11,900 operands, and from about 16,300 already in the variable renaming, before lowering. Evaluating a comma-separated sequence, or an XSLT sequence constructor, takes memory linear in its length and nesting, where upstream held and copied every intermediate concatenation (`xee-xpath/tests/evaluation_allocations.rs`, `xee-xslt-compiler/tests/evaluation_allocations.rs`). A flat sequence takes time n log n, as its items are joined the way a binary counter carries; a nested one still copies its inner sequence at each level, time quadratic in the nesting. A union or intersect chain is one node set of its operands, joined two at a time the same way, and an intersect and except chain is the intersection of the operands it keeps, except the union of those it removes (`xee-xpath/tests/evaluation_allocations.rs`, `xee-xpath/tests/node_set_chains.rs`). Upstream held every intermediate result of such a chain, and hashed and sorted everything before it at each union or `except` step: memory and time quadratic in its length. Here about log2(n) intermediate results are held at once, each no larger than the operands it joins, so memory is linear in the chain's length for operands of bounded size, such as single nodes, and m log n for n operands of the same m nodes; a union or except chain of single nodes takes time n log² n. A parenthesized chain is an operand of its own, so a union nested in parentheses still sorts its inner result at each level, time quadratic in the nesting.
- **32-bit name, prefix and namespace ids in `xot`.** `NameId`, `PrefixId` and `NamespaceId` hold a `u32`, built with a checked conversion; upstream's were `u16`, built with `as`, so the 65,537th distinct name, prefix or namespace in one `Xot` wrapped and read back as another. `Value` stays 40 bytes.
- **`xot` reads `a:xmlns` as an attribute.** Only `xmlns` and `xmlns:p` declare a namespace (Namespaces in XML 1.0, section 3); upstream read any attribute whose local name is `xmlns`, prefixed or not, as a default namespace declaration, which moved the element and its descendants into another namespace and dropped the attribute.
- **Linear duplicate-attribute check in `xot`.** A start tag of 32 attributes or more is checked for a repeated attribute against a set of the qualified names seen, a shorter one by a scan as before; upstream scanned every attribute before at each one, time quadratic in a start tag's attributes.
- **A clone's inherited declarations in a fixed order.** `Xot::clone_with_prefixes`, which `fn:serialize` uses for a node below the root, adds the namespace declarations the clone inherits in a fixed order, farthest ancestor first and each ancestor's in its own order, a prefix that a nearer ancestor declares again moving to that place, so that of the prefixes it inherits a name takes the nearest; upstream added them in a hash map's order, which changes from one run to the next, so the same expression serialized differently. A name can still take another prefix of its namespace than a serialization of the whole document gives it: the clone's own declarations come first, and an ancestor's default namespace the clone does not need is dropped.
- **Regular expressions of our own.** `fn:matches`, `fn:replace`, `fn:tokenize`, `fn:analyze-string` and the casts to `xs:language`, `xs:NMTOKEN`, `xs:Name` and `xs:NCName` (with `xs:ID`, `xs:IDREF` and `xs:ENTITY`) run on `xee-regex`, where upstream used `regexml` (MPL-2.0), a backtracking engine with no bound on its work. Each call runs under a budget of its own of 50,000,000 steps (`STEPS_PER_CALL` in `xee-interpreter/src/regex_limits.rs`, no setting) and the crate's ceiling of 2^27 bytes on the memory a call holds; a pattern over one of the crate's parse ceilings, a call that needs the program of a pattern over one of its compile ceilings (one on an input no shorter than the pattern's minimum length that FORX0003 or FORX0004 does not end first), and a call over its budget or its memory, are refused with XPDY0130, and FORX0001 to FORX0004 are raised as before (`xee-xpath/tests/regex_limits.rs`). The casts' patterns and `fn:serialize`'s normalization-form test are the fork's own, anchored and linear (at most 13 steps a byte for the name types and 16.25 for `xs:language`, measured), and run under a budget that grows with the value, 40 steps a byte and 4,096 more (`fixed_pattern_steps`), so that a cast and `castable as` answer whatever the value's length, as `fn:string-length` does; `castable as` raises XPDY0130 rather than answer false only if that budget is ever reached, which `xee-interpreter`'s `the_fixed_patterns_stay_within_half_their_budget` guards against. QT3's 1,641 Perl-derived cases, which the runner skips (`fn-matches-50` depends on XSD 1.0), run through `fn:matches` in `xee-xpath/tests/perl_cases.rs`; the four that assume XSD 1.0's hyphen rules, p888 to p891, are asserted to be the only ones that disagree with the suite. The per-evaluation cache of compiled patterns (`xee-interpreter/src/interpreter/state.rs`) holds at most 2^26 bytes, each allocation counted at what glibc's allocator takes for it (`xee_regex::chunk`): each entry's `heap_bytes`, its key's text and its `Rc`, and a share of the map's table; a counting allocator checks that the count covers what the entries hold. Past that a pattern is compiled for its call and not kept, and nothing is evicted. The crate also has XSD 1.1 Part 2 Appendix G's own dialect, `Regex::xsd(pattern)`, which nothing in xee calls (a host checks `xs:pattern` facets with it): no flags; the pattern matched whole, every branch anchored; `^` and `$` normal characters; F&O 3.1 §5.6.1's additions (reluctant quantifiers, `(?:`, back-references, `\$`) syntax errors; `\p{IsX}` and `\P{IsX}` with a well-formed, unknown block name X each every character (XSD 1.1 §G.4.2.4), where XPath raises FORX0002, the names listed by `Regex::unknown_blocks` for the warning §G.4.2.4 asks for; the same ceilings, and F&O's four operations under the caller's budget, on the PikeVM. Its tests run QT3's `fn/matches.re.xml` tests in their wrapped form unwrapped through it (`xee-regex/tests/xsd_cases.rs`), and the `regex_parse` and `regex_match` fuzz targets select it with bit 5 of their first byte. Not bounded here: the results an evaluation holds across calls, each as large as its call's budget allows, held like any other function's; the `fn:analyze-string` tree xee builds from the crate's result, which is outside the call's 2^27 bytes and holds more than the result does; and compiling a pattern, which the crate's compile ceilings bound and the budget does not charge, done again at each call for a pattern the cache does not keep.
- **Checked bytecode fields.** Every index, jump displacement and arity the bytecode builder writes into a fixed-width instruction field is checked, and so is the arity `fn:apply` passes on at run time: a program that does not fit is refused with XPDY0130 (an implementation limit) instead of panicking or being silently miscompiled. The builder checks when the program is compiled, so it also refuses an oversized part that would never run. Static function ids are bounded where functions are registered (`ExtensionFunctionLimitExceeded`, untested), not by the builder.

**Design boundary.** Keep XBRL semantics in `xbrlstd-rs` and generic hooks in xee (registry, providers, dialects). This is a design preference, not an upstream constraint. XBRL-specific code may live here when that is clearly the better home. Items still marked `pub #[doc(hidden)]` only for macro support can become proper public API.

**Legacy branches.** The old upstream-oriented branch stack was merged into `main` and deleted on 2026-09-24. The one exception is the remote `macro-hardening` branch, which stays because it is the source branch of the still-open upstream PR [Paligo/xee#151](https://github.com/Paligo/xee/pull/151); deleting it would close that PR. Its commits are already in `main`.

**License.** MIT. Keep `LICENSE-MIT` and `COPYRIGHT` intact, and `xot/LICENSE.txt`, xot's own MIT notice, and `xee-regex/LICENSE-UNICODE`, the Unicode License v3 that `xee-regex`'s generated tables are under (its `license` is `MIT AND Unicode-3.0`).

## CI and tooling

- **Toolchain:** `rust-toolchain.toml` pins the one Rust version; rustup picks it up locally, and CI installs it with `rustup toolchain install --no-self-update`. It moves together with `xbrlstd-rs`'s pin, never ahead of it, because xee is compiled there with that toolchain.
- **Workspace:** `[workspace.package]` holds the edition and shared metadata, and sets `publish = false`. `[workspace.dependencies]` declares every shared dependency, including the internal crates. Members inherit with `{ workspace = true }`, so an upgrade moves one line. `xot` inherits the edition, `publish` and its shared dependencies, and keeps upstream's `authors`, `license` and links.
- **Gate:** `.github/workflows/ci.yml` runs on pushes to `main`, on PRs, and by hand (`workflow_dispatch`): fmt, clippy `-D warnings`, build, test, and the XPath and XSLT conformance `check`s, both in debug mode. `./check` runs the same tests and both suites locally. `.githooks/pre-commit` runs the same gate on each `git commit` that changes code; change the two together. Activate the hook per clone with `git config core.hooksPath .githooks`.
- **Fuzzing:** `fuzz/` is its own cargo workspace (outside the gate, so stable never builds libfuzzer) with `xee-json` targets `robustness` and `differential` (against serde_json), and the `xee-regex` targets `regex_parse` (compiling any pattern under any flags, or in the XSD dialect, returns without a panic) and `regex_match` (the PikeVM and the backtracker give the same slots for every match); their first byte selects the flags (bits 0 to 4) or the XSD dialect (bit 5). Run from the root: `cargo +nightly fuzz run <target> fuzz/corpus/<target> vendor/xpath-tests/misc/JSONTestSuite/test_parsing -- -max_total_time=300 -max_len=4096` (the writable corpus first, so `vendor/` is never written; for the `regex_` targets, seed it with patterns rather than JSON, and for `regex_match` a pattern, a zero byte and an input). Its replay tests, fmt and clippy run on stable with `--manifest-path fuzz/Cargo.toml`.
- **Advisories:** `.github/workflows/audit.yml` runs `cargo audit` daily and on lockfile changes. It sits beside the gate, not in it. It matches an advisory only to a package locked from crates.io's registry: cargo-audit 0.22.2, the version CI installed on 2026-10-13, matches none to a path package, so this workspace's own crates, `xot` among them, are outside it, and their dependencies are not.
- **Upgrades:** follow the rounds in `xbrlstd-rs` (the umbrella `upgrade-tooling` skill). A lockfile step also needs the per-test verdict diff on both suites (*Verification*).
- **Releases:** none. There is no crates.io publishing; the fork is consumed by git SHA.
