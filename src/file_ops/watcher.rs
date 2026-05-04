// - Hawkwind?

use crate::types::FileEntry;
use notify::EventKind;
use std::path::Path;

pub fn apply_delta(entries: &mut Vec<FileEntry>, event: notify::Event, current_dir: &Path) -> bool {
    let mut modified = false;
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if path.parent() == Some(current_dir)
                    && let Ok(metadata) = std::fs::metadata(&path)
                {
                    entries.push(crate::file_ops::read::create_entry(path, metadata));
                    modified = true;
                }
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                let initial_len = entries.len();
                entries.retain(|e| e.path != path);
                if entries.len() < initial_len {
                    modified = true;
                }
            }
        }
        EventKind::Modify(_) => {
            for path in event.paths {
                if let Some(entry) = entries.iter_mut().find(|e| e.path == path)
                    && let Ok(metadata) = std::fs::metadata(&path)
                {
                    *entry = crate::file_ops::read::create_entry(path.clone(), metadata);
                    modified = true;
                }
            }
        }
        _ => {}
    }
    modified
}
