use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
use percent_encoding::percent_decode_str;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub const MARKDOWN_LIMIT: u64 = 20 * 1024 * 1024;
pub const IMAGE_LIMIT: u64 = 16 * 1024 * 1024;
pub const PIXEL_LIMIT: u64 = 24_000_000;

pub fn markdown_path(path: &Path) -> Result<PathBuf, String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !["md", "markdown", "mdown", "mkd", "mkdn", "mdwn"].contains(&extension.as_str()) {
        return Err(
            "Bitte eine Markdown-Datei öffnen (.md, .markdown, .mdown, .mkd, .mkdn, .mdwn).".into(),
        );
    }
    path.canonicalize()
        .map_err(|_| "Die Datei wurde nicht gefunden oder ist nicht lesbar.".into())
}

pub fn read_bounded(dir: &Dir, path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    // Dir::open enforces the directory boundary during the actual OS read, including symlinks.
    let file = dir
        .open_with(
            path,
            OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK),
        )
        .map_err(|_| {
            "Die Datei ist nicht lesbar oder liegt außerhalb des freigegebenen Verzeichnisses."
                .to_string()
        })?;
    let metadata = file
        .metadata()
        .map_err(|_| "Dateiinformationen sind nicht verfügbar.".to_string())?;
    if !metadata.is_file() {
        return Err("Das Ziel ist keine reguläre Datei.".into());
    }
    if metadata.len() > limit {
        return Err(format!(
            "Die Datei überschreitet das Limit von {} MiB.",
            limit / 1024 / 1024
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Die Datei konnte nicht vollständig gelesen werden.".to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Die Datei ist während des Lesens über das Größenlimit gewachsen.".into());
    }
    Ok(bytes)
}

pub fn decode_relative_url(value: &str) -> Result<PathBuf, String> {
    let path = value.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_decode_str(path)
        .decode_utf8()
        .map_err(|_| "Ungültiger URL-Pfad.".to_string())?;
    if decoded.contains('\0') || decoded.contains('\\') || decoded.contains(':') {
        return Err("Dieser Ressourcenpfad ist nicht erlaubt.".into());
    }
    Ok(PathBuf::from(decoded.as_ref()))
}

pub fn image_bytes(dir: &Dir, source: &str) -> Result<(Vec<u8>, &'static str), String> {
    let path = decode_relative_url(source)?;
    if path.is_absolute() {
        return Err("Absolute Bildpfade sind nicht freigegeben.".into());
    }
    let bytes = read_bounded(dir, &path, IMAGE_LIMIT)?;
    validate_image(bytes)
}

pub fn validate_image(bytes: Vec<u8>) -> Result<(Vec<u8>, &'static str), String> {
    let format = image::guess_format(&bytes).map_err(|_| "Unbekanntes Bildformat.".to_string())?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err("Dieses Bildformat wird nicht unterstützt.".into()),
    };
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| "Beschädigtes Bild.".to_string())?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > PIXEL_LIMIT {
        return Err("Das Bild überschreitet das Limit von 24 Megapixeln.".into());
    }
    Ok((bytes, mime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    #[test]
    fn url_paths_decode_once_and_preserve_spaces() {
        assert_eq!(
            decode_relative_url("Bilder/gr%C3%BCn%20%23.png?v=1#x").unwrap(),
            Path::new("Bilder/grün #.png")
        );
        assert_eq!(
            decode_relative_url("a%2520.png").unwrap(),
            Path::new("a%20.png")
        );
        assert!(decode_relative_url("file:/etc/passwd").is_err());
        assert!(decode_relative_url("a%00.png").is_err());
    }
    #[test]
    fn actual_reads_reject_escape_symlinks_and_enforce_size() {
        let root = std::env::temp_dir().join(format!(
            "hashline-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("scope")).unwrap();
        fs::write(root.join("outside"), "private").unwrap();
        fs::write(root.join("scope/inside"), "hello").unwrap();
        std::process::Command::new("mkfifo")
            .arg(root.join("scope/fifo"))
            .status()
            .unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("scope/link")).unwrap();
        let dir = Dir::open_ambient_dir(root.join("scope"), cap_std::ambient_authority()).unwrap();
        assert_eq!(
            read_bounded(&dir, Path::new("inside"), 5).unwrap(),
            b"hello"
        );
        assert!(read_bounded(&dir, Path::new("inside"), 4).is_err());
        assert!(read_bounded(&dir, Path::new("../outside"), 100).is_err());
        assert!(read_bounded(&dir, Path::new("link"), 100).is_err());
        assert!(read_bounded(&dir, Path::new("fifo"), 100).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
