-- A Gmail mailbox added by hand (app password) carries provider 'custom'.
-- Every Gmail-specific path keys on the host from now on (fork::gmail), and
-- the stored label is repaired once so the UI (labels heading, web calendar)
-- agrees.
UPDATE accounts SET provider = 'gmail'
 WHERE lower(imap_host) = 'imap.gmail.com' AND provider = 'custom';
