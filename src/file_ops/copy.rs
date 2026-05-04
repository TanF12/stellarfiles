use crate::errors::AppError;
use crate::types::ProgressMsg;
use nix::fcntl::copy_file_range;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn chunked_copy_fallback(
    mut src: &std::fs::File,
    mut dst: &std::fs::File,
    size: u64,
    tx: &async_channel::Sender<ProgressMsg>,
    id: usize,
    filename: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<(), AppError> {
    let mut buffer = vec![0u8; 1024 * 1024 * 4];
    let mut remaining = size;
    let mut bytes_acc = 0;
    let mut last_report = std::time::Instant::now();

    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let n = src.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buffer[..n])?;

        remaining -= n as u64;
        bytes_acc += n as u64;

        if bytes_acc > 1_048_576 && last_report.elapsed().as_millis() > 50 {
            let _ = tx.send_blocking(ProgressMsg::Update {
                id,
                bytes_chunk: bytes_acc,
                active_file: filename.to_string(),
            });
            bytes_acc = 0;
            last_report = std::time::Instant::now();
        }
    }
    if bytes_acc > 0 {
        let _ = tx.send_blocking(ProgressMsg::Update {
            id,
            bytes_chunk: bytes_acc,
            active_file: filename.to_string(),
        });
    }
    Ok(())
}

fn zero_copy_file(
    src: &std::fs::File,
    dst: &std::fs::File,
    size: u64,
    tx: &async_channel::Sender<ProgressMsg>,
    id: usize,
    filename: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<(), AppError> {
    #[cfg(target_os = "linux")]
    {
        let mut remaining = size as i64;
        let mut bytes_acc = 0;
        let mut last_report = std::time::Instant::now();
        let chunk_size = 4 * 1024 * 1024;

        while remaining > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let to_copy = remaining.min(chunk_size);

            match copy_file_range(src.as_fd(), None, dst.as_fd(), None, to_copy as usize) {
                Ok(0) => break,
                Ok(n) => {
                    remaining -= n as i64;
                    bytes_acc += n as u64;

                    if bytes_acc > 1_048_576 && last_report.elapsed().as_millis() > 50 {
                        let _ = tx.send_blocking(ProgressMsg::Update {
                            id,
                            bytes_chunk: bytes_acc,
                            active_file: filename.to_string(),
                        });
                        bytes_acc = 0;
                        last_report = std::time::Instant::now();
                    }
                }
                Err(nix::errno::Errno::EXDEV) | Err(nix::errno::Errno::ENOSYS) => {
                    return chunked_copy_fallback(src, dst, size, tx, id, filename, cancel);
                }
                Err(e) => return Err(std::io::Error::from(e).into()),
            }
        }

        if bytes_acc > 0 {
            let _ = tx.send_blocking(ProgressMsg::Update {
                id,
                bytes_chunk: bytes_acc,
                active_file: filename.to_string(),
            });
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        chunked_copy_fallback(src, dst, size, tx, id, filename, cancel)
    }
}

pub fn copy_recursive_safe(
    src: &Path,
    dst: &Path,
    tx: &async_channel::Sender<ProgressMsg>,
    overwrite: bool,
    id: usize,
    cancel: &Arc<AtomicBool>,
) -> Result<(), AppError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::Cancelled);
    }
    let meta = fs::symlink_metadata(src)?;
    crate::math::check_security(&meta)?;

    if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        if overwrite && dst.exists() {
            let _ = std::fs::remove_file(dst).or_else(|_| std::fs::remove_dir_all(dst));
        }
        std::os::unix::fs::symlink(target, dst)?;
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive_safe(
                &entry.path(),
                &dst.join(entry.file_name()),
                tx,
                overwrite,
                id,
                cancel,
            )?;
        }
    } else {
        let src_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(src)?;
        let mut opts = OpenOptions::new();
        opts.write(true);
        if overwrite {
            opts.create(true).truncate(true);
        } else {
            opts.create_new(true);
        }
        let dst_file = opts.open(dst)?;
        zero_copy_file(
            &src_file,
            &dst_file,
            meta.len(),
            tx,
            id,
            &src.file_name().unwrap_or_default().to_string_lossy(),
            cancel,
        )?;
    }
    Ok(())
}
