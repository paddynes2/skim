-- [Gmail]/Important was mapped to role 'starred', so "Starred" mixed 7,881
-- Important messages with the 1,141 real stars. detect_role now maps it to
-- its own role; repair the rows already stored.
UPDATE folders SET role = 'important'
 WHERE role = 'starred'
   AND (lower(imap_name) LIKE '%/important' OR lower(imap_name) = 'important');
