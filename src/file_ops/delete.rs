use crate::errors::AppError;
use std::path::Path;

pub fn safe_remove_recursively(path: &Path) -> Result<(), AppError> {
    // force remove readonly tags if possible
    if let Ok(m) = std::fs::symlink_metadata(path) {
        let mut perms = m.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
        }
    }

    if path.is_dir() && !std::fs::symlink_metadata(path)?.is_symlink() {
        if std::fs::remove_dir_all(path).is_err() {
            // if dir wipe failed, forcefully unlock all children and retry
            for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
                let p = entry.path();
                if let Ok(m) = std::fs::symlink_metadata(p) {
                    let mut perms = m.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(p, perms);
                }
            }
            std::fs::remove_dir_all(path).map_err(AppError::Io)?;
        }
    } else {
        std::fs::remove_file(path).map_err(AppError::Io)?;
    }
    Ok(())
}
