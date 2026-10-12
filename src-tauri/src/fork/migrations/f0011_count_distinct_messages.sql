-- v1.1.10 (D63): a thread's message_count counts distinct Message-IDs, as the
-- conversation view shows it (threading::recompute_thread). Recount every
-- thread once, so a thread nothing touches after the update does not keep
-- counting the Inbox and Important copies of one message twice.
UPDATE threads SET message_count =
  (SELECT count(DISTINCT COALESCE(m.message_id, 'pk:' || m.id)) FROM messages m
    WHERE m.thread_id = threads.id)
WHERE EXISTS (SELECT 1 FROM messages m WHERE m.thread_id = threads.id);
