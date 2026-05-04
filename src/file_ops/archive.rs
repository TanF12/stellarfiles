use crate::errors::AppError;
use std::fs;
use std::io::{Read, Write};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct ProgressReader<'a, R: Read> {
    inner: R,
    tx: &'a async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    file_name: String,
    last_report: std::time::Instant,
    bytes_acc: u64,
    cancel: Arc<AtomicBool>,
}

impl<'a, R: Read> Read for ProgressReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("Cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.bytes_acc += n as u64;
        if self.last_report.elapsed().as_millis() > 50 {
            let _ = self.tx.send_blocking(crate::types::ProgressMsg::Update {
                id: self.id,
                bytes_chunk: self.bytes_acc,
                active_file: self.file_name.clone(),
            });
            self.bytes_acc = 0;
            self.last_report = std::time::Instant::now();
        }
        Ok(n)
    }
}

impl<'a, R: Read> Drop for ProgressReader<'a, R> {
    fn drop(&mut self) {
        if self.bytes_acc > 0 {
            let _ = self.tx.send_blocking(crate::types::ProgressMsg::Update {
                id: self.id,
                bytes_chunk: self.bytes_acc,
                active_file: self.file_name.clone(),
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn compress_path(
    path: PathBuf,
    dest_name: String,
    format: String,
    level: String,
    current_dir: PathBuf,
    tx: async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let total_bytes = crate::file_ops::read::get_size(&path);
        let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });
        let dest_path = current_dir.join(dest_name);

        if format == "7z" {
            let mx = match level.as_str() {
                "Fast" => "-mx=1",
                "Maximum" => "-mx=9",
                _ => "-mx=5",
            };
            let status = std::process::Command::new("7z")
                .arg("a")
                .arg(mx)
                .arg(&dest_path)
                .arg(&path)
                .status()
                .map_err(|e| AppError::Archive(format!("7z command failed: {}", e)))?;
            if !status.success() {
                return Err(AppError::Archive("7z compression failed".into()));
            }
            return Ok(format!(
                "Successfully compressed to {}",
                dest_path.display()
            ));
        }

        let file = fs::File::create(&dest_path)?;

        if format == "zip" {
            let mut zip = zip::ZipWriter::new(file);
            let lvl = match level.as_str() {
                "Fast" => 1,
                "Maximum" => 9,
                _ => 5,
            };
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(lvl));

            if path.is_dir() {
                for entry in walkdir::WalkDir::new(&path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(AppError::Cancelled);
                    }
                    let p = entry.path();
                    let name = p.strip_prefix(&path).unwrap_or(p);
                    if name.as_os_str().is_empty() {
                        continue;
                    }

                    if p.is_file() {
                        zip.start_file(name.to_string_lossy().into_owned(), options)
                            .map_err(|e| AppError::Archive(e.to_string()))?;
                        let f = fs::File::open(p)?;
                        let file_name = name.to_string_lossy().into_owned();
                        let mut pr = ProgressReader {
                            inner: f,
                            tx: &tx,
                            id,
                            file_name,
                            last_report: std::time::Instant::now(),
                            bytes_acc: 0,
                            cancel: cancel.clone(),
                        };
                        std::io::copy(&mut pr, &mut zip)?;
                    } else if p.is_dir() {
                        zip.add_directory(name.to_string_lossy().into_owned(), options)
                            .map_err(|e| AppError::Archive(e.to_string()))?;
                    }
                }
            } else {
                zip.start_file(
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    options,
                )
                .map_err(|e| AppError::Archive(e.to_string()))?;
                let f = fs::File::open(&path)?;
                let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
                let mut pr = ProgressReader {
                    inner: f,
                    tx: &tx,
                    id,
                    file_name,
                    last_report: std::time::Instant::now(),
                    bytes_acc: 0,
                    cancel: cancel.clone(),
                };
                std::io::copy(&mut pr, &mut zip)?;
            }
            zip.finish().map_err(|e| AppError::Archive(e.to_string()))?;
        } else {
            let enc: Box<dyn std::io::Write> = if format == "tar.zst" {
                let lvl = match level.as_str() {
                    "Fast" => 1,
                    "Maximum" => 19,
                    _ => 3,
                };
                Box::new(
                    zstd::stream::Encoder::new(file, lvl)
                        .map_err(|e| AppError::Archive(e.to_string()))?
                        .auto_finish(),
                )
            } else {
                let lvl = match level.as_str() {
                    "Fast" => flate2::Compression::fast(),
                    "Maximum" => flate2::Compression::best(),
                    _ => flate2::Compression::default(),
                };
                Box::new(flate2::write::GzEncoder::new(file, lvl))
            };

            let mut tar = tar::Builder::new(enc);
            if path.is_dir() {
                let base_name = path.file_name().unwrap();
                for entry in walkdir::WalkDir::new(&path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(AppError::Cancelled);
                    }
                    let p = entry.path();
                    let name = p.strip_prefix(&path).unwrap_or(p);
                    if name.as_os_str().is_empty() {
                        continue;
                    }

                    let dest_name = std::path::Path::new(base_name).join(name);
                    if p.is_file() {
                        let f = fs::File::open(p)?;
                        let meta = f.metadata()?;
                        let mut header = tar::Header::new_gnu();
                        header.set_size(meta.len());
                        header.set_mode(meta.mode());
                        header.set_mtime(meta.mtime() as u64);
                        header.set_cksum();

                        let file_name = dest_name.to_string_lossy().into_owned();
                        let mut pr = ProgressReader {
                            inner: f,
                            tx: &tx,
                            id,
                            file_name,
                            last_report: std::time::Instant::now(),
                            bytes_acc: 0,
                            cancel: cancel.clone(),
                        };
                        tar.append_data(&mut header, &dest_name, &mut pr)?;
                    } else if p.is_dir() {
                        tar.append_dir(&dest_name, p)?;
                    }
                }
            } else {
                let f = fs::File::open(&path)?;
                let meta = f.metadata()?;
                let mut header = tar::Header::new_gnu();
                header.set_size(meta.len());
                header.set_mode(meta.mode());
                header.set_mtime(meta.mtime() as u64);
                header.set_cksum();

                let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
                let mut pr = ProgressReader {
                    inner: f,
                    tx: &tx,
                    id,
                    file_name,
                    last_report: std::time::Instant::now(),
                    bytes_acc: 0,
                    cancel: cancel.clone(),
                };
                tar.append_data(&mut header, path.file_name().unwrap(), &mut pr)?;
            }
            tar.finish()?;
        }
        Ok(format!(
            "Successfully compressed to {}",
            dest_path.display()
        ))
    })
    .await
    .unwrap_or_else(|e| Err(AppError::Task(e.to_string())))
}

pub async fn extract_archive(
    path: PathBuf,
    dest: PathBuf,
    tx: async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let safe_dest = dest.canonicalize().unwrap_or_else(|_| dest.clone());

        if ext == "7z" {
            sevenz_rust::decompress_file(&path, &safe_dest)
                .map_err(|e| AppError::Archive(format!("7z extract failed: {}", e)))?;
            return Ok("Extracted successfully".into());
        }

        if ext == "zip" {
            let file = fs::File::open(&path)?;
            let mut archive =
                zip::ZipArchive::new(file).map_err(|e| AppError::Archive(e.to_string()))?;
            let mut total_bytes = 0;
            for i in 0..archive.len() {
                if let Ok(item) = archive.by_index(i) {
                    total_bytes += item.size();
                }
            }
            let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });

            for i in 0..archive.len() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Cancelled);
                }
                let mut item = archive
                    .by_index(i)
                    .map_err(|e| AppError::Archive(e.to_string()))?;

                let outpath = match item.enclosed_name() {
                    Some(p) => safe_dest.join(p),
                    None => continue,
                };

                if outpath.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::RootDir
                    )
                }) {
                    continue;
                }

                if (*item.name()).ends_with('/') {
                    fs::create_dir_all(&outpath).ok();
                } else {
                    if let Some(p) = outpath.parent() {
                        fs::create_dir_all(p).ok();
                    }
                    if outpath.is_dir() {
                        fs::remove_dir_all(&outpath).ok();
                    }

                    let mut outfile = fs::File::create(&outpath)?;
                    let file_name = item.name().to_string();

                    let mut pr = ProgressReader {
                        inner: &mut item,
                        tx: &tx,
                        id,
                        file_name,
                        last_report: std::time::Instant::now(),
                        bytes_acc: 0,
                        cancel: cancel.clone(),
                    };
                    std::io::copy(&mut pr, &mut outfile)?;
                }
            }
        } else if ext == "gz" || ext == "tgz" || ext == "zst" {
            let file = fs::File::open(&path)?;
            let total_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });

            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let pr = ProgressReader {
                inner: file,
                tx: &tx,
                id,
                file_name,
                last_report: std::time::Instant::now(),
                bytes_acc: 0,
                cancel: cancel.clone(),
            };

            let mut archive = if ext == "zst" {
                let dec =
                    zstd::stream::Decoder::new(pr).map_err(|e| AppError::Archive(e.to_string()))?;
                tar::Archive::new(Box::new(dec) as Box<dyn std::io::Read>)
            } else {
                let gz = flate2::read::GzDecoder::new(pr);
                tar::Archive::new(Box::new(gz) as Box<dyn std::io::Read>)
            };

            archive.set_unpack_xattrs(false);

            let mut chunk_buf = vec![0u8; 2 * 1024 * 1024];

            for entry in archive.entries()? {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Cancelled);
                }
                let mut entry = entry?;
                let p = entry.path()?.into_owned();

                if p.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::RootDir
                    )
                }) {
                    continue;
                }
                if entry.header().entry_type().is_symlink()
                    && let Ok(Some(target)) = entry.link_name()
                    && target
                        .components()
                        .any(|c| matches!(c, std::path::Component::RootDir))
                {
                    continue;
                }

                let outpath = safe_dest.join(&p);
                if entry.header().entry_type().is_dir() {
                    fs::create_dir_all(&outpath).ok();
                } else {
                    if let Some(par) = outpath.parent() {
                        fs::create_dir_all(par).ok();
                    }
                    if outpath.is_dir() {
                        fs::remove_dir_all(&outpath).ok();
                    }
                    let mut outfile = fs::File::create(&outpath)?;

                    #[cfg(target_os = "linux")]
                    unsafe {
                        libc::posix_fadvise(outfile.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL);
                    }

                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(AppError::Cancelled);
                        }
                        let n = entry.read(&mut chunk_buf)?;
                        if n == 0 {
                            break;
                        }
                        outfile.write_all(&chunk_buf[..n])?;
                    }
                }
            }
        }
        Ok("Extracted successfully".into())
    })
    .await
    .unwrap_or_else(|e| Err(AppError::Task(e.to_string())))
}
