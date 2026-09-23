//! Fork settings keys the frontend may read and write through the upstream
//! `get_settings` / `set_setting` commands (`commands/settings.rs` chains this
//! list onto its own `ALLOWED`). Values are plain strings, like upstream's.

pub const ALLOWED: &[&str] = &[
    // Phase 2
    "fork_density",       // comfortable | compact
    "fork_avatars",       // on | off
    "fork_zoom",          // 0.8 .. 1.5
    "fork_after_archive", // next | previous | list
    // Phase 3
    "fork_list_order", // date | unread_first
    // Phase 6
    "fork_reply_inline",     // on | off
    "fork_rich_text",        // on | off
    "fork_undo_send_secs",   // 0 | 5 | 10 | 20 | 30
    "fork_smell",            // on | off
    "fork_smell_block_hard", // on | off
    "fork_smell_ignored",    // JSON list of rule ids
    // Phase 7
    "fork_cal_default_len", // minutes
    "fork_cal_work_start",  // HH:MM
    "fork_cal_work_end",    // HH:MM
    "fork_cal_work_days",   // e.g. "1,2,3,4,5" (Mon=1)
    "fork_cal_second_tz",   // IANA zone
    "fork_booking_link",
    // Phase 9
    "fork_crm_drawer", // open | closed
    // Phase 10
    "fork_court_ai",         // on | off
    "fork_court_ai_cap",     // threads/day
    "fork_court_amber_days", // default 2
    "fork_court_red_days",   // default 5
    "fork_court_nudge",      // HH:MM or "off"
    // Phase 12
    "fork_mcp", // on | off
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_prefixed_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for key in ALLOWED {
            assert!(key.starts_with("fork_"), "{key} must be fork_-prefixed");
            assert!(seen.insert(*key), "{key} listed twice");
        }
    }
}
