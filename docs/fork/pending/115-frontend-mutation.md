# Frontend semantic mutation checks, 1.1.5

Each change is restored byte-for-byte in a finally block. These exercise behavior in new modules beyond the aggregate revert proof.

- unknown availability incorrectly treated as free: killed by assertion; src/fork/tests/cal115-planning.test.mjs
- untouched company bytes stripped: killed by assertion; src/fork/tests/deals115.test.mjs
- duplicate saved-search names accepted: killed by assertion; src/fork/tests/search115.test.mjs
