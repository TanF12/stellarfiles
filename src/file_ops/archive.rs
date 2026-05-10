use crate::errors::AppError;
use bzip3::read as bz3read;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const IO_BUF: usize = 8 * 1024 * 1024;
const EXTRACT_READ_BUF: usize = 2 * 1024 * 1024;
const PROGRESS_INTERVAL_MS: u128 = 50;
const PROGRESS_BYTES_THRESHOLD: u64 = 524_288; // 512 KB

#[derive(Clone, Copy, Debug)]
pub enum CompressionLevel {
    Fast,
    Normal,
    Maximum,
}

impl CompressionLevel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Fast" => Self::Fast,
            "Maximum" => Self::Maximum,
            _ => Self::Normal,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArchiveFormat {
    Zip,
    TarBz3,
    TarZst,
    TarGz,
    Tar,
    SevenZ,
    Unsupported,
}

impl ArchiveFormat {
    pub fn from_filename(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.ends_with(".7z") {
            Self::SevenZ
        } else if lower.ends_with(".zip") {
            Self::Zip
        } else if lower.ends_with(".tar.bz3") {
            Self::TarBz3
        } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
            Self::TarZst
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Self::TarGz
        } else if lower.ends_with(".tar") {
            Self::Tar
        } else {
            Self::Unsupported
        }
    }

    pub fn from_str_id(id: &str) -> Self {
        match id.to_lowercase().as_str() {
            "7z" => Self::SevenZ,
            "zip" => Self::Zip,
            "tar.bz3" => Self::TarBz3,
            "tar.zst" => Self::TarZst,
            "tar.gz" => Self::TarGz,
            "tar" => Self::Tar,
            _ => Self::Unsupported,
        }
    }
}

pub struct CompressOptions {
    pub paths: Vec<PathBuf>,
    pub dest_name: String,
    pub format: String,
    pub level: String,
    pub current_dir: PathBuf,
    pub tx: async_channel::Sender<crate::types::ProgressMsg>,
    pub id: usize,
    pub cancel: Arc<AtomicBool>,
}

struct ProgressTracker {
    tx: async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    bytes_acc: u64,
    bytes_since_check: u64,
    last_report: std::time::Instant,
    active_file: String,
}

impl ProgressTracker {
    #[inline]
    fn new(
        tx: async_channel::Sender<crate::types::ProgressMsg>,
        id: usize,
        active_file: String,
    ) -> Self {
        Self {
            tx,
            id,
            bytes_acc: 0,
            bytes_since_check: 0,
            last_report: std::time::Instant::now(),
            active_file,
        }
    }

    #[inline(always)]
    fn update(&mut self, n: u64) {
        self.bytes_acc += n;
        self.bytes_since_check += n;

        if self.bytes_since_check >= PROGRESS_BYTES_THRESHOLD {
            self.bytes_since_check = 0;
            if self.last_report.elapsed().as_millis() > PROGRESS_INTERVAL_MS {
                self.emit();
            }
        }
    }

    #[inline(always)]
    fn emit(&mut self) {
        let msg = crate::types::ProgressMsg::Update {
            id: self.id,
            bytes_chunk: self.bytes_acc,
            active_file: self.active_file.clone(),
        };
        if self.tx.try_send(msg).is_ok() {
            self.bytes_acc = 0;
            self.last_report = std::time::Instant::now();
        }
    }

    #[inline]
    fn set_file(&mut self, file: String) {
        self.active_file = file;
    }
}

impl Drop for ProgressTracker {
    fn drop(&mut self) {
        if self.bytes_acc > 0 {
            self.emit();
        }
    }
}

struct ProgressReader<R: Read> {
    inner: R,
    tracker: ProgressTracker,
    cancel: Arc<AtomicBool>,
}

impl<R: Read> ProgressReader<R> {
    #[inline]
    fn new(
        inner: R,
        tx: async_channel::Sender<crate::types::ProgressMsg>,
        id: usize,
        file_name: String,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            inner,
            tracker: ProgressTracker::new(tx, id, file_name),
            cancel,
        }
    }
}

impl<R: Read> Read for ProgressReader<R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.tracker.update(n as u64);
        }
        Ok(n)
    }
}

#[inline]
fn is_safe_path<P: AsRef<Path>>(path: P) -> bool {
    !path.as_ref().components().any(|c| {
        matches!(
            c,
            std::path::Component::RootDir
                | std::path::Component::ParentDir
                | std::path::Component::Prefix(_)
        )
    })
}

#[inline]
fn ensure_parent(path: &Path, seen: &mut HashSet<PathBuf>) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !seen.contains(parent)
    {
        fs::create_dir_all(parent)?;
        seen.insert(parent.to_path_buf());
    }
    Ok(())
}

fn create_symlink(target: &Path, dst: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, dst)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, dst)
    }
}

#[inline]
fn zip_level(l: CompressionLevel) -> i64 {
    match l {
        CompressionLevel::Fast => 1,
        CompressionLevel::Maximum => 9,
        CompressionLevel::Normal => 5,
    }
}

#[inline]
fn zstd_level(l: CompressionLevel) -> i32 {
    match l {
        CompressionLevel::Fast => 1,
        CompressionLevel::Maximum => 19,
        CompressionLevel::Normal => 3,
    }
}

#[inline]
fn gz_level(l: CompressionLevel) -> flate2::Compression {
    match l {
        CompressionLevel::Fast => flate2::Compression::fast(),
        CompressionLevel::Maximum => flate2::Compression::best(),
        CompressionLevel::Normal => flate2::Compression::default(),
    }
}

#[inline]
fn bz3_block(l: CompressionLevel) -> usize {
    match l {
        CompressionLevel::Fast => 100 * 1024,
        CompressionLevel::Normal => 500 * 1024,
        CompressionLevel::Maximum => 1024 * 1024,
    }
}

#[inline]
fn p7z_args(l: CompressionLevel) -> Vec<&'static str> {
    match l {
        CompressionLevel::Fast => vec!["-mx=1", "-ms=16m"],
        CompressionLevel::Maximum => vec!["-mx=9", "-ms=128m"],
        CompressionLevel::Normal => vec!["-mx=5", "-ms=64m"],
    }
}

pub async fn compress_path(opts: CompressOptions) -> Result<String, AppError> {
    let cleanup_path = opts.current_dir.join(&opts.dest_name);
    let format = ArchiveFormat::from_str_id(&opts.format);

    let res = if format == ArchiveFormat::SevenZ {
        compress_7z_async(opts).await
    } else {
        tokio::task::spawn_blocking(move || compress_blocking(opts, format))
            .await
            .unwrap_or_else(|e| Err(AppError::Task(e.to_string())))
    };

    if res.is_err() && cleanup_path.exists() {
        let _ = fs::remove_file(cleanup_path);
    }

    res
}

pub async fn extract_archive(
    path: PathBuf,
    dest: PathBuf,
    tx: async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<String, AppError> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let format = ArchiveFormat::from_filename(&file_name);

    if format == ArchiveFormat::SevenZ {
        extract_7z_async(path, dest, tx, id, cancel).await
    } else {
        tokio::task::spawn_blocking(move || extract_blocking(path, dest, format, tx, id, cancel))
            .await
            .unwrap_or_else(|e| Err(AppError::Task(e.to_string())))
    }
}

fn compress_blocking(opts: CompressOptions, format: ArchiveFormat) -> Result<String, AppError> {
    let level_enum = CompressionLevel::from_str(&opts.level);
    let total_bytes: u64 = opts
        .paths
        .iter()
        .map(|p| crate::file_ops::read::get_size(p))
        .sum();

    let _ = opts.tx.send_blocking(crate::types::ProgressMsg::Init {
        id: opts.id,
        total_bytes,
    });

    let dest_path = opts.current_dir.join(&opts.dest_name);
    let _ = fs::remove_file(&dest_path);

    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&dest_path)?;

    let buf_w = io::BufWriter::with_capacity(IO_BUF, file);

    match format {
        ArchiveFormat::Zip => compress_zip(
            &opts.paths,
            buf_w,
            level_enum,
            &opts.tx,
            opts.id,
            opts.cancel,
        )?,
        ArchiveFormat::TarBz3 => {
            let enc = bzip3::write::Bz3Encoder::new(buf_w, bz3_block(level_enum))
                .map_err(|e| AppError::Archive(e.to_string()))?;
            let buffered_enc = io::BufWriter::with_capacity(IO_BUF, enc);
            compress_tar(
                &opts.paths,
                Box::new(buffered_enc),
                &opts.tx,
                opts.id,
                opts.cancel,
            )?;
        }
        ArchiveFormat::TarZst => {
            let mut enc = zstd::stream::Encoder::new(buf_w, zstd_level(level_enum))
                .map_err(|e| AppError::Archive(e.to_string()))?;
            let workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let _ = enc.multithread(workers as u32);
            let enc = enc.auto_finish();
            let buffered_enc = io::BufWriter::with_capacity(IO_BUF, enc);
            compress_tar(
                &opts.paths,
                Box::new(buffered_enc),
                &opts.tx,
                opts.id,
                opts.cancel,
            )?;
        }
        ArchiveFormat::TarGz => {
            let enc = flate2::write::GzEncoder::new(buf_w, gz_level(level_enum));
            let buffered_enc = io::BufWriter::with_capacity(IO_BUF, enc);
            compress_tar(
                &opts.paths,
                Box::new(buffered_enc),
                &opts.tx,
                opts.id,
                opts.cancel,
            )?;
        }
        ArchiveFormat::Tar => {
            compress_tar(&opts.paths, Box::new(buf_w), &opts.tx, opts.id, opts.cancel)?
        }
        ArchiveFormat::SevenZ | ArchiveFormat::Unsupported => {
            return Err(AppError::Archive(format!(
                "Unsupported blocking format: {}",
                opts.format
            )));
        }
    }

    Ok(format!(
        "Successfully compressed to {}",
        dest_path.display()
    ))
}

async fn compress_7z_async(opts: CompressOptions) -> Result<String, AppError> {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    let dest = opts.current_dir.join(&opts.dest_name);
    let level_enum = CompressionLevel::from_str(&opts.level);
    let total_bytes: u64 = opts
        .paths
        .iter()
        .map(|p| crate::file_ops::read::get_size(p))
        .sum();

    let _ = opts
        .tx
        .send(crate::types::ProgressMsg::Init {
            id: opts.id,
            total_bytes,
        })
        .await;

    let mut child = Command::new("7z")
        .args(["a", "-y", "-bso1", "-bse1", "-bsp1"])
        .args(p7z_args(level_enum))
        .arg(&dest)
        .arg("--") // prevents command injection from filenames
        .args(&opts.paths)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| AppError::Archive(format!("7z failed to start: {e}")))?;

    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 1024];
    let mut text_buf = Vec::with_capacity(64);
    let mut last_percent = 0u64;
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        tokio::select! {
                _ = interval.tick() => {
                    if opts.cancel.load(Ordering::Relaxed) {
                        let _ = child.kill().await;
                        return Err(AppError::Cancelled);
                    }
                }
                res = stdout.read(&mut buf) => {
        match res {
            Ok(0) => break,

            Ok(n) => {
                for &byte in &buf[..n] {
                    if matches!(byte, b'\r' | b'\n' | b'\x08')
                        && let Ok(s) = std::str::from_utf8(&text_buf) {
                            if let Some(p_idx) = s.rfind('%') {
                                let start = s[..p_idx]
                                    .rfind(' ')
                                    .map(|i| i + 1)
                                    .unwrap_or(0);

                                if let Ok(p) = s[start..p_idx].parse::<u64>()
                                    && p > last_percent
                                    && p <= 100
                                {
                                    let chunk = (total_bytes * (p - last_percent)) / 100;

                                    let _ = opts.tx.try_send(
                                        crate::types::ProgressMsg::Update {
                                            id: opts.id,
                                            bytes_chunk: chunk,
                                            active_file: "Compressing with 7z...".into(),
                                        },
                                    );

                                    last_percent = p;
                                }
                        }

                        text_buf.clear();
                    } else if text_buf.len() < 64 {
                        text_buf.push(byte);
                    }
                }
            }

                        Err(e) => return Err(AppError::Archive(format!("7z read error: {e}"))),
                    }
                }
            }
    }

    let s = child
        .wait()
        .await
        .map_err(|e| AppError::Archive(e.to_string()))?;
    if s.success() {
        Ok(format!("Successfully compressed to {}", dest.display()))
    } else {
        Err(AppError::Archive(format!(
            "7z compression failed (Exit code: {s})"
        )))
    }
}

fn compress_zip(
    paths: &[PathBuf],
    buf_w: io::BufWriter<fs::File>,
    level: CompressionLevel,
    tx: &async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<(), AppError> {
    let mut zip = zip::ZipWriter::new(buf_w);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(zip_level(level)))
        .large_file(true);

    let mut buf = vec![0u8; IO_BUF];

    for path in paths {
        if path.is_dir() {
            let base_parent = path.parent().unwrap_or(path);
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Cancelled);
                }
                let p = entry.path();
                let rel = p.strip_prefix(base_parent).unwrap_or(p);
                if rel.as_os_str().is_empty() || !is_safe_path(rel) {
                    continue;
                }
                let arc_name = rel.to_string_lossy();
                if p.is_dir() {
                    zip.add_directory(arc_name.as_ref(), options)
                        .map_err(|e| AppError::Archive(e.to_string()))?;
                } else {
                    zip_add_file(
                        &mut zip,
                        p,
                        arc_name.as_ref(),
                        options,
                        &mut buf,
                        tx,
                        id,
                        cancel.clone(),
                    )?;
                }
            }
        } else {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if is_safe_path(Path::new(name.as_ref())) {
                zip_add_file(
                    &mut zip,
                    path,
                    &name,
                    options,
                    &mut buf,
                    tx,
                    id,
                    cancel.clone(),
                )?;
            }
        }
    }

    let mut inner = zip.finish().map_err(|e| AppError::Archive(e.to_string()))?;
    inner.flush()?;
    Ok(())
}

fn compress_tar(
    paths: &[PathBuf],
    enc: Box<dyn Write>,
    tx: &async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<(), AppError> {
    let mut tar = tar::Builder::new(enc);
    tar.follow_symlinks(false);

    for path in paths {
        if path.is_dir() {
            let base_parent = path.parent().unwrap_or(path);
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Cancelled);
                }
                let p = entry.path();
                let rel = p.strip_prefix(base_parent).unwrap_or(p);
                if rel.as_os_str().is_empty() || !is_safe_path(rel) {
                    continue;
                }
                if p.is_dir() {
                    tar.append_dir(rel, p)
                        .map_err(|e| AppError::Archive(e.to_string()))?;
                } else {
                    tar_add_file(&mut tar, p, rel, tx, id, cancel.clone())?;
                }
            }
        } else {
            let dest_name = path.file_name().unwrap_or_default();
            let dest_path = std::path::Path::new(&dest_name);
            if is_safe_path(dest_path) {
                tar_add_file(&mut tar, path, dest_path, tx, id, cancel.clone())?;
            }
        }
    }

    let mut enc_out = tar
        .into_inner()
        .map_err(|e| AppError::Archive(e.to_string()))?;
    enc_out.flush()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn zip_add_file(
    zip: &mut zip::ZipWriter<io::BufWriter<fs::File>>,
    path: &Path,
    arc_name: &str,
    mut options: zip::write::SimpleFileOptions,
    buf: &mut [u8],
    tx: &async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<(), AppError> {
    let meta = fs::symlink_metadata(path)?;

    if meta.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        #[cfg(unix)]
        {
            options = options.unix_permissions(0o120000 | (meta.mode() & 0o777));
        }
        zip.start_file(arc_name, options)
            .map_err(|e| AppError::Archive(e.to_string()))?;
        zip.write_all(target.to_string_lossy().as_bytes())?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        options = options.unix_permissions(meta.mode());
    }

    zip.start_file(arc_name, options)
        .map_err(|e| AppError::Archive(e.to_string()))?;
    let mut f = fs::File::open(path)?;

    #[cfg(target_os = "linux")]
    {
        let _ = nix::fcntl::posix_fadvise(
            &f,
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_SEQUENTIAL,
        );
    }

    let mut tracker = ProgressTracker::new(tx.clone(), id, arc_name.to_string());

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let n = f.read(buf)?;
        if n == 0 {
            break;
        }
        zip.write_all(&buf[..n])?;
        tracker.update(n as u64);
    }
    Ok(())
}

fn tar_add_file(
    tar: &mut tar::Builder<Box<dyn Write>>,
    src: &Path,
    arc_path: &Path,
    tx: &async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<(), AppError> {
    let meta = fs::symlink_metadata(src)?;
    let mut header = tar::Header::new_gnu();

    #[cfg(unix)]
    {
        header.set_mode(meta.mode());
        header.set_mtime(meta.mtime() as u64);
    }

    if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_cksum();
        tar.append_link(&mut header, arc_path, target)
            .map_err(|e| AppError::Archive(e.to_string()))?;
        return Ok(());
    }

    header.set_size(meta.len());
    header.set_cksum();

    let f = fs::File::open(src)?;

    #[cfg(target_os = "linux")]
    {
        let _ = nix::fcntl::posix_fadvise(
            &f,
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_SEQUENTIAL,
        );
    }

    let f = io::BufReader::with_capacity(128 * 1024, f);
    let fname = arc_path.to_string_lossy().to_string();
    let mut pr = ProgressReader::new(f, tx.clone(), id, fname, cancel.clone());

    if let Err(e) = tar.append_data(&mut header, arc_path, &mut pr) {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        return Err(AppError::Archive(e.to_string()));
    }
    Ok(())
}

fn extract_blocking(
    path: PathBuf,
    dest: PathBuf,
    format: ArchiveFormat,
    tx: async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
) -> Result<String, AppError> {
    let safe_dest = dest.canonicalize().unwrap_or_else(|_| dest.clone());
    let mut created_files = Vec::new();
    let mut created_dirs = HashSet::new();

    let res = match format {
        ArchiveFormat::Zip => extract_zip(
            &path,
            &safe_dest,
            &tx,
            id,
            cancel.clone(),
            &mut created_files,
            &mut created_dirs,
        ),
        ArchiveFormat::TarBz3
        | ArchiveFormat::TarZst
        | ArchiveFormat::TarGz
        | ArchiveFormat::Tar => extract_tar_generic(
            &path,
            &safe_dest,
            format,
            &tx,
            id,
            cancel.clone(),
            &mut created_files,
            &mut created_dirs,
        ),
        ArchiveFormat::SevenZ | ArchiveFormat::Unsupported => Err(AppError::Archive(format!(
            "unsupported archive extraction format for file: {}",
            path.display()
        ))),
    };

    if res.is_err() {
        for f in created_files.iter().rev() {
            let _ = fs::remove_file(f);
        }
        let mut dirs: Vec<_> = created_dirs.into_iter().collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.as_os_str().len()));
        for d in dirs {
            let _ = fs::remove_dir(d);
        }
    }

    res
}

async fn extract_7z_async(
    path: std::path::PathBuf,
    dest: std::path::PathBuf,
    tx: async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, AppError> {
    use std::sync::atomic::Ordering;
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    let file_size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let _ = tx
        .send(crate::types::ProgressMsg::Init {
            id,
            total_bytes: file_size,
        })
        .await;

    let mut out_arg = std::ffi::OsString::from("-o");
    out_arg.push(&dest);

    let mut child = Command::new("7z")
        .args(["x", "-y", "-bso1", "-bse1", "-bsp1"])
        .arg(&out_arg)
        .arg("--") // Security delimiter
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| AppError::Archive(format!("7z failed to start: {e}")))?;

    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 1024];
    let mut text_buf = Vec::with_capacity(64);
    let mut last_percent = 0u64;
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill().await;
                    return Err(AppError::Cancelled);
                }
            }
            res = stdout.read(&mut buf) => {
                match res {
                    Ok(0) => break,
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            if byte == b'\r' || byte == b'\n' || byte == b'\x08' {
                                if let Ok(s) = std::str::from_utf8(&text_buf)
                                    && let Some(p_idx) = s.rfind('%') {
                                        let start = s[..p_idx].rfind(' ').map(|i| i + 1).unwrap_or(0);
                                        if let Ok(p) = s[start..p_idx].parse::<u64>()
                                            && p > last_percent && p <= 100 {
                                                let delta = p - last_percent;
                                                let bytes_chunk = (file_size * delta) / 100;
                                                let _ = tx.try_send(crate::types::ProgressMsg::Update {
                                                    id,
                                                    bytes_chunk,
                                                    active_file: "Extracting with 7z...".into(),
                                                });
                                                last_percent = p;
                                            }
                                }
                                text_buf.clear();
                            } else if text_buf.len() < 64 {
                                text_buf.push(byte);
                            }
                        }
                    }
                    Err(e) => return Err(AppError::Archive(format!("7z read error: {e}"))),
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::Archive(e.to_string()))?;
    if status.success() {
        Ok("Extracted successfully".into())
    } else {
        Err(AppError::Archive(format!(
            "7z extraction failed (Exit code: {status})"
        )))
    }
}

fn extract_zip(
    path: &Path,
    dest: &Path,
    tx: &async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
    created_files: &mut Vec<PathBuf>,
    created_dirs: &mut HashSet<PathBuf>,
) -> Result<String, AppError> {
    let file = fs::File::open(path)?;

    #[cfg(target_os = "linux")]
    {
        let _ = nix::fcntl::posix_fadvise(
            &file,
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_RANDOM,
        );
    }

    let reader = io::BufReader::with_capacity(128 * 1024, file);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| AppError::Archive(e.to_string()))?;

    let mut file_list: Vec<(usize, u64)> = Vec::with_capacity(archive.len());
    let mut total_bytes: u64 = 0;

    for i in 0..archive.len() {
        if let Ok(item) = archive.by_index(i) {
            let sz = item.size();
            total_bytes += sz;
            if !item.is_dir() {
                file_list.push((i, sz));
            }
        }
    }
    let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });

    let mut buf = vec![0u8; IO_BUF];
    let mut tracker = ProgressTracker::new(tx.clone(), id, "Extracting...".to_string());

    for (i, size) in file_list {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let mut item = archive
            .by_index(i)
            .map_err(|e| AppError::Archive(e.to_string()))?;
        let rel = match item.enclosed_name() {
            Some(n) => n,
            None => continue,
        };

        if !is_safe_path(&rel) {
            continue;
        }

        let outpath = dest.join(rel);

        if outpath.exists() || outpath.is_symlink() && !outpath.is_dir() {
            let _ = fs::remove_file(&outpath);
        }

        ensure_parent(&outpath, created_dirs)?;

        let mut is_symlink = false;
        if let Some(mode) = item.unix_mode()
            && mode & 0o170000 == 0o120000
        {
            is_symlink = true;
        }

        if is_symlink {
            let mut target = String::new();
            let _ = std::io::Read::take(&mut item, 4096).read_to_string(&mut target);
            let target_path = PathBuf::from(target.trim_end_matches('\0'));

            if !is_safe_path(&target_path) {
                continue;
            }

            create_symlink(&target_path, &outpath).map_err(AppError::Io)?;
            created_files.push(outpath.clone());
            continue;
        }

        let mut outfile = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&outpath)?;

        created_files.push(outpath.clone());

        #[cfg(target_os = "linux")]
        {
            let _ = nix::fcntl::posix_fadvise(
                &outfile,
                0,
                0,
                nix::fcntl::PosixFadviseAdvice::POSIX_FADV_SEQUENTIAL,
            );
            if size > 0 {
                let _ = nix::fcntl::fallocate(
                    &outfile,
                    nix::fcntl::FallocateFlags::empty(),
                    0,
                    size as libc::off_t,
                );
            }
        }

        tracker.set_file(item.name().to_string());

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let n = match item.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => return Err(AppError::Io(e)),
            };
            if let Err(e) = outfile.write_all(&buf[..n]) {
                return Err(AppError::Io(e));
            }
            tracker.update(n as u64);
        }

        #[cfg(target_os = "linux")]
        {
            let _ = outfile.sync_data();
            let _ = nix::fcntl::posix_fadvise(
                &outfile,
                0,
                0,
                nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
            );
        }
    }

    Ok("Extracted successfully".into())
}

#[allow(clippy::too_many_arguments)]
fn extract_tar_generic(
    path: &Path,
    dest: &Path,
    comp: ArchiveFormat,
    tx: &async_channel::Sender<crate::types::ProgressMsg>,
    id: usize,
    cancel: Arc<AtomicBool>,
    created_files: &mut Vec<PathBuf>,
    created_dirs: &mut HashSet<PathBuf>,
) -> Result<String, AppError> {
    let file = fs::File::open(path)?;
    let total_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = tx.send_blocking(crate::types::ProgressMsg::Init { id, total_bytes });

    #[cfg(target_os = "linux")]
    {
        let _ = nix::fcntl::posix_fadvise(
            &file,
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_SEQUENTIAL,
        );
    }

    let f = io::BufReader::with_capacity(EXTRACT_READ_BUF, file);
    let fname = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let pr = ProgressReader::new(f, tx.clone(), id, fname, cancel.clone());

    let dec: Box<dyn Read> = match comp {
        ArchiveFormat::TarZst => {
            let dec =
                zstd::stream::Decoder::new(pr).map_err(|e| AppError::Archive(e.to_string()))?;
            Box::new(io::BufReader::with_capacity(IO_BUF, dec))
        }
        ArchiveFormat::TarBz3 => {
            let dec = bz3read::Bz3Decoder::new(pr).map_err(|e| AppError::Archive(e.to_string()))?;
            Box::new(io::BufReader::with_capacity(IO_BUF, dec))
        }
        ArchiveFormat::TarGz => {
            let dec = flate2::read::GzDecoder::new(pr);
            Box::new(io::BufReader::with_capacity(IO_BUF, dec))
        }
        ArchiveFormat::Tar => Box::new(pr),
        _ => return Err(AppError::Archive("Invalid tar format requested".into())),
    };

    match unpack_tar(
        tar::Archive::new(dec),
        dest,
        cancel.clone(),
        created_files,
        created_dirs,
    ) {
        Err(e) => {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            Err(e)
        }
        Ok(s) => Ok(s),
    }
}

fn unpack_tar<R: Read>(
    mut archive: tar::Archive<R>,
    dest: &Path,
    cancel: Arc<AtomicBool>,
    created_files: &mut Vec<PathBuf>,
    created_dirs: &mut HashSet<PathBuf>,
) -> Result<String, AppError> {
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(true);

    let mut buf = vec![0u8; IO_BUF];
    let entries = archive
        .entries()
        .map_err(|e| AppError::Archive(e.to_string()))?;

    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let mut entry = entry.map_err(|e| AppError::Archive(e.to_string()))?;
        let p = entry
            .path()
            .map_err(|e| AppError::Archive(e.to_string()))?
            .into_owned();

        if !is_safe_path(&p) {
            continue;
        }

        let outpath = dest.join(&p);
        let entry_type = entry.header().entry_type();

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            if let Ok(Some(target)) = entry.link_name() {
                if !is_safe_path(&target) {
                    continue;
                }

                if outpath.exists() || outpath.is_symlink() {
                    let _ = fs::remove_file(&outpath);
                }

                ensure_parent(&outpath, created_dirs)?;

                if entry_type.is_symlink() {
                    create_symlink(&target, &outpath).map_err(AppError::Io)?;
                } else {
                    fs::hard_link(dest.join(target), &outpath).map_err(AppError::Io)?;
                }
                created_files.push(outpath.clone());
            }
            continue;
        }

        if entry_type.is_dir() {
            if !created_dirs.contains(&outpath) {
                fs::create_dir_all(&outpath)?;
                created_dirs.insert(outpath.clone());
            }
            continue;
        }

        if outpath.exists() || outpath.is_symlink() {
            let _ = fs::remove_file(&outpath);
        }

        ensure_parent(&outpath, created_dirs)?;

        let mut outfile = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&outpath)?;

        created_files.push(outpath.clone());
        let size = entry.header().size().unwrap_or(0);

        #[cfg(target_os = "linux")]
        {
            let _ = nix::fcntl::posix_fadvise(
                &outfile,
                0,
                0,
                nix::fcntl::PosixFadviseAdvice::POSIX_FADV_SEQUENTIAL,
            );
            if size > 0 {
                let _ = nix::fcntl::fallocate(
                    &outfile,
                    nix::fcntl::FallocateFlags::empty(),
                    0,
                    size as libc::off_t,
                );
            }
        }

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let n = match entry.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => return Err(AppError::Io(e)),
            };
            if let Err(e) = outfile.write_all(&buf[..n]) {
                return Err(AppError::Io(e));
            }
        }

        #[cfg(target_os = "linux")]
        {
            let _ = outfile.sync_data();
            let _ = nix::fcntl::posix_fadvise(
                &outfile,
                0,
                0,
                nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
            );
        }
    }

    Ok("Extracted successfully".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_path() {
        assert!(is_safe_path(Path::new("valid/folder/file.txt")));
        assert!(is_safe_path(Path::new("file.txt")));

        assert!(!is_safe_path(Path::new("../file.txt")));
        assert!(!is_safe_path(Path::new("valid/../../file.txt")));
        assert!(!is_safe_path(Path::new("/etc/passwd")));

        #[cfg(windows)]
        assert!(!is_safe_path(Path::new("C:\\Windows\\System32")));
    }

    #[test]
    fn test_archive_format_parsing() {
        assert_eq!(
            ArchiveFormat::from_filename("archive.7z"),
            ArchiveFormat::SevenZ
        );
        assert_eq!(
            ArchiveFormat::from_filename("archive.ZIP"),
            ArchiveFormat::Zip
        );
        assert_eq!(
            ArchiveFormat::from_filename("file.TAR.BZ3"),
            ArchiveFormat::TarBz3
        );
        assert_eq!(
            ArchiveFormat::from_filename("backup.tzst"),
            ArchiveFormat::TarZst
        );
        assert_eq!(
            ArchiveFormat::from_filename("backup.tar.gz"),
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_filename("unknown.rar"),
            ArchiveFormat::Unsupported
        );
    }

    #[tokio::test]
    async fn test_round_trip_zip() {
        let temp_dir = std::env::temp_dir().join(format!("zip_test_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        let file_to_compress = temp_dir.join("test_file.txt");
        fs::write(&file_to_compress, b"Hello World Zip Data!").unwrap();

        let (tx, _rx) = async_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));

        let compress_opts = CompressOptions {
            paths: vec![file_to_compress.clone()],
            dest_name: "test_archive.zip".to_string(),
            format: "zip".to_string(),
            level: "Fast".to_string(),
            current_dir: temp_dir.clone(),
            tx: tx.clone(),
            id: 1,
            cancel: cancel.clone(),
        };

        let res = compress_path(compress_opts).await;
        assert!(res.is_ok(), "Compression failed: {:?}", res);

        let zip_file = temp_dir.join("test_archive.zip");
        assert!(zip_file.exists());

        let extract_dest = temp_dir.join("extracted_zip");
        fs::create_dir_all(&extract_dest).unwrap();

        let res_extract =
            extract_archive(zip_file.clone(), extract_dest.clone(), tx, 1, cancel).await;
        assert!(res_extract.is_ok(), "Extraction failed: {:?}", res_extract);

        let extracted_file = extract_dest.join("test_file.txt");
        assert!(extracted_file.exists());
        let content = fs::read_to_string(extracted_file).unwrap();
        assert_eq!(content, "Hello World Zip Data!");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[tokio::test]
    async fn test_round_trip_tar() {
        let temp_dir = std::env::temp_dir().join(format!("tar_test_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        let file_to_compress = temp_dir.join("test_tar_file.txt");
        fs::write(&file_to_compress, b"Hello World Tar Data!").unwrap();

        let (tx, _rx) = async_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));

        let compress_opts = CompressOptions {
            paths: vec![file_to_compress.clone()],
            dest_name: "test_archive.tar".to_string(),
            format: "tar".to_string(),
            level: "Normal".to_string(),
            current_dir: temp_dir.clone(),
            tx: tx.clone(),
            id: 2,
            cancel: cancel.clone(),
        };

        let res = compress_path(compress_opts).await;
        assert!(res.is_ok(), "Compression failed: {:?}", res);

        let tar_file = temp_dir.join("test_archive.tar");
        assert!(tar_file.exists());

        let extract_dest = temp_dir.join("extracted_tar");
        fs::create_dir_all(&extract_dest).unwrap();

        let res_extract =
            extract_archive(tar_file.clone(), extract_dest.clone(), tx, 2, cancel).await;
        assert!(res_extract.is_ok(), "Extraction failed: {:?}", res_extract);

        let extracted_file = extract_dest.join("test_tar_file.txt");
        assert!(extracted_file.exists());
        let content = fs::read_to_string(extracted_file).unwrap();
        assert_eq!(content, "Hello World Tar Data!");

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
