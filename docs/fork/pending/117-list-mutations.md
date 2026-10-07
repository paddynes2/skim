# List helper semantic mutation evidence

Command: `node --experimental-strip-types --test src/fork/tests/list117-layout.test.mjs`.

Baseline: 2 passed. Restored source: 2 passed.

## virtual_offset

Changed `offset += 28; previous = bucket;` to `offset += 0; previous = bucket;`. Result: KILLED by an assertion failure; the other test remained green.

```text
TAP version 13
# Subtest: date sections keep exact virtual offsets without dropping boundary rows
not ok 1 - date sections keep exact virtual offsets without dropping boundary rows
  ---
  duration_ms: 4.4903
  type: 'test'
  location: 'C:\\Users\\Patrick\\Projects\\skim-header-polish-20261007\\src\\fork\\tests\\list117-layout.test.mjs:5:1'
  failureType: 'testCodeFailure'
  error: |-
    Expected values to be strictly deep-equal:
    + actual - expected
    
      [
        0,
    +   0,
    +   80,
    +   80,
    +   160,
    +   160,
    +   240
    -   28,
    -   108,
    -   136,
    -   216,
    -   244,
    -   324
      ]
    
  code: 'ERR_ASSERTION'
  name: 'AssertionError'
  expected:
    0: 0
    1: 28
    2: 108
    3: 136
    4: 216
    5: 244
    6: 324
  actual:
    0: 0
    1: 0
    2: 80
    3: 80
    4: 160
    5: 160
    6: 240
  operator: 'deepStrictEqual'
  stack: |-
    TestContext.<anonymous> (file:///C:/Users/Patrick/Projects/skim-header-polish-20261007/src/fork/tests/list117-layout.test.mjs:10:10)
    Test.runInAsyncScope (node:async_hooks:214:14)
    Test.run (node:internal/test_runner/test:1047:25)
    Test.start (node:internal/test_runner/test:944:17)
    startSubtestAfterBootstrap (node:internal/test_runner/harness:296:17)
  ...
# Subtest: previews trim quoted tails but preserve normal sentences
ok 2 - previews trim quoted tails but preserve normal sentences
  ---
  duration_ms: 0.6919
  type: 'test'
  ...
1..2
# tests 2
# suites 0
# pass 1
# fail 1
# cancelled 0
# skipped 0
# todo 0
# duration_ms 135.7834

```

## quoted_tail

Changed `wrote:|` to `said:|`. Result: KILLED by an assertion failure; the other test remained green.

```text
TAP version 13
# Subtest: date sections keep exact virtual offsets without dropping boundary rows
ok 1 - date sections keep exact virtual offsets without dropping boundary rows
  ---
  duration_ms: 2.0578
  type: 'test'
  ...
# Subtest: previews trim quoted tails but preserve normal sentences
not ok 2 - previews trim quoted tails but preserve normal sentences
  ---
  duration_ms: 1.0528
  type: 'test'
  location: 'C:\\Users\\Patrick\\Projects\\skim-header-polish-20261007\\src\\fork\\tests\\list117-layout.test.mjs:18:1'
  failureType: 'testCodeFailure'
  error: |-
    Expected values to be strictly equal:
    + actual - expected
    
    + 'Please review. On Tuesday, Anna wrote: old reply'
    - 'Please review.'
                     ^
    
  code: 'ERR_ASSERTION'
  name: 'AssertionError'
  expected: 'Please review.'
  actual: 'Please review. On Tuesday, Anna wrote: old reply'
  operator: 'strictEqual'
  stack: |-
    TestContext.<anonymous> (file:///C:/Users/Patrick/Projects/skim-header-polish-20261007/src/fork/tests/list117-layout.test.mjs:19:10)
    Test.runInAsyncScope (node:async_hooks:214:14)
    Test.run (node:internal/test_runner/test:1047:25)
    Test.processPendingSubtests (node:internal/test_runner/test:744:18)
    Test.postRun (node:internal/test_runner/test:1173:19)
    Test.run (node:internal/test_runner/test:1101:12)
    async startSubtestAfterBootstrap (node:internal/test_runner/harness:296:3)
  ...
1..2
# tests 2
# suites 0
# pass 1
# fail 1
# cancelled 0
# skipped 0
# todo 0
# duration_ms 126.891

```

Both trials restored src/fork/layout/mail-list.ts with the original bytes in a finally block. Original and restored SHA256: `e28af36358a995114cd6f957bf88f0d370eadc2d0fed5254fec1ae4ecc808b3c`. No other source file was edited.
