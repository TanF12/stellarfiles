use crate::errors::AppError;
use rand::Rng;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

pub async fn shred_paths(paths: Vec<PathBuf>) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let mut rng = rand::rng();
        let mut shredded = 0;

        for base_path in paths {
            for entry in walkdir::WalkDir::new(&base_path)
                .contents_first(true)
                .into_iter()
                .flatten()
            {
                let path = entry.path();
                let meta = match fs::symlink_metadata(path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if crate::math::check_security(&meta).is_err() {
                    continue;
                }

                if meta.file_type().is_file()
                    && meta.len() > 0
                    && let Ok(mut file) = OpenOptions::new()
                        .write(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(path)
                {
                    let len = meta.len();
                    let chunk_size = 4096.min(len) as usize;
                    let iterations = (len / chunk_size as u64) + 1;

                    let zeros = vec![0u8; chunk_size];
                    let ones = vec![0xFFu8; chunk_size];

                    let _ = file.seek(SeekFrom::Start(0));
                    for _ in 0..iterations {
                        let _ = file.write_all(&zeros);
                    }
                    let _ = file.sync_data();

                    let _ = file.seek(SeekFrom::Start(0));
                    for _ in 0..iterations {
                        let _ = file.write_all(&ones);
                    }
                    let _ = file.sync_data();

                    let _ = file.seek(SeekFrom::Start(0));
                    for _ in 0..iterations {
                        let mut rand_buf = vec![0u8; chunk_size];
                        rng.fill_bytes(&mut rand_buf);
                        let _ = file.write_all(&rand_buf);
                    }
                    let _ = file.sync_data();
                }

                let mut new_name = String::new();
                for _ in 0..16 {
                    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
                    new_name.push(chars[(rng.next_u32() as usize) % chars.len()]);
                }
                let obfuscated_path = path.with_file_name(new_name);
                let _ = fs::rename(path, &obfuscated_path);

                if meta.is_dir() {
                    fs::remove_dir(&obfuscated_path).ok();
                } else {
                    fs::remove_file(&obfuscated_path).ok();
                }
                shredded += 1;
            }
        }
        Ok(format!("Securely shredded {} item(s).", shredded))
    })
    .await
    .unwrap_or_else(|e| Err(AppError::Task(e.to_string())))
}
