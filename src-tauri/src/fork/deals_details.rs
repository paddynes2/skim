//! Local manual company membership, notes and current document.
use super::*;
use rusqlite::OptionalExtension;
use serde::Deserialize;

const DETAILS_SETTING: &str = "fork_deal_details";
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Details {
    pub assignments: HashMap<i64, String>,
    pub excluded: HashSet<i64>,
    pub notes: HashMap<String, String>,
    pub pins: HashMap<String, i64>,
}
fn invalid(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}
pub(super) fn load(conn: &Connection) -> rusqlite::Result<Details> {
    let raw = queries::get_setting(conn, DETAILS_SETTING)?.unwrap_or_else(|| "{}".into());
    serde_json::from_str(&raw).map_err(|_| {
        invalid("Company details could not be read. The stored value was not changed.")
    })
}
pub(super) fn cache_key(conn: &Connection) -> rusqlite::Result<String> {
    Ok(queries::get_setting(conn, DETAILS_SETTING)?.unwrap_or_default())
}
fn save(conn: &Connection, details: &Details) -> rusqlite::Result<()> {
    if details.assignments.len() > 5000
        || details.excluded.len() > 5000
        || details.notes.len() > 500
        || details.pins.len() > 500
    {
        return Err(invalid("The local company detail limit has been reached."));
    }
    queries::set_setting(
        conn,
        DETAILS_SETTING,
        &serde_json::to_string(details)
            .map_err(|_| invalid("Company details could not be saved."))?,
    )
}
fn checked_name(name: &str) -> rusqlite::Result<String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 200
        || name.contains(['\n', '\r', ':'])
        || name.starts_with('#')
    {
        return Err(invalid(
            "Enter a company name without a colon or line break.",
        ));
    }
    Ok(name.to_string())
}
pub(super) fn extend_catalog(deals: &mut Vec<Deal>, details: &Details) {
    let mut names: Vec<_> = details.assignments.values().cloned().collect();
    names.sort();
    names.dedup();
    for name in names {
        if !deals.iter().any(|d| d.name.eq_ignore_ascii_case(&name)) {
            deals.push(Deal {
                name,
                domains: vec![],
                addresses: vec![],
                dotted: vec![],
            });
        }
    }
}
fn live_thread(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM messages m JOIN folders f ON f.id=m.folder_id WHERE m.thread_id=?1 AND {LIVE_FOLDER})"),[id],|r|r.get(0))
}
pub(super) fn apply_membership(
    conn: &Connection,
    deals: &[Deal],
    map: &mut HashMap<i64, usize>,
    details: &Details,
) -> rusqlite::Result<()> {
    for (id, name) in &details.assignments {
        if live_thread(conn, *id)? {
            if let Some(i) = deals.iter().position(|d| d.name.eq_ignore_ascii_case(name)) {
                map.insert(*id, i);
            }
        }
    }
    for id in &details.excluded {
        map.remove(id);
    }
    Ok(())
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeInput {
    pub thread_id: i64,
    pub name: String,
    pub scope: String,
    pub entry: String,
}
#[derive(Debug, Serialize)]
pub struct ScopePreview {
    pub count: usize,
    pub entry: String,
}
fn scoped_deal(conn: &Connection, input: &ScopeInput) -> rusqlite::Result<Deal> {
    let name = checked_name(&input.name)?;
    if !live_thread(conn, input.thread_id)? {
        return Err(invalid(
            "This conversation is no longer in the local mail cache.",
        ));
    }
    if input.scope == "conversation" {
        return Ok(Deal {
            name,
            domains: vec![],
            addresses: vec![],
            dotted: vec![],
        });
    }
    let parsed = parse(&format!("{name}: {}", input.entry), &own_domains(conn)?);
    if parsed.deals.len() != 1 || !parsed.ignored.is_empty() {
        return Err(invalid("Choose one valid company domain or person address. Personal mail providers and your own domain cannot match a whole company."));
    }
    let d = parsed.deals.into_iter().next().unwrap();
    match input.scope.as_str() {
        "person" if d.addresses.len() == 1 && d.domains.is_empty() => {}
        "company" if d.domains.len() == 1 && d.addresses.is_empty() => {}
        _ => {
            return Err(invalid(
                "Choose an email address for This person, or a company domain for Whole company.",
            ))
        }
    }
    Ok(d)
}
pub(super) fn preview_scope(
    conn: &Connection,
    input: &ScopeInput,
) -> rusqlite::Result<ScopePreview> {
    let d = scoped_deal(conn, input)?;
    if input.scope == "conversation" {
        return Ok(ScopePreview {
            count: 1,
            entry: String::new(),
        });
    }
    let mut details = load(conn)?;
    details.assignments.remove(&input.thread_id);
    details.excluded.remove(&input.thread_id);
    let current = queries::get_setting(conn, SETTING)?.unwrap_or_default();
    let name = read_list(conn)?
        .deals
        .into_iter()
        .find(|old| old.name.eq_ignore_ascii_case(&d.name))
        .map(|old| old.name)
        .unwrap_or(d.name.clone());
    let mut deals = parse(&add_to(&current, &name, &input.entry), &own_domains(conn)?).deals;
    extend_catalog(&mut deals, &details);
    let mut ids = scan(conn, &deals)?;
    apply_membership(conn, &deals, &mut ids, &details)?;
    let matching = scan(conn, std::slice::from_ref(&d))?;
    let count = ids
        .iter()
        .filter(|(id, i)| matching.contains_key(id) && deals[**i].name.eq_ignore_ascii_case(&name))
        .count();
    Ok(ScopePreview {
        count,
        entry: d
            .addresses
            .first()
            .or(d.domains.first())
            .cloned()
            .unwrap_or_default(),
    })
}
pub(super) fn apply_scope(conn: &mut Connection, input: &ScopeInput) -> rusqlite::Result<String> {
    let tx = conn.transaction()?;
    let d = scoped_deal(&tx, input)?;
    if preview_scope(&tx, input)?.count == 0 {
        return Err(invalid("This scope adds no conversations. An earlier company rule may already match this address."));
    }
    let mut details = load(&tx)?;
    let current = queries::get_setting(&tx, SETTING)?.unwrap_or_default();
    let canonical = read_list(&tx)?
        .deals
        .into_iter()
        .find(|old| old.name.eq_ignore_ascii_case(&d.name))
        .map(|old| old.name)
        .unwrap_or(d.name.clone());
    let text = if input.scope == "conversation" {
        details.assignments.insert(input.thread_id, canonical);
        current
    } else {
        details.assignments.remove(&input.thread_id);
        add_to(&current, &canonical, &input.entry)
    };
    details.excluded.remove(&input.thread_id);
    save(&tx, &details)?;
    queries::set_setting(&tx, SETTING, &text)?;
    tx.commit()?;
    Ok(text)
}
#[tauri::command]
pub async fn fork_deals_catalog(state: State<'_, AppState>) -> Result<Parsed> {
    state.db.read("fork_deals_catalog", |c| read_list(c)).await
}
#[tauri::command]
pub async fn fork_deals_preview_scope(
    state: State<'_, AppState>,
    input: ScopeInput,
) -> Result<ScopePreview> {
    state
        .db
        .read("fork_deals_preview_scope", move |c| {
            preview_scope(c, &input)
        })
        .await
}
#[tauri::command]
pub async fn fork_deals_apply_scope(
    state: State<'_, AppState>,
    input: ScopeInput,
) -> Result<String> {
    state.db.call(move |c| apply_scope(c, &input)).await
}
#[tauri::command]
pub async fn fork_deals_exclude(
    state: State<'_, AppState>,
    thread_id: i64,
    excluded: bool,
) -> Result<()> {
    state
        .db
        .call(move |c| {
            let mut d = load(c)?;
            if excluded {
                d.excluded.insert(thread_id);
            } else {
                d.excluded.remove(&thread_id);
            }
            save(c, &d)
        })
        .await
}

pub(super) fn attachment(
    conn: &Connection,
    id: i64,
    ids: &[i64],
) -> rusqlite::Result<Option<DealAttachment>> {
    conn.query_row("SELECT a.id,a.message_id,m.thread_id,COALESCE(a.filename,'Attachment'),m.date,a.mime_type,COALESCE(a.size,0) FROM attachments a JOIN messages m ON m.id=a.message_id JOIN folders f ON f.id=m.folder_id WHERE a.id=?1 AND a.is_inline=0 AND (f.role IS NULL OR f.role NOT IN ('trash','junk')) AND m.thread_id IN (SELECT value FROM json_each(?2))",params![id,serde_json::to_string(ids).unwrap_or_default()],|r|Ok(DealAttachment{id:r.get(0)?,message_id:r.get(1)?,thread_id:r.get(2)?,filename:r.get(3)?,date:r.get(4)?,mime_type:r.get(5)?,size:r.get(6)?,is_inline:false})).optional()
}
pub(super) fn save_context(
    conn: &mut Connection,
    company: &str,
    notes: &str,
    pinned_id: Option<i64>,
) -> rusqlite::Result<()> {
    let company = checked_name(company)?;
    if notes.chars().count() > 20000 {
        return Err(invalid("Company notes are limited to 20,000 characters."));
    }
    let cache = Cache::default();
    let membership = cache.membership(conn)?;
    let Some(index) = membership.0.iter().position(|d| d.name == company) else {
        return Err(invalid("This company is no longer in Deals."));
    };
    let ids: Vec<i64> = membership
        .1
        .iter()
        .filter(|(_, i)| **i == index)
        .map(|(id, _)| *id)
        .collect();
    if let Some(id) = pinned_id {
        if attachment(conn, id, &ids)?.is_none() {
            return Err(invalid(
                "Choose a downloaded attachment from this company's conversations.",
            ));
        }
    }
    let mut details = load(conn)?;
    details.notes.insert(company.clone(), notes.to_string());
    if let Some(id) = pinned_id {
        details.pins.insert(company, id);
    } else {
        details.pins.remove(&company);
    }
    save(conn, &details)
}
#[tauri::command]
pub async fn fork_deals_save_context(
    state: State<'_, AppState>,
    company: String,
    notes: String,
    pinned_id: Option<i64>,
) -> Result<()> {
    state
        .db
        .call(move |c| save_context(c, &company, &notes, pinned_id))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        models::{Address, NewMessage},
        Db,
    };
    fn seed(conn: &mut Connection) -> rusqlite::Result<Vec<(i64, i64)>> {
        conn.execute_batch("INSERT INTO accounts(id,email,provider,imap_host,smtp_host,created_at) VALUES('a','me@own.example','custom','i','s',0); INSERT INTO folders(id,account_id,imap_name,role,display_name,sort_order) VALUES(1,'a','INBOX','inbox','Inbox',0),(2,'a','Trash','trash','Trash',0);")?;
        let mut ids = vec![];
        for (uid, folder, from) in [
            (1, 1, "one@alpha.example"),
            (2, 1, "two@alpha.example"),
            (3, 1, "person@beta.example"),
            (4, 2, "gone@alpha.example"),
        ] {
            crate::db::queries::insert_message(
                conn,
                &NewMessage {
                    account_id: "a".into(),
                    folder_id: folder,
                    uid,
                    message_id: Some(format!("<m{uid}>")),
                    subject: Some(format!("Subject {uid}")),
                    from_addr: Some(from.into()),
                    to_addrs: vec![Address {
                        name: None,
                        addr: "me@own.example".into(),
                    }],
                    date: i64::from(uid),
                    ..Default::default()
                },
            )?;
            let pair = conn.query_row(
                "SELECT id,thread_id FROM messages WHERE uid=?1 AND folder_id=?2",
                params![uid, folder],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            ids.push(pair);
        }
        queries::set_setting(conn, SETTING, "Alpha: alpha.example\nBeta: beta.example")?;
        Ok(ids)
    }
    #[test]
    fn scoped_membership_counts_live_mail_and_honours_manual_exclusions() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let ids = seed(c)?;
            let first = ids[0].1;
            let mut input = ScopeInput {
                thread_id: first,
                name: "Alpha".into(),
                scope: "company".into(),
                entry: "alpha.example".into(),
            };
            assert_eq!(preview_scope(c, &input)?.count, 2);
            input.scope = "person".into();
            input.entry = "one@alpha.example".into();
            assert_eq!(preview_scope(c, &input)?.count, 1);
            // The count describes the selected scope and respects earlier company rules.
            input.name = "Person-only".into();
            assert_eq!(preview_scope(c, &input)?.count, 0);
            assert!(apply_scope(c, &input).is_err());
            input.scope = "conversation".into();
            input.name = "Manual".into();
            assert_eq!(preview_scope(c, &input)?.count, 1);
            apply_scope(c, &input)?;
            let cache = Cache::default();
            let rows = list_filtered(c, &cache, 0, 20, Some("Manual"))?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].row.id, first);
            assert_eq!(list_filtered(c, &cache, 0, 20, Some("Alpha"))?.len(), 1);
            let mut d = load(c)?;
            d.excluded.insert(first);
            save(c, &d)?;
            assert!(list_filtered(c, &cache, 0, 20, Some("Manual"))?.is_empty());
            apply_scope(c, &input)?;
            assert_eq!(list_filtered(c, &cache, 0, 20, Some("Manual"))?.len(), 1);
            assert_eq!(suggest(c, first)?.unwrap().deal.as_deref(), Some("Manual"));
            input.scope = "company".into();
            input.entry = "gmail.com".into();
            assert!(preview_scope(c, &input).is_err());
            input.thread_id = 999999;
            input.scope = "conversation".into();
            assert!(preview_scope(c, &input).is_err());
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn private_notes_and_current_document_validate_company_ownership() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c|{
        let ids=seed(c)?;
        c.execute("INSERT INTO attachments(message_id,part_id,filename,mime_type,size) VALUES(?1,'1','terms.pdf','application/pdf',120)",[ids[0].0])?;
        let attachment_id=c.last_insert_rowid();
        save_context(c,"Alpha","Private discussion notes",Some(attachment_id))?;
        let context=super::super::context(c,&Cache::default(),"Alpha",0)?;
        assert_eq!(context.notes,"Private discussion notes");assert_eq!(context.pinned.unwrap().id,attachment_id);
        assert!(save_context(c,"Beta","Wrong attachment",Some(attachment_id)).is_err());
        assert!(save_context(c,"Alpha",&"x".repeat(20001),None).is_err());
        assert_eq!(load(c)?.notes.get("Alpha").unwrap(),"Private discussion notes");
        save_context(c,"Alpha","Updated locally",None)?;assert!(!load(c)?.pins.contains_key("Alpha"));
        queries::set_setting(c,DETAILS_SETTING,"{broken")?;assert!(load(c).is_err());
        assert!(save_context(c,"Alpha","must not overwrite",None).is_err());
        assert_eq!(queries::get_setting(c,DETAILS_SETTING)?.unwrap(),"{broken");
        Ok(())
      }).unwrap();
    }
}
