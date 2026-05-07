use crate::errors::AppError;
use std::collections::HashSet;
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
        if n == 0 {
            return Ok(0);
        }
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

        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&dest_path)?;

        let buf_writer = std::io::BufWriter::with_capacity(4 * 1024 * 1024, file);

        if format == "zip" {
            let mut zip = zip::ZipWriter::new(buf_writer);
            let lvl = match level.as_str() {
                "Fast" => 1,
                "Maximum" => 9,
                _ => 5,
            };

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(lvl))
                .large_file(true);

            let mut chunk_buf = vec![0u8; 4 * 1024 * 1024];

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

                        let mut f = fs::File::open(p)?;
                        let file_name = name.to_string_lossy().into_owned();
                        let mut pr = ProgressReader {
                            inner: &mut f,
                            tx: &tx,
                            id,
                            file_name,
                            last_report: std::time::Instant::now(),
                            bytes_acc: 0,
                            cancel: cancel.clone(),
                        };

                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                return Err(AppError::Cancelled);
                            }
                            let n = std::io::Read::read(&mut pr, &mut chunk_buf)?;
                            if n == 0 {
                                break;
                            }
                            std::io::Write::write_all(&mut zip, &chunk_buf[..n])?;
                        }
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

                let mut f = fs::File::open(&path)?;
                let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
                let mut pr = ProgressReader {
                    inner: &mut f,
                    tx: &tx,
                    id,
                    file_name,
                    last_report: std::time::Instant::now(),
                    bytes_acc: 0,
                    cancel: cancel.clone(),
                };

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(AppError::Cancelled);
                    }
                    let n = std::io::Read::read(&mut pr, &mut chunk_buf)?;
                    if n == 0 {
                        break;
                    }
                    std::io::Write::write_all(&mut zip, &chunk_buf[..n])?;
                }
            }

            let mut inner_writer = zip.finish().map_err(|e| AppError::Archive(e.to_string()))?;
            inner_writer.flush()?;
        } else {
            let enc: Box<dyn std::io::Write> = if format == "tar.zst" {
                let lvl = match level.as_str() {
                    "Fast" => 1,
                    "Maximum" => 19,
                    _ => 3,
                };
                Box::new(
                    zstd::stream::Encoder::new(buf_writer, lvl)
                        .map_err(|e| AppError::Archive(e.to_string()))?
                        .auto_finish(),
                )
            } else {
                let lvl = match level.as_str() {
                    "Fast" => flate2::Compression::fast(),
                    "Maximum" => flate2::Compression::best(),
                    _ => flate2::Compression::default(),
                };
                Box::new(flate2::write::GzEncoder::new(buf_writer, lvl))
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
            let mut inner_writer = tar
                .into_inner()
                .map_err(|e| AppError::Archive(e.to_string()))?;
            inner_writer.flush()?;
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

        let mut created_dirs = HashSet::with_capacity(512);

        if ext == "zip" {
            let file = fs::File::open(&path)?;

            #[cfg(target_os = "linux")]
            unsafe {
                libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_RANDOM);
            }

            let reader = std::io::BufReader::with_capacity(128 * 1024, file);
            let mut archive =
                zip::ZipArchive::new(reader).map_err(|e| AppError::Archive(e.to_string()))?;

            let mut total_bytes = 0;
            for i in 0..archive.len() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Cancelled);
                }
                if let Ok(item) = archive.by_index(i) {
                    total_bytes += item.size();
                }
            }
            let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });

            let mut chunk_buf = vec![0u8; 4 * 1024 * 1024];

            for i in 0..archive.len() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Cancelled);
                }

                let mut item = archive
                    .by_index(i)
                    .map_err(|e| AppError::Archive(e.to_string()))?;

                let p = match item.enclosed_name() {
                    Some(name) => name,
                    None => continue,
                };

                let outpath = safe_dest.join(&p);
                let is_dir = item.is_dir() || (*item.name()).ends_with('/');

                if is_dir {
                    if created_dirs.insert(outpath.clone()) {
                        fs::create_dir_all(&outpath).ok();
                    }
                    continue;
                } else if let Some(parent) = outpath.parent()
                    && created_dirs.insert(parent.to_path_buf())
                {
                    fs::create_dir_all(parent).ok();
                }

                let mut outfile = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&outpath)?;

                let size = item.size();

                #[cfg(target_os = "linux")]
                unsafe {
                    let fd = outfile.as_raw_fd();
                    libc::posix_fadvise(fd, 0, 0, libc::POSIX_FADV_SEQUENTIAL);
                    if size > 0 {
                        libc::fallocate(fd, 0, 0, size as libc::off_t);
                    }
                }

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

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(AppError::Cancelled);
                    }
                    let n = std::io::Read::read(&mut pr, &mut chunk_buf)?;
                    if n == 0 {
                        break;
                    }
                    std::io::Write::write_all(&mut outfile, &chunk_buf[..n])?;
                }

                #[cfg(target_os = "linux")]
                unsafe {
                    libc::posix_fadvise(outfile.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
                }
            }
        } else if ext == "gz" || ext == "tgz" || ext == "zst" || ext == "tar" {
            let file = fs::File::open(&path)?;
            let total_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });

            #[cfg(target_os = "linux")]
            unsafe {
                libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL);
            }

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
            } else if ext == "gz" || ext == "tgz" {
                let gz = flate2::read::GzDecoder::new(pr);
                tar::Archive::new(Box::new(gz) as Box<dyn std::io::Read>)
            } else {
                tar::Archive::new(Box::new(pr) as Box<dyn std::io::Read>)
            };

            archive.set_unpack_xattrs(false);
            let mut chunk_buf = vec![0u8; 4 * 1024 * 1024];

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
                    if created_dirs.insert(outpath.clone()) {
                        fs::create_dir_all(&outpath).ok();
                    }
                    continue;
                } else if let Some(par) = outpath.parent()
                    && created_dirs.insert(par.to_path_buf())
                {
                    fs::create_dir_all(par).ok();
                }

                let mut outfile = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&outpath)?;

                let size = entry.header().size().unwrap_or(0);

                #[cfg(target_os = "linux")]
                unsafe {
                    let fd = outfile.as_raw_fd();
                    libc::posix_fadvise(fd, 0, 0, libc::POSIX_FADV_SEQUENTIAL);
                    if size > 0 {
                        libc::fallocate(fd, 0, 0, size as libc::off_t);
                    }
                }

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(AppError::Cancelled);
                    }
                    let n = std::io::Read::read(&mut entry, &mut chunk_buf)?;
                    if n == 0 {
                        break;
                    }
                    std::io::Write::write_all(&mut outfile, &chunk_buf[..n])?;
                }

                #[cfg(target_os = "linux")]
                unsafe {
                    libc::posix_fadvise(outfile.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
                }
            }
        }
        Ok("Extracted successfully".into())
    })
    .await
    .unwrap_or_else(|e| Err(AppError::Task(e.to_string())))
}
