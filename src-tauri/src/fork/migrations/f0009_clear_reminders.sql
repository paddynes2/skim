-- v1.1.3: Snooze and Follow-ups are gone from the UI (D56) and their sweep no
-- longer runs, but fork::list still pins a due reminder until it is opened.
-- Clear the table once so no leftover row can pin a thread for good. There
-- were no rows on the live database when this shipped.
DELETE FROM fork_reminders;
