-- v1.1.17 (D68): Jev now reads the start of each body, not the 140-character
-- snippet. Every stored score is cleared so the 90-day window is scored again
-- with the better preview, and today's counter restarts so the re-score is
-- not stopped by the daily cap.
DELETE FROM fork_priority_score;
DELETE FROM settings WHERE key = 'fork_priority_used';
