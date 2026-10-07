//! Event controls stored with each cached event and sent only when changed.
use crate::error::{Result, SkimError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<EventAttachment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminders: Option<Reminders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guests_can_modify: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guests_can_invite_others: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guests_can_see_other_guests: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventAttachment {
    pub file_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reminders {
    pub use_default: bool,
    #[serde(default)]
    pub overrides: Vec<Reminder>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reminder {
    pub method: String,
    pub minutes: i64,
}

impl EventOptions {
    pub fn validate(&self) -> Result<()> {
        let bad = |s| SkimError::other("gcal_input", s);
        if self
            .visibility
            .as_deref()
            .is_some_and(|s| !["default", "public", "private", "confidential"].contains(&s))
        {
            return Err(bad("Invalid event visibility"));
        }
        if self
            .transparency
            .as_deref()
            .is_some_and(|s| !["opaque", "transparent"].contains(&s))
        {
            return Err(bad("Invalid event availability"));
        }
        if self
            .color_id
            .as_deref()
            .is_some_and(|s| !s.is_empty() && !s.parse::<u8>().is_ok_and(|v| (1..=11).contains(&v)))
        {
            return Err(bad("Invalid event colour"));
        }
        if let Some(files) = &self.attachments {
            if files.len() > 25
                || files.iter().any(|f| {
                    f.file_url.len() > 4096
                        || f.title.as_ref().is_some_and(|s| s.len() > 512)
                        || !url::Url::parse(&f.file_url).is_ok_and(|u| {
                            u.scheme() == "https"
                                && u.host_str().is_some()
                                && u.username().is_empty()
                                && u.password().is_none()
                        })
                })
            {
                return Err(bad(
                    "Use up to 25 HTTPS attachment links with no embedded credentials.",
                ));
            }
        }
        if let Some(r) = &self.reminders {
            if r.overrides.len() > 5
                || r.overrides.iter().any(|v| {
                    !["popup", "email"].contains(&v.method.as_str())
                        || !(0..=40320).contains(&v.minutes)
                })
            {
                return Err(bad(
                    "Use up to five reminders, each between zero minutes and four weeks",
                ));
            }
        }
        if let Some(lines) = &self.recurrence {
            if lines.len() > 32
                || lines.iter().any(|v| {
                    v.len() > 4096
                        || v.contains(['\r', '\n'])
                        || ![
                            "RRULE:", "RDATE:", "RDATE;", "EXDATE:", "EXDATE;", "EXRULE:",
                        ]
                        .iter()
                        .any(|p| v.starts_with(p))
                })
            {
                return Err(bad("Invalid event recurrence"));
            }
        }
        Ok(())
    }
    pub fn merge(&mut self, patch: &Self) {
        macro_rules! set { ($($f:ident),*) => { $(if patch.$f.is_some() { self.$f = patch.$f.clone(); })* }; }
        set!(
            attachments,
            recurrence,
            reminders,
            visibility,
            color_id,
            transparency,
            guests_can_modify,
            guests_can_invite_others,
            guests_can_see_other_guests
        );
    }
}

/// Add links to the fresh remote list. An old local cache must not remove a file.
pub fn merge_attachment_additions(
    current: &serde_json::Value,
    requested: &serde_json::Value,
) -> Result<serde_json::Value> {
    let mut files = current.as_array().cloned().unwrap_or_default();
    for file in requested.as_array().into_iter().flatten() {
        if !files
            .iter()
            .any(|existing| existing["fileUrl"] == file["fileUrl"])
        {
            files.push(file.clone());
        }
    }
    if files.len() > 25 {
        return Err(SkimError::other(
            "gcal_input",
            "This event already has too many attachment links.",
        ));
    }
    Ok(serde_json::Value::Array(files))
}
