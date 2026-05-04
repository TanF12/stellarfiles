use crate::errors::AppError;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

pub fn format_bytes(size: u64) -> String {
    const KB: u64 = 1_000;
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{} KB", size / KB)
    } else {
        format!("{} B", size)
    }
}

pub fn sanitize_filename(name: &str) -> Result<&str, AppError> {
    let name_trim = name.trim();
    if name_trim.is_empty()
        || name_trim == "."
        || name_trim == ".."
        || name_trim.contains('/')
        || name_trim.contains('\0')
    {
        Err(AppError::security("Invalid filename."))
    } else {
        Ok(name)
    }
}

pub fn check_security(meta: &std::fs::Metadata) -> Result<(), AppError> {
    let ft = meta.file_type();
    if ft.is_block_device() || ft.is_char_device() || ft.is_fifo() || ft.is_socket() {
        return Err(AppError::security(
            "Target is a system device. Cannot operate safely.",
        ));
    }
    if meta.nlink() > 1 {
        eprintln!("Stellar Warning: Operating on file with > 1 hard links.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");

        assert_eq!(format_bytes(1_729), "1 KB");
        assert_eq!(format_bytes(1_618_033), "1.6 MB");
        assert_eq!(format_bytes(3_141_592_653), "3.1 GB");
    }

    #[test]
    fn test_sanitize_filename() {
        // valid names
        assert_eq!(
            sanitize_filename("banach-tarski_sphere.obj").unwrap(),
            "banach-tarski_sphere.obj"
        );
        assert_eq!(
            sanitize_filename("  flying teapot  ").unwrap(),
            "  flying teapot  "
        );

        // invalid names
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("   ").is_err());
        assert!(sanitize_filename(".").is_err());
        assert!(sanitize_filename("..").is_err());
        assert!(sanitize_filename("hawkwind/slash.txt").is_err());
        assert!(sanitize_filename("null\0byte").is_err());
    }

    #[test]
    fn test_check_security_standard_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let meta = temp_file.path().metadata().expect("Failed to get metadata");

        let result = check_security(&meta);
        assert!(result.is_ok(), "Standard files should pass security checks");
    }
}
