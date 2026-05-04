use regex::Regex;
use std::path::PathBuf;

pub fn execute_batch_rename(
    targets: &[PathBuf],
    pattern: &str,
    replace: &str,
) -> Result<usize, String> {
    let re = Regex::new(pattern).map_err(|e| format!("Invalid Regex: {}", e))?;
    let mut renamed_count = 0;

    for (index, target) in targets.iter().enumerate() {
        if let Some(name) = target.file_name().and_then(|n| n.to_str())
            && re.is_match(name)
        {
            let temp_name = re.replace(name, replace).into_owned();
            let final_name = temp_name.replace("#SEQ", &(index + 1).to_string());
            let new_path = target.with_file_name(final_name);

            if std::fs::rename(target, new_path).is_ok() {
                renamed_count += 1;
            }
        }
    }
    Ok(renamed_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_execute_batch_rename() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("vacation_img1.jpg");
        let file2 = dir.path().join("vacation_img2.jpg");
        let file3 = dir.path().join("ignore_me.txt");

        // Create dummy files
        File::create(&file1).unwrap();
        File::create(&file2).unwrap();
        File::create(&file3).unwrap();

        let targets = vec![file1.clone(), file2.clone(), file3.clone()];

        // Rename "vacation_imgX.jpg" to "Nova_Terra_#SEQ.jpg"
        let pattern = r"^vacation_img\d\.jpg$";
        let replace = "Nova_Terra_#SEQ.jpg";

        let renamed_count = execute_batch_rename(&targets, pattern, replace).unwrap();

        // ensure exactly 2 files were renamed
        // Note: ignore_me.txt should be skipped)
        assert_eq!(renamed_count, 2);

        // Verify new files exist
        assert!(dir.path().join("Nova_Terra_1.jpg").exists());
        assert!(dir.path().join("Nova_Terra_2.jpg").exists());

        // Verify old files are gone
        assert!(!file1.exists());
        assert!(!file2.exists());

        // Verify the ignored file is untouched
        assert!(file3.exists());
    }

    #[test]
    fn test_batch_rename_invalid_regex() {
        let targets = vec![PathBuf::from("dummy.txt")];
        // Missing closing bracket in regex
        let result = execute_batch_rename(&targets, "[a-z", "replace");

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid Regex"));
    }

    #[test]
    fn test_batch_rename_capture_groups() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("IMG_2023.jpg");
        File::create(&file1).unwrap();

        let targets = vec![file1.clone()];

        // renames "IMG_YYYY.ext" to "Photo_YYYY.ext" using capture groups ($1, $2)
        let pattern = r"^IMG_(\d{4})\.(jpg)$";
        let replace = "Photo_$1.$2";

        let renamed_count = execute_batch_rename(&targets, pattern, replace).unwrap();

        assert_eq!(renamed_count, 1);
        assert!(dir.path().join("Photo_2023.jpg").exists());
        assert!(!file1.exists());
    }
}
