use crate::math::format_bytes;
use crate::types::{DiskInfo, FileEntry, PropertiesDetails};
use chrono::{DateTime, Local};
use md5::{Digest, Md5};
use mime_guess::from_path;
use rayon::prelude::*;
use rust_i18n::t;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use sysinfo::Disks;

pub fn resolve_mime_str(path: &Path, is_dir: bool) -> &'static str {
    if is_dir {
        return "Folder";
    }
    let mime = from_path(path).first_or_octet_stream();
    match mime.type_().as_str() {
        "image" => "Image",
        "video" => "Video",
        "audio" => "Audio",
        "text" => "Document",
        "application" => match mime.subtype().as_str() {
            "pdf" => "PDF Document",
            "zip" | "x-tar" | "gzip" => "Archive",
            _ => "Application",
        },
        _ => "File",
    }
}

pub fn resolve_icon(path: &Path, is_dir: bool) -> &'static str {
    if is_dir {
        return "folder-symbolic";
    }
    let mime = from_path(path).first_or_octet_stream();
    match mime.type_().as_str() {
        "image" => "image-x-generic-symbolic",
        "video" => "video-x-generic-symbolic",
        "audio" => "audio-x-generic-symbolic",
        "text" => match mime.subtype().as_str() {
            "html" | "xml" | "json" => "text-html-symbolic",
            "x-python" | "x-rust" | "javascript" => "text-x-script-symbolic",
            _ => "text-x-generic-symbolic",
        },
        "application" => match mime.subtype().as_str() {
            "pdf" => "application-pdf-symbolic",
            "zip" | "x-tar" | "gzip" | "x-7z-compressed" => "package-x-generic-symbolic",
            _ => "application-x-executable-symbolic",
        },
        _ => "text-x-generic-symbolic",
    }
}

fn truncate_text(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count > max {
        let available = max.saturating_sub(3);
        if available == 0 {
            return "...".to_string();
        }
        let keep_front = (available / 2) + (available % 2);
        let keep_back = available / 2;
        let first: String = s.chars().take(keep_front).collect();
        let last: String = s.chars().skip(count - keep_back).collect();
        format!("{}...{}", first, last)
    } else {
        s.to_string()
    }
}

pub fn create_entry(path: PathBuf, metadata: std::fs::Metadata) -> FileEntry {
    let is_dir = metadata.is_dir();
    let size_bytes = metadata.len();
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let name_str = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    FileEntry {
        path: path.clone(),
        grid_name: truncate_text(&name_str, 10).into(),
        name: name_str.into(),
        is_dir,
        size_bytes,
        modified,
        size_str: if is_dir {
            "--".into()
        } else {
            format_bytes(size_bytes).into()
        },
        modified_str: DateTime::<Local>::from(modified)
            .format("%b %d, %Y")
            .to_string()
            .into(),
        file_type_str: t!(resolve_mime_str(&path, is_dir)).to_string().into(),
        icon_name: resolve_icon(&path, is_dir),
    }
}

pub async fn load_directory(dir: PathBuf) -> Result<Vec<FileEntry>, String> {
    tokio::task::spawn_blocking(move || {
        let entries: Vec<_> = fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        let mut files: Vec<FileEntry> = entries
            .into_par_iter()
            .filter_map(|entry| {
                let path = entry.path();
                let metadata = fs::metadata(&path)
                    .or_else(|_| fs::symlink_metadata(&path))
                    .ok()?;
                Some(create_entry(path, metadata))
            })
            .collect();
        files.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(files)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

pub fn load_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let mut seen = HashSet::new();
    for d in Disks::new_with_refreshed_list().list() {
        let mp = d.mount_point().to_path_buf();
        let fs = d.file_system().to_string_lossy().to_lowercase();
        if fs.contains("tmpfs")
            || fs.contains("squashfs")
            || fs.contains("overlay")
            || fs.contains("dev")
            || fs.contains("loop")
        {
            continue;
        }
        if seen.insert(mp.clone()) {
            let mut name = d.name().to_string_lossy().into_owned();
            if name.trim().is_empty() {
                name = mp
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            }
            if name.trim().is_empty() {
                name = "Disk".into();
            }
            disks.push(DiskInfo {
                name: name.into(),
                mount_point: mp,
            });
        }
    }
    disks
}

pub async fn fetch_properties(path: PathBuf, icon: &'static str) -> PropertiesDetails {
    tokio::task::spawn_blocking(move || {
        let fallback = || PropertiesDetails {
            path: path.to_string_lossy().into_owned(),
            file_type: "Broken Link".into(),
            size_str: "0 B".into(),
            created: "N/A".into(),
            modified: "N/A".into(),
            accessed: "N/A".into(),
            owner: 0,
            group: 0,
            mode_octal: 0,
            items_count: None,
            icon: "dialog-warning",
        };

        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => return fallback(),
        };

        let is_dir = meta.is_dir();
        let mut items_count = None;
        let mut size = meta.len();
        if is_dir {
            let (c, s) = walkdir::WalkDir::new(&path)
                .min_depth(1)
                .into_iter()
                .flatten()
                .fold((0, 0), |(count, sz), e| {
                    (count + 1, sz + e.metadata().map(|m| m.len()).unwrap_or(0))
                });
            items_count = Some(c);
            size += s;
        }

        let dt_fmt = |t: Option<SystemTime>| {
            t.map(|time| {
                DateTime::<Local>::from(time)
                    .format("%b %d, %Y %I:%M %p")
                    .to_string()
            })
            .unwrap_or_else(|| "Unknown".to_string())
        };

        PropertiesDetails {
            path: path.to_string_lossy().into_owned(),
            file_type: t!(if is_dir { "Folder" } else { "File" }).to_string(),
            size_str: format_bytes(size),
            created: dt_fmt(meta.created().ok()),
            modified: dt_fmt(meta.modified().ok()),
            accessed: dt_fmt(meta.accessed().ok()),
            owner: meta.uid(),
            group: meta.gid(),
            mode_octal: meta.mode() & 0o777,
            items_count,
            icon,
        }
    })
    .await
    .unwrap_or_else(|_| PropertiesDetails {
        path: "Error".into(),
        file_type: "Error".into(),
        size_str: "0 B".into(),
        created: "N/A".into(),
        modified: "N/A".into(),
        accessed: "N/A".into(),
        owner: 0,
        group: 0,
        mode_octal: 0,
        items_count: None,
        icon: "dialog-error",
    })
}

pub async fn generate_thumbnails(paths: Vec<PathBuf>) -> Vec<(PathBuf, u32, u32, Vec<u8>)> {
    tokio::task::spawn_blocking(move || {
        let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let normal_dir = cache_dir.join("thumbnails/normal");
        let _ = fs::create_dir_all(&normal_dir);

        let bind = pdfium_render::prelude::Pdfium::bind_to_system_library().ok();
        let pdfium_opt = bind.map(pdfium_render::prelude::Pdfium::new);
        ffmpeg_next::init().ok();

        paths
            .into_par_iter()
            .filter_map(|p| {
                let uri = format!("file://{}", p.to_string_lossy());
                let mut hasher = Md5::new();
                hasher.update(uri.as_bytes());
                let hash_bytes = hasher.finalize();
                let hash = hash_bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>();
                let file_name = format!("{}.png", hash);
                let cached_path = normal_dir.join(&file_name);

                if cached_path.exists()
                    && let Ok(file) = fs::File::open(&cached_path)
                    && let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) }
                    && let Ok(img) = image::load_from_memory(&mmap)
                {
                    let thumb = img.thumbnail(256, 256).into_rgba8();
                    let (w, h) = thumb.dimensions();
                    return Some((p, w, h, thumb.into_raw()));
                }

                let mime = mime_guess::from_path(&p).first_or_octet_stream();
                let mime_type = mime.type_().as_str();
                let mime_subtype = mime.subtype().as_str();

                if mime_type == "image" {
                    if let Ok(file) = fs::File::open(&p)
                        && let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) }
                        && let Ok(img) = image::load_from_memory(&mmap)
                    {
                        let thumb = img.thumbnail(256, 256);
                        let _ = thumb.save(&cached_path);
                        let rgba = thumb.into_rgba8();
                        let (w, h) = rgba.dimensions();
                        return Some((p, w, h, rgba.into_raw()));
                    }
                } else if mime_subtype == "pdf" {
                    if let Some(pdfium) = &pdfium_opt
                        && let Ok(doc) = pdfium.load_pdf_from_file(&p, None)
                        && let Ok(page) = doc.pages().get(0)
                    {
                        let config = pdfium_render::prelude::PdfRenderConfig::new()
                            .set_target_width(256)
                            .set_maximum_height(256);
                        if let Ok(bitmap) = page.render_with_config(&config)
                            && let Ok(img) = bitmap.as_image()
                        {
                            let _ = img.save(&cached_path);
                            let rgba = img.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            return Some((p, w, h, rgba.into_raw()));
                        }
                    }
                } else if mime_type == "video"
                    && let Ok(mut ictx) = ffmpeg_next::format::input(&p)
                {
                    let best_video_info = ictx
                        .streams()
                        .best(ffmpeg_next::media::Type::Video)
                        .map(|s| (s.index(), s.parameters()));

                    if let Some((stream_index, params)) = best_video_info
                        && let Ok(context) =
                            ffmpeg_next::codec::context::Context::from_parameters(params)
                        && let Ok(mut decoder) = context.decoder().video()
                        && let Ok(mut scaler) = ffmpeg_next::software::scaling::Context::get(
                            decoder.format(),
                            decoder.width(),
                            decoder.height(),
                            ffmpeg_next::format::Pixel::RGB24,
                            256,
                            256,
                            ffmpeg_next::software::scaling::flag::Flags::BILINEAR,
                        )
                    {
                        let mut frame = ffmpeg_next::frame::Video::empty();
                        let mut rgb_frame = ffmpeg_next::frame::Video::empty();
                        let mut got_frame = false;

                        for (s, packet) in ictx.packets() {
                            if s.index() == stream_index {
                                let _ = decoder.send_packet(&packet);
                                if decoder.receive_frame(&mut frame).is_ok() {
                                    got_frame = true;
                                    break;
                                }
                            }
                        }

                        if got_frame
                            && scaler.run(&frame, &mut rgb_frame).is_ok()
                            && let Some(img) = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(
                                256,
                                256,
                                rgb_frame.data(0).to_vec(),
                            )
                        {
                            let dynamic = image::DynamicImage::ImageRgb8(img);
                            let _ = dynamic.save(&cached_path);
                            let rgba = dynamic.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            return Some((p, w, h, rgba.into_raw()));
                        }
                    }
                }
                None
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

pub fn get_size(path: &Path) -> u64 {
    if path.is_dir() {
        walkdir::WalkDir::new(path)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum()
    } else {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_truncate_text() {
        // Length 29, Max 15.
        // Available chars: 15 - 3 = 12 (Front 6, Back 6).
        // First 6: "silver", Last 6: "2.flac"
        assert_eq!(
            truncate_text("silver_machine_live_1972.flac", 15),
            "silver...2.flac"
        );

        // Length 17, Max 10.
        // Available chars: 7 (Front 4, Back 3).
        // First 4: "cala", Last 3: "old"
        assert_eq!(truncate_text("calabiyaumanifold", 10), "cala...old");

        // No truncation needed
        assert_eq!(truncate_text("gong.ogg", 15), "gong.ogg");

        // Exactly the max length
        assert_eq!(truncate_text("spacemen3_drugs", 15), "spacemen3_drugs");

        // Extremely small max length (available chars hits 0)
        assert_eq!(truncate_text("euler", 2), "...");
        assert_eq!(truncate_text("euler", 3), "...");
    }

    #[test]
    fn test_resolve_mime_str() {
        assert_eq!(
            resolve_mime_str(Path::new("tensor_category"), true),
            "Folder"
        );
        assert_eq!(
            resolve_mime_str(Path::new("ozric_tentacles.jpg"), false),
            "Image"
        );
        assert_eq!(
            resolve_mime_str(Path::new("space_ritual.mp4"), false),
            "Video"
        );
        assert_eq!(
            resolve_mime_str(Path::new("poincare_conjecture.txt"), false),
            "Document"
        );
        assert_eq!(
            resolve_mime_str(Path::new("doremi_fasol_latido.zip"), false),
            "Archive"
        );

        // unknown files default to application/octet-stream, which evaluates to Application
        assert_eq!(
            resolve_mime_str(Path::new("riemann.zeta"), false),
            "Application"
        );
    }

    #[test]
    fn test_resolve_icon() {
        assert_eq!(
            resolve_icon(Path::new("homotopy_group"), true),
            "folder-symbolic"
        );
        assert_eq!(
            resolve_icon(Path::new("levitation.png"), false),
            "image-x-generic-symbolic"
        );
        assert_eq!(
            resolve_icon(Path::new("incompleteness.js"), false),
            "text-x-script-symbolic"
        );
        assert_eq!(
            resolve_icon(Path::new("masters_of_the_universe.tar.gz"), false),
            "package-x-generic-symbolic"
        );
    }
}
