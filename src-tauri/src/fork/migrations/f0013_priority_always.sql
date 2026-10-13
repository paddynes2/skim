-- v1.1.12 (D65): `fork_priority.reason` gains 'always' (a sender on the
-- always list). The table is derived data, rebuilt by the next pass, so it is
-- recreated rather than migrated row by row.
DROP TABLE IF EXISTS fork_priority;
CREATE TABLE fork_priority (
  thread_id INTEGER PRIMARY KEY,
  reason    TEXT NOT NULL CHECK(reason IN ('deal','starred','always','jev'))
);
