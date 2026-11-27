//! Hand-built ZIM archives for tests.
//!
//! There is no ZIM writer crate, so this writes the format directly: a
//! version 6.1 archive with the new (`C`) namespace scheme, one uncompressed
//! cluster holding every blob, a `M/Title` metadata entry and an
//! `X/listing/titleOrdered/v1` article listing. That is enough for the
//! `zim` crate's directory lookups, title listings and content reads used by
//! search, retrieval and the full-text indexer.

use std::path::Path;

/// One HTML article (`C/<url>`).
#[derive(Debug, Clone)]
pub struct TestArticle {
    pub url: String,
    pub title: String,
    pub html: String,
}

impl TestArticle {
    pub fn new(url: &str, title: &str, html: &str) -> Self {
        Self {
            url: url.to_string(),
            title: title.to_string(),
            html: html.to_string(),
        }
    }
}

/// A redirect entry `C/<url>` pointing at another `C/` entry by URL.
#[derive(Debug, Clone)]
pub struct TestRedirect {
    pub url: String,
    pub title: String,
    pub target: String,
}

impl TestRedirect {
    pub fn new(url: &str, title: &str, target: &str) -> Self {
        Self {
            url: url.to_string(),
            title: title.to_string(),
            target: target.to_string(),
        }
    }
}

/// Write a ZIM archive with `articles` and `redirects` to `path`.
pub fn write_zim(path: &Path, articles: &[TestArticle], redirects: &[TestRedirect]) {
    enum Target {
        Blob(u32),
        Redirect(String),
    }
    struct Entry {
        ns: u8,
        url: String,
        title: String,
        mime: u16,
        target: Target,
    }

    const MIME_HTML: u16 = 0;
    const MIME_TEXT: u16 = 1;
    const MIME_BINARY: u16 = 2;
    const MIME_REDIRECT: u16 = 0xffff;

    let mut blobs: Vec<Vec<u8>> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    for article in articles {
        entries.push(Entry {
            ns: b'C',
            url: article.url.clone(),
            title: article.title.clone(),
            mime: MIME_HTML,
            target: Target::Blob(blobs.len() as u32),
        });
        blobs.push(article.html.as_bytes().to_vec());
    }
    for redirect in redirects {
        entries.push(Entry {
            ns: b'C',
            url: redirect.url.clone(),
            title: redirect.title.clone(),
            mime: MIME_REDIRECT,
            target: Target::Redirect(redirect.target.clone()),
        });
    }
    entries.push(Entry {
        ns: b'M',
        url: "Title".to_string(),
        title: String::new(),
        mime: MIME_TEXT,
        target: Target::Blob(blobs.len() as u32),
    });
    blobs.push(b"Test Wiki".to_vec());
    let listing_blob = blobs.len() as u32;
    blobs.push(Vec::new()); // filled in once the entry order is known
    entries.push(Entry {
        ns: b'X',
        url: "listing/titleOrdered/v1".to_string(),
        title: String::new(),
        mime: MIME_BINARY,
        target: Target::Blob(listing_blob),
    });

    // Directory entries are ordered by (namespace, url).
    entries.sort_by(|a, b| (a.ns, a.url.as_bytes()).cmp(&(b.ns, b.url.as_bytes())));
    let index_of = |url: &str| -> u32 {
        entries
            .iter()
            .position(|e| e.ns == b'C' && e.url == url)
            .unwrap_or_else(|| panic!("redirect target {url} missing")) as u32
    };

    // Article listing: every C entry, ordered by title (url when untitled).
    let mut by_title: Vec<(String, u32)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.ns == b'C')
        .map(|(i, e)| {
            let key = if e.title.is_empty() { e.url.clone() } else { e.title.clone() };
            (key, i as u32)
        })
        .collect();
    by_title.sort();
    blobs[listing_blob as usize] = by_title.iter().flat_map(|(_, i)| i.to_le_bytes()).collect();

    let header_size = 80u64;
    let mut body: Vec<u8> = Vec::new();
    for mime in ["text/html", "text/plain", "application/octet-stream"] {
        body.extend_from_slice(mime.as_bytes());
        body.push(0);
    }
    body.push(0);

    let mut dirent_offsets = Vec::new();
    for entry in &entries {
        dirent_offsets.push(header_size + body.len() as u64);
        body.extend_from_slice(&entry.mime.to_le_bytes());
        body.push(0); // parameter len
        body.push(entry.ns);
        body.extend_from_slice(&0u32.to_le_bytes()); // revision
        match &entry.target {
            Target::Blob(blob) => {
                body.extend_from_slice(&0u32.to_le_bytes()); // cluster
                body.extend_from_slice(&blob.to_le_bytes());
            }
            Target::Redirect(url) => body.extend_from_slice(&index_of(url).to_le_bytes()),
        }
        body.extend_from_slice(entry.url.as_bytes());
        body.push(0);
        body.extend_from_slice(entry.title.as_bytes());
        body.push(0);
    }

    let url_ptr_pos = header_size + body.len() as u64;
    for offset in &dirent_offsets {
        body.extend_from_slice(&offset.to_le_bytes());
    }
    let cluster_ptr_pos = header_size + body.len() as u64;
    let cluster_pos = cluster_ptr_pos + 8;
    body.extend_from_slice(&cluster_pos.to_le_bytes());

    body.push(1); // uncompressed, not extended
    let table_len = 4 * (blobs.len() as u32 + 1);
    let mut offset = table_len;
    body.extend_from_slice(&offset.to_le_bytes());
    for blob in &blobs {
        offset += blob.len() as u32;
        body.extend_from_slice(&offset.to_le_bytes());
    }
    for blob in &blobs {
        body.extend_from_slice(blob);
    }
    let checksum_pos = header_size + body.len() as u64;

    let mut header = Vec::new();
    header.extend_from_slice(&72173914u32.to_le_bytes());
    header.extend_from_slice(&6u16.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&[7u8; 16]);
    header.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    header.extend_from_slice(&1u32.to_le_bytes());
    header.extend_from_slice(&url_ptr_pos.to_le_bytes());
    header.extend_from_slice(&u64::MAX.to_le_bytes()); // no legacy title pointer list
    header.extend_from_slice(&cluster_ptr_pos.to_le_bytes());
    header.extend_from_slice(&header_size.to_le_bytes());
    header.extend_from_slice(&u32::MAX.to_le_bytes());
    header.extend_from_slice(&u32::MAX.to_le_bytes());
    header.extend_from_slice(&checksum_pos.to_le_bytes());
    assert_eq!(header.len() as u64, header_size);

    let mut file = header;
    file.extend_from_slice(&body);
    file.extend_from_slice(&[0u8; 16]);
    std::fs::write(path, file).expect("write zim");
}

/// The small two-article archive used by the retrieval tests: "Gustav Vasa"
/// and "Kings of Sweden" plus a "Vasa" redirect to the former.
pub fn write_sample_zim(path: &Path) {
    let gustav = "<html><head><title>Gustav Vasa</title></head><body><h1>Gustav Vasa</h1>\
        <p>Gustav Vasa was King of Sweden from 1523 until his death in 1560.[1]</p>\
        <p>He is remembered as the founder of the modern Swedish state.</p></body></html>";
    let kings = "<html><body><p>The list of kings of Sweden starts in the early Middle Ages.</p></body></html>";
    write_zim(
        path,
        &[
            TestArticle::new("Gustav_Vasa", "Gustav Vasa", gustav),
            TestArticle::new("Kings_of_Sweden", "Kings of Sweden", kings),
        ],
        &[TestRedirect::new("Vasa", "Vasa", "Gustav_Vasa")],
    );
}
