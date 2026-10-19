-- v1.1.26 (D73): which build stored a draft's editor words. 0 = an older
-- build, which stored the plain rendering of a draft written in Gmail or by
-- an agent merely because it was opened, so the server copy's links may
-- replace it. 1 = a build that opens such a draft from its server copy: the
-- row holds the user's own edits and the server copy no longer applies.
ALTER TABLE fork_draft_html ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
