# Semantic mutation proof

Each mutation was applied alone. Exact source bytes were restored in finally after each test. An initial Rust mutation to None caused a database lookup error rather than an assertion and was rejected as proof; the revised mutation below targets an existing wrong message.

### hash grouping guard
Command: `node --test --test-name-pattern=attachment grouping src/fork/tests/search-compose116.test.mjs`
Mutation: `const key=hash?`${hash}:${file.size}`:`id:${file.id}`;` becomes `const key=`${file.filename}:${file.size}`;`.
Exit: 1. Assertion failed as required.

```text
TAP version 13
# Subtest: attachment grouping never merges same-name versions without equal content hashes
not ok 1 - attachment grouping never merges same-name versions without equal content hashes
  ---
  duration_ms: 2.6224
  type: 'test'
  location: 'C:\\Users\\Patrick\\Projects\\skim-header-polish-20261007\\src\\fork\\tests\\search-compose116.test.mjs:14:1'
  failureType: 'testCodeFailure'
  error: |-
    Expected values to be strictly deep-equal:
    + actual - expected
    
      [
        [
          1,
          2,
    -   ],
    -   [
          3,
    -   ],
    -   [
          4
        ]
      ]
    
  code: 'ERR_ASSERTION'
  name: 'AssertionError'
  expected:
    0:
      0: 1
      1: 2
    1:
      0: 3
    2:
      0: 4
  actual:
    0:
      0: 1
      1: 2
      2: 3
      3: 4
  operator: 'deepStrictEqual'
  stack: |-
    TestContext.<anonymous> (file:///C:/Users/Patrick/Projects/skim-header-polish-20261007/src/fork/tests/search-compose116.test.mjs:16:10)
    Test.runInAsyncScope (node:async_hooks:214:14)
    Test.run (node:internal/test_runner/test:1047:25)
    Test.start (node:internal/test_runner/test:944:17)
    startSubtestAfterBootstrap (node:internal/test_runner/harness:296:17)
  ...
1..1
# tests 1
# suites 0
# pass 0
# fail 1
# cancelled 0
# skipped 0
# todo 0
# duration_ms 175.7588

```

### transition ordering
Command: `node --test --test-name-pattern=serialized autosaves src/fork/tests/search-compose116.test.mjs`
Mutation: `await save();await open();finish();` becomes `finish();await save();await open();`.
Exit: 1. Assertion failed as required.

```text
TAP version 13
# Subtest: serialized autosaves finish before popout and failed transitions keep the editor
not ok 1 - serialized autosaves finish before popout and failed transitions keep the editor
  ---
  duration_ms: 1.6502
  type: 'test'
  location: 'C:\\Users\\Patrick\\Projects\\skim-header-polish-20261007\\src\\fork\\tests\\search-compose116.test.mjs:25:1'
  failureType: 'testCodeFailure'
  error: |-
    Expected values to be strictly equal:
    
    true !== false
    
  code: 'ERR_ASSERTION'
  name: 'AssertionError'
  expected: false
  actual: true
  operator: 'strictEqual'
  stack: |-
    TestContext.<anonymous> (file:///C:/Users/Patrick/Projects/skim-header-polish-20261007/src/fork/tests/search-compose116.test.mjs:30:34)
    async Test.run (node:internal/test_runner/test:1054:7)
    async startSubtestAfterBootstrap (node:internal/test_runner/harness:296:3)
  ...
1..1
# tests 1
# suites 0
# pass 0
# fail 1
# cancelled 0
# skipped 0
# todo 0
# duration_ms 148.2825

```

### exact matching message
Command: `cargo test --manifest-path src-tauri/Cargo.toml cached_search_returns_the_matching_passage_and_exact_older_message`
Mutation: `message_id: Some(r.get(11)?),` becomes `message_id: Some(r.get::<_, i64>(11)? + 1),`.
Exit: 101. Assertion failed as required.

```text

running 1 test
test fork::search_query::tests::cached_search_returns_the_matching_passage_and_exact_older_message ... FAILED

failures:

---- fork::search_query::tests::cached_search_returns_the_matching_passage_and_exact_older_message stdout ----

thread 'fork::search_query::tests::cached_search_returns_the_matching_passage_and_exact_older_message' (68784) panicked at src\fork\search_query.rs:1327:13:
assertion `left == right` failed
  left: Some(2)
 right: Some(1)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    fork::search_query::tests::cached_search_returns_the_matching_passage_and_exact_older_message

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 544 filtered out; finished in 0.02s

   Compiling skim v1.1.6 (C:\Users\Patrick\Projects\skim-header-polish-20261007\src-tauri)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 25.66s
     Running unittests src\lib.rs (C:/Users/Patrick/Projects/skim/src-tauri/target\debug\deps\skim_lib-6778e5284d9d69fa.exe)
error: test failed, to rerun pass `--lib`

```
