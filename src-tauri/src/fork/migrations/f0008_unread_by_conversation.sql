-- v1.1.2: folder unread counts are unread conversations, as Gmail counts them
-- (db::queries::recompute_folder_unread). Recount every folder once, so a
-- folder nothing touches after the update does not keep its message count.
UPDATE folders SET unread_count =
  (SELECT count(DISTINCT COALESCE(m.thread_id, -m.id)) FROM messages m
    WHERE m.folder_id = folders.id AND m.is_read = 0);
