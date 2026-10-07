use crate::{
    db::bodies,
    error::{Result, SkimError},
    state::AppState,
};
use base64::Engine;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
use tauri::{AppHandle, State};
const PREVIEW_LIMIT: u64 = 25 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    kind: &'static str,
    data_url: Option<String>,
    text: Option<String>,
    truncated: bool,
}

fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

#[tauri::command]
pub async fn fork_attachment_preview(
    state: State<'_, AppState>,
    attachment_id: i64,
) -> Result<Preview> {
    let file = state
        .db
        .read("fork_attachment_preview", move |conn| {
            bodies::get_attachment(conn, attachment_id)
        })
        .await?
        .ok_or_else(|| SkimError::other("attachment", "Attachment not found."))?;
    let path = file.cache_path.ok_or_else(|| {
        SkimError::other(
            "attachment",
            "Download this message before previewing its files.",
        )
    })?;
    tokio::task::spawn_blocking(move || {
        let mut bytes = Vec::new();
        std::fs::File::open(&path)?
            .take(PREVIEW_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > PREVIEW_LIMIT {
            return Err(SkimError::other(
                "attachment",
                "This file exceeds the 25 MB preview limit. Open the original file.",
            ));
        }
        if let Some(mime) = image_type(&bytes) {
            return Ok(Preview {
                kind: "image",
                data_url: Some(format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )),
                text: None,
                truncated: false,
            });
        }
        if bytes.starts_with(b"%PDF-") {
            let document = lopdf::Document::load_mem(&bytes).map_err(|_| {
                SkimError::other(
                    "attachment",
                    "This PDF cannot be read. Open the original file.",
                )
            })?;
            let pages: Vec<u32> = document.get_pages().keys().copied().collect();
            let text = document
                .extract_text(&pages[..pages.len().min(50)])
                .map_err(|_| {
                    SkimError::other(
                        "attachment",
                        "This PDF has no readable text. Open the original file.",
                    )
                })?;
            let truncated = pages.len() > 50 || text.chars().count() > 80000;
            let text = text.chars().take(80000).collect::<String>();
            return Ok(Preview {
                kind: "pdf-text",
                data_url: None,
                text: Some(text),
                truncated,
            });
        }
        Err(SkimError::other(
            "attachment",
            "Preview supports PNG, JPEG, GIF, WebP and PDF text. Open this file in its usual app.",
        ))
    })
    .await?
}

#[derive(Serialize)]
pub struct Fingerprint {
    id: i64,
    sha256: Option<String>,
}
#[tauri::command]
pub async fn fork_attachment_fingerprints(
    state: State<'_, AppState>,
    attachment_ids: Vec<i64>,
) -> Result<Vec<Fingerprint>> {
    if attachment_ids.len() > 200 {
        return Err(SkimError::other("attachment", "Select up to 200 files."));
    }
    let files = state
        .db
        .read("fork_attachment_fingerprints", move |conn| {
            attachment_ids
                .into_iter()
                .map(|id| {
                    bodies::get_attachment(conn, id)
                        .map(|file| (id, file.and_then(|f| f.cache_path)))
                })
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    tokio::task::spawn_blocking(move || {
        let mut budget = 128 * 1024 * 1024_u64;
        let mut result = vec![];
        for (id, path) in files {
            let digest = path.and_then(|path| {
                let mut file = std::fs::File::open(path).ok()?;
                let size = file.metadata().ok()?.len();
                if size > budget {
                    return None;
                }
                budget -= size;
                let mut hash = Sha256::new();
                let mut buf = [0u8; 65536];
                let mut read = 0;
                loop {
                    let n = file.read(&mut buf).ok()?;
                    if n == 0 {
                        break;
                    }
                    read += n as u64;
                    if read > size {
                        return None;
                    }
                    hash.update(&buf[..n]);
                }
                Some(
                    hash.finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>(),
                )
            });
            result.push(Fingerprint { id, sha256: digest });
        }
        result
    })
    .await
    .map_err(Into::into)
}

pub fn safe_filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(150)
        .collect();
    let clean = clean.trim().trim_end_matches(['.', ' ']);
    if clean.is_empty() {
        return "attachment".into();
    }
    let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        })
    {
        format!("_{clean}")
    } else {
        clean.into()
    }
}

fn copy_unique(source: &Path, directory: &Path, name: &str) -> std::io::Result<String> {
    let name = safe_filename(name);
    let path = Path::new(&name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment");
    let ext = path.extension().and_then(|s| s.to_str());
    for suffix in 0..10000 {
        let candidate = if suffix == 0 {
            name.clone()
        } else {
            format!(
                "{stem} ({suffix}){}",
                ext.map(|e| format!(".{e}")).unwrap_or_default()
            )
        };
        let target = directory.join(&candidate);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut output) => {
                let copied = (|| {
                    let mut input = std::fs::File::open(source)?;
                    std::io::copy(&mut input, &mut output)?;
                    output.flush()
                })();
                if let Err(error) = copied {
                    drop(output);
                    let _ = std::fs::remove_file(target);
                    return Err(error);
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("Too many files share this filename."))
}

#[derive(Serialize)]
pub struct SaveResult {
    saved: usize,
    failed: Vec<String>,
    cancelled: bool,
}
#[tauri::command]
pub async fn fork_save_attachments(
    app: AppHandle,
    state: State<'_, AppState>,
    attachment_ids: Vec<i64>,
) -> Result<SaveResult> {
    use tauri_plugin_dialog::DialogExt;
    if attachment_ids.is_empty() || attachment_ids.len() > 200 {
        return Err(SkimError::other(
            "attachment",
            "Select between 1 and 200 files.",
        ));
    }
    let files = state
        .db
        .read("fork_save_attachments", move |conn| {
            attachment_ids
                .into_iter()
                .map(|id| bodies::get_attachment(conn, id))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .await?;
    let dialog = app.dialog().file();
    let picked = tokio::task::spawn_blocking(move || dialog.blocking_pick_folder()).await?;
    let Some(directory) = picked else {
        return Ok(SaveResult {
            saved: 0,
            failed: vec![],
            cancelled: true,
        });
    };
    let directory = directory
        .into_path()
        .map_err(|e| SkimError::other("attachment", e.to_string()))?;
    tokio::task::spawn_blocking(move || {
        let mut result = SaveResult {
            saved: 0,
            failed: vec![],
            cancelled: false,
        };
        for file in files {
            let Some(file) = file else {
                result.failed.push("Missing attachment".into());
                continue;
            };
            let name = file.filename.unwrap_or_else(|| "attachment".into());
            match file
                .cache_path
                .and_then(|path| copy_unique(Path::new(&path), &directory, &name).ok())
            {
                Some(_) => result.saved += 1,
                None => result.failed.push(name),
            }
        }
        result
    })
    .await
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filename_safety_and_distinct_versions_are_preserved() {
        assert_eq!(safe_filename("../../CON.txt"), ".._.._CON.txt");
        assert_eq!(safe_filename("CON.txt"), "_CON.txt");
        assert_eq!(safe_filename("report.pdf. "), "report.pdf");
        let dir = std::env::temp_dir().join(format!("skim-save-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let source = dir.join("source");
        std::fs::write(&source, b"version 1").unwrap();
        copy_unique(&source, &dir, "report.pdf").unwrap();
        std::fs::write(&source, b"version 2").unwrap();
        assert_eq!(
            copy_unique(&source, &dir, "report.pdf").unwrap(),
            "report (1).pdf"
        );
        assert_eq!(std::fs::read(dir.join("report.pdf")).unwrap(), b"version 1");
        assert_eq!(
            std::fs::read(dir.join("report (1).pdf")).unwrap(),
            b"version 2"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn active_svg_and_html_never_become_image_previews() {
        assert!(image_type(b"<svg onload=alert(1)>").is_none());
        assert!(image_type(b"<html>").is_none());
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nimage"), Some("image/png"));
    }
}
