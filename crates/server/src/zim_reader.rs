use std::path::PathBuf;
use types::Config;
use zim::{DirectoryEntry, MimeType, Namespace, Target, Zim};

#[derive(Debug)]
pub enum ZimReaderError {
    InvalidFilename,
    ArchiveNotFound,
    EntryNotFound,
    MainPageUnavailable,
    ArchiveUnsupported,
    ReadFailure(String),
}

impl std::fmt::Display for ZimReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZimReaderError::InvalidFilename => write!(f, "invalid archive filename"),
            ZimReaderError::ArchiveNotFound => write!(f, "archive not found"),
            ZimReaderError::EntryNotFound => write!(f, "content entry not found"),
            ZimReaderError::MainPageUnavailable => write!(f, "archive main page is unavailable"),
            ZimReaderError::ArchiveUnsupported => write!(f, "archive format is unsupported by current reader"),
            ZimReaderError::ReadFailure(message) => write!(f, "archive read failure: {message}"),
        }
    }
}

impl std::error::Error for ZimReaderError {}

type Result<T> = std::result::Result<T, ZimReaderError>;

pub struct ZimArchiveMeta {
    pub filename: String,
    pub article_count: u32,
    pub cluster_count: u32,
    pub version_major: u16,
    pub version_minor: u16,
    pub main_page_index: Option<u32>,
}

pub struct ZimResolvedContent {
    pub content_type: String,
    pub content: Vec<u8>,
}

pub fn read_archive_meta(config: &Config, filename: &str) -> Result<ZimArchiveMeta> {
    let zim = open_archive(config, filename)?;

    Ok(ZimArchiveMeta {
        filename: filename.to_string(),
        article_count: zim.header.article_count,
        cluster_count: zim.header.cluster_count,
        version_major: zim.header.version_major,
        version_minor: zim.header.version_minor,
        main_page_index: zim.header.main_page,
    })
}

pub fn read_main_page(config: &Config, filename: &str) -> Result<ZimResolvedContent> {
    let zim = open_archive(config, filename)?;

    let main_idx = zim.header.main_page.ok_or(ZimReaderError::MainPageUnavailable)?;

    let entry = zim
        .get_by_url_index(main_idx)
        .map_err(|error| ZimReaderError::ReadFailure(format!("could not load main page directory entry: {error}")))?;

    read_content_with_redirects(&zim, entry, 0)
}

pub fn read_content_by_path(config: &Config, filename: &str, path: &str) -> Result<ZimResolvedContent> {
    let zim = open_archive(config, filename)?;
    let requested = normalize_path(path);

    let entry = find_entry(&zim, &requested).ok_or(ZimReaderError::EntryNotFound)?;

    read_content_with_redirects(&zim, entry, 0)
}

fn open_archive(config: &Config, filename: &str) -> Result<Zim> {
    if !is_safe_zim_filename(filename) {
        return Err(ZimReaderError::InvalidFilename);
    }

    let path: PathBuf = config.books_dir().join(filename);
    if !path.exists() {
        return Err(ZimReaderError::ArchiveNotFound);
    }

    let open_result = std::panic::catch_unwind(|| Zim::new(&path));
    match open_result {
        Ok(parsed) => parsed.map_err(|error| ZimReaderError::ReadFailure(error.to_string())),
        Err(_) => Err(ZimReaderError::ArchiveUnsupported),
    }
}

fn find_entry(zim: &Zim, requested: &str) -> Option<DirectoryEntry> {
    let requested_with_spaces = requested.replace('_', " ");

    zim.iterate_by_urls().find(|entry| {
        is_content_namespace(entry.namespace)
            && (entry.url == requested
                || entry.title == requested
                || entry.url == requested_with_spaces
                || entry.title == requested_with_spaces)
    })
}

fn is_content_namespace(namespace: Namespace) -> bool {
    matches!(namespace, Namespace::Articles | Namespace::Layout | Namespace::UserContent | Namespace::ImagesFile | Namespace::ImagesText)
}

fn read_content_with_redirects(zim: &Zim, mut entry: DirectoryEntry, mut redirect_hops: u8) -> Result<ZimResolvedContent> {
    loop {
        if redirect_hops > 8 {
            return Err(ZimReaderError::ReadFailure("too many redirects while resolving entry".to_string()));
        }

        match entry.target.as_ref() {
            Some(Target::Redirect(url_idx)) => {
                entry = zim
                    .get_by_url_index(*url_idx)
                    .map_err(|error| ZimReaderError::ReadFailure(format!("failed to resolve redirect target: {error}")))?;
                redirect_hops += 1;
            }
            Some(Target::Cluster(cluster_idx, blob_idx)) => {
                let cluster = zim
                    .get_cluster(*cluster_idx)
                    .map_err(|error| ZimReaderError::ReadFailure(format!("failed to load target cluster: {error}")))?;
                let blob = cluster
                    .get_blob(*blob_idx)
                    .map_err(|error| ZimReaderError::ReadFailure(format!("failed to read cluster blob: {error}")))?;
                return Ok(ZimResolvedContent {
                    content_type: mime_type_to_string(&entry.mime_type),
                    content: blob.as_ref().to_vec(),
                });
            }
            None => {
                return Err(ZimReaderError::ReadFailure("entry has no target content".to_string()));
            }
        }
    }
}

fn mime_type_to_string(mime_type: &MimeType) -> String {
    match mime_type {
        MimeType::Type(value) => value.clone(),
        MimeType::Redirect => "text/plain; charset=utf-8".to_string(),
        MimeType::LinkTarget => "application/octet-stream".to_string(),
        MimeType::DeletedEntry => "application/octet-stream".to_string(),
    }
}

fn normalize_path(path: &str) -> String {
    path.trim_matches('/').to_string()
}

fn is_safe_zim_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let is_archive = lower.ends_with(".zim") || lower.ends_with(".zimaa");

    is_archive
        && !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
}
