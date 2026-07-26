//! Unified library API — format-agnostic book metadata, table of contents, and search.
//!
//! Each supported book format (EPUB, ZIM, PDF, Markdown) implements the same
//! traits so that the unified endpoints in `handlers.rs` can delegate to the
//! correct implementation without format-specific branching.

use std::path::Path;
use tracing::warn;
use types::{BookMetadata, BookSearchResponse, SearchResult, TocEntry};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Extract unified metadata for a book file.
pub fn extract_book_metadata(
    books_dir: &Path,
    filename: &str,
) -> Result<BookMetadata, String> {
    let path = books_dir.join(filename);
    let format = detect_format(filename).ok_or_else(|| format!("unsupported format: {}", filename))?;

    let metadata = std::fs::metadata(&path).map_err(|e| format!("cannot read file: {}", e))?;
    let file_size = metadata.len();

    let title = extract_title(&path, format);
    let author = extract_author(&path, format);
    let mime_type = format_mime_type(format);
    let toc_available = format_supports_toc(format);
    let search_available = format_supports_search(format);

    Ok(BookMetadata {
        filename: filename.to_string(),
        format: format.to_string(),
        title,
        author,
        file_size,
        mime_type,
        toc_available,
        search_available,
    })
}

/// Extract the table of contents for a book file.
pub fn extract_toc(books_dir: &Path, filename: &str) -> Result<Vec<TocEntry>, String> {
    let path = books_dir.join(filename);
    let format = detect_format(filename).ok_or_else(|| format!("unsupported format: {}", filename))?;

    match format {
        "epub" => extract_epub_toc(&path),
        "zim" => extract_zim_toc(&path),
        "pdf" => extract_pdf_toc(&path),
        "md" => extract_markdown_toc(&path),
        _ => Err(format!("TOC not supported for format: {}", format)),
    }
}

/// Search within a book file.
pub fn search_book(
    books_dir: &Path,
    filename: &str,
    query: &str,
    limit: usize,
) -> Result<BookSearchResponse, String> {
    let path = books_dir.join(filename);
    let format = detect_format(filename).ok_or_else(|| format!("unsupported format: {}", filename))?;
    let needle = query.trim();

    if needle.is_empty() {
        return Ok(BookSearchResponse {
            filename: filename.to_string(),
            query: String::new(),
            results: Vec::new(),
        });
    }

    let results = match format {
        "zim" => search_zim(&path, needle, limit)?,
        "epub" => search_epub(&path, needle, limit)?,
        "md" => search_markdown(&path, needle, limit)?,
        _ => Vec::new(),
    };

    Ok(BookSearchResponse {
        filename: filename.to_string(),
        query: needle.to_string(),
        results,
    })
}

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

pub fn detect_format(filename: &str) -> Option<&'static str> {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "epub" => Some("epub"),
        "zim" => Some("zim"),
        "pdf" => Some("pdf"),
        "md" => Some("md"),
        _ => None,
    }
}

fn format_mime_type(format: &str) -> String {
    match format {
        "epub" => "application/epub+zip",
        "zim" => "application/x-zim",
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn format_supports_toc(format: &str) -> bool {
    matches!(format, "epub" | "zim" | "pdf" | "md")
}

fn format_supports_search(format: &str) -> bool {
    matches!(format, "epub" | "zim" | "md")
}

// ---------------------------------------------------------------------------
// Title extraction (shared with handlers.rs)
// ---------------------------------------------------------------------------

/// Extract the human-readable title from a book file.
pub fn extract_title(path: &Path, format: &str) -> Option<String> {
    match format {
        "epub" => extract_epub_title(path),
        "zim" => extract_zim_title(path),
        "pdf" => extract_pdf_title(path),
        "md" => extract_markdown_title(path),
        _ => None,
    }
}

/// Extract the author from a book file.
pub fn extract_author(path: &Path, format: &str) -> Option<String> {
    match format {
        "epub" => extract_epub_author(path),
        "pdf" => extract_pdf_author(path),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// EPUB helpers
// ---------------------------------------------------------------------------

fn extract_epub_title(path: &Path) -> Option<String> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let container_xml = {
        let mut entry = archive.by_name("META-INF/container.xml").ok()?;
        let mut content = String::new();
        entry.read_to_string(&mut content).ok()?;
        content
    };
    let opf_path = extract_xml_attr(&container_xml, "full-path")?;

    let opf_content = {
        let mut entry = archive.by_name(&opf_path).ok()?;
        let mut content = String::new();
        entry.read_to_string(&mut content).ok()?;
        content
    };

    extract_xml_text_content(&opf_content, "dc:title")
}

fn extract_epub_author(path: &Path) -> Option<String> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let container_xml = {
        let mut entry = archive.by_name("META-INF/container.xml").ok()?;
        let mut content = String::new();
        entry.read_to_string(&mut content).ok()?;
        content
    };
    let opf_path = extract_xml_attr(&container_xml, "full-path")?;

    let opf_content = {
        let mut entry = archive.by_name(&opf_path).ok()?;
        let mut content = String::new();
        entry.read_to_string(&mut content).ok()?;
        content
    };

    extract_xml_text_content(&opf_content, "dc:creator")
}

fn extract_epub_toc(path: &Path) -> Result<Vec<TocEntry>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open EPUB: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("invalid EPUB zip: {}", e))?;

    // Try nav.xhtml first (EPUB 3), fall back to toc.ncx (EPUB 2)
    let toc_xml = find_epub_toc_xml(&mut archive)?;
    let entries = parse_epub_nav_toc(&toc_xml)
        .or_else(|| parse_epub_ncx_toc(&toc_xml))
        .unwrap_or_default();

    Ok(entries)
}

fn find_epub_toc_xml(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<String, String> {
    // EPUB 3: nav.xhtml inside the OPF spine
    let container_xml = read_zip_entry(archive, "META-INF/container.xml")?;
    let opf_path = extract_xml_attr(&container_xml, "full-path")
        .ok_or_else(|| "cannot find OPF path in container.xml".to_string())?;
    let opf_dir = Path::new(&opf_path).parent().unwrap_or(Path::new(""));

    let opf_content = read_zip_entry(archive, &opf_path)?;

    // Look for <nav> element in nav.xhtml referenced from the OPF
    if let Some(nav_path) = find_epub3_nav_path(&opf_content) {
        let full_nav_path = opf_dir.join(&nav_path);
        let full_nav_str = full_nav_path.to_string_lossy().replace('\\', "/");
        if let Ok(content) = read_zip_entry(archive, &full_nav_str) {
            return Ok(content);
        }
    }

    // EPUB 2: toc.ncx
    let ncx_path = find_epub2_ncx_path(&opf_content);
    if let Some(ncx) = ncx_path {
        let full_ncx_path = opf_dir.join(&ncx);
        let full_ncx_str = full_ncx_path.to_string_lossy().replace('\\', "/");
        if let Ok(content) = read_zip_entry(archive, &full_ncx_str) {
            return Ok(content);
        }
    }

    Err("no TOC found in EPUB".to_string())
}

fn find_epub3_nav_path(opf_xml: &str) -> Option<String> {
    // Look for <item … properties="nav" … href="…"/>
    let nav_marker = "properties=\"nav\"";
    let idx = opf_xml.find(nav_marker)?;
    let before = &opf_xml[..idx];
    let item_start = before.rfind("<item")?;
    let item_section = &opf_xml[item_start..];
    extract_xml_attr(item_section, "href")
}

fn find_epub2_ncx_path(opf_xml: &str) -> Option<String> {
    // Look for <item … media-type="application/x-dtbncx+xml" … href="…"/>
    let ncx_marker = "application/x-dtbncx+xml";
    let idx = opf_xml.find(ncx_marker)?;
    let before = &opf_xml[..idx];
    let item_start = before.rfind("<item")?;
    let item_section = &opf_xml[item_start..];
    extract_xml_attr(item_section, "href")
}

fn parse_epub_nav_toc(xml: &str) -> Option<Vec<TocEntry>> {
    // Find <nav epub:type="toc">…</nav>
    let nav_start = xml.find("<nav")?;
    let nav_section = &xml[nav_start..];

    // Check if it's a toc nav
    if !nav_section.contains("toc") {
        return None;
    }

    let close_idx = find_matching_tag(nav_section, "nav")?;
    let nav_content = &nav_section[..=close_idx];

    let mut entries = Vec::new();
    parse_nav_ol(nav_content, 0, &mut entries);
    Some(entries)
}

fn parse_nav_ol(xml: &str, depth: u32, entries: &mut Vec<TocEntry>) {
    // Find <ol> inside the nav
    let ol_start = match xml.find("<ol") {
        Some(i) => i,
        None => return,
    };
    let ol_close = match find_matching_tag(&xml[ol_start..], "ol") {
        Some(i) => ol_start + i,
        None => return,
    };
    let ol_content = &xml[ol_start..=ol_close];

    // Parse each <li>
    let mut pos = 0;
    while let Some(li_start) = ol_content[pos..].find("<li") {
        let abs_start = pos + li_start;
        let li_close = match find_matching_tag(&ol_content[abs_start..], "li") {
            Some(i) => abs_start + i,
            None => break,
        };
        let li_section = &ol_content[abs_start..=li_close];

        // Extract <a> href and text
        if let Some(a_start) = li_section.find("<a") {
            let a_section = &li_section[a_start..];
            let a_close = match find_matching_tag(a_section, "a") {
                Some(i) => i,
                None => {
                    pos = li_close + 1;
                    continue;
                }
            };
            let a_content = &a_section[..=a_close];

            let href = extract_xml_attr(a_content, "href").unwrap_or_default();
            let text = extract_xml_text_content(a_content, "a").unwrap_or_default();

            if !text.is_empty() {
                entries.push(TocEntry {
                    id: href,
                    title: text,
                    depth,
                });
            }
        }

        // Recurse into nested <ol>
        if li_section.contains("<ol") {
            parse_nav_ol(li_section, depth + 1, entries);
        }

        pos = li_close + 1;
    }
}

fn parse_epub_ncx_toc(xml: &str) -> Option<Vec<TocEntry>> {
    let mut entries = Vec::new();
    parse_ncx_nav_points(xml, 0, &mut entries);
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn parse_ncx_nav_points(xml: &str, depth: u32, entries: &mut Vec<TocEntry>) {
    let mut pos = 0;
    while let Some(np_start) = xml[pos..].find("<navPoint") {
        let abs_start = pos + np_start;
        let np_close = match find_matching_tag(&xml[abs_start..], "navPoint") {
            Some(i) => abs_start + i,
            None => break,
        };
        let np_section = &xml[abs_start..=np_close];

        // Extract <navLabel><text>…</text></navLabel>
        let text = extract_xml_text_content(np_section, "text").unwrap_or_default();
        let src = extract_xml_attr(np_section, "src").unwrap_or_default();

        if !text.is_empty() {
            entries.push(TocEntry {
                id: src,
                title: text,
                depth,
            });
        }

        // Recurse into nested navPoints
        if np_section.contains("<navPoint") {
            parse_ncx_nav_points(np_section, depth + 1, entries);
        }

        pos = np_close + 1;
    }
}

fn search_epub(path: &Path, needle: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open EPUB: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("invalid EPUB zip: {}", e))?;

    let container_xml = read_zip_entry(&mut archive, "META-INF/container.xml")?;
    let opf_path = extract_xml_attr(&container_xml, "full-path")
        .ok_or_else(|| "cannot find OPF path".to_string())?;
    let opf_dir = Path::new(&opf_path).parent().unwrap_or(Path::new(""));
    let opf_content = read_zip_entry(&mut archive, &opf_path)?;

    // Collect spine item hrefs
    let spine_items = collect_epub_spine_items(&opf_content);

    let needle_lower = needle.to_lowercase();
    let mut results = Vec::new();

    for item_path in spine_items {
        if results.len() >= limit {
            break;
        }

        let full_path = opf_dir.join(&item_path);
        let full_str = full_path.to_string_lossy().replace('\\', "/");
        let content = match read_zip_entry(&mut archive, &full_str) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Simple text search (strip HTML tags for snippet)
        let content_lower = content.to_lowercase();
        if !content_lower.contains(&needle_lower) {
            continue;
        }

        let title = extract_xml_text_content(&content, "title").unwrap_or_else(|| item_path.clone());
        let snippet = extract_snippet(&content, &needle_lower);

        results.push(SearchResult {
            path: item_path,
            title,
            snippet,
        });
    }

    Ok(results)
}

fn collect_epub_spine_items(opf_xml: &str) -> Vec<String> {
    let mut items = Vec::new();

    // Find <spine> section
    let spine_start = match opf_xml.find("<spine") {
        Some(i) => i,
        None => return items,
    };
    let spine_close = match find_matching_tag(&opf_xml[spine_start..], "spine") {
        Some(i) => spine_start + i,
        None => return items,
    };
    let spine_section = &opf_xml[spine_start..=spine_close];

    // Collect idrefs from <itemref idref="..."/>
    let mut pos = 0;
    while let Some(ir_start) = spine_section[pos..].find("<itemref") {
        let abs_start = pos + ir_start;
        let ir_end = match spine_section[abs_start..].find('>') {
            Some(i) => abs_start + i + 1,
            None => break,
        };
        let ir_tag = &spine_section[abs_start..ir_end];
        if let Some(idref) = extract_xml_attr(ir_tag, "idref") {
            // Resolve idref to href from <manifest>
            if let Some(href) = resolve_manifest_href(opf_xml, &idref) {
                items.push(href);
            }
        }
        pos = ir_end;
    }

    items
}

fn resolve_manifest_href(opf_xml: &str, idref: &str) -> Option<String> {
    // Find <item id="idref" href="..."/>
    let search = format!("id=\"{}\"", idref);
    let idx = opf_xml.find(&search)?;
    let before = &opf_xml[..idx];
    let item_start = before.rfind("<item")?;
    let item_section = &opf_xml[item_start..];
    let item_end = item_section.find("/>")?;
    let item_tag = &item_section[..=item_end + 1];
    extract_xml_attr(item_tag, "href")
}

// ---------------------------------------------------------------------------
// ZIM helpers
// ---------------------------------------------------------------------------

fn extract_zim_title(path: &Path) -> Option<String> {
    use std::panic::AssertUnwindSafe;

    let zim = std::panic::catch_unwind(AssertUnwindSafe(|| zim::Zim::new(path)))
        .ok()?
        .ok()?;

    let content = zim.metadata("Title").ok()??;
    let blob = content.to_vec().ok()?;
    let title = String::from_utf8_lossy(&blob).trim().to_string();

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn extract_zim_toc(path: &Path) -> Result<Vec<TocEntry>, String> {
    use std::panic::AssertUnwindSafe;

    let zim = std::panic::catch_unwind(AssertUnwindSafe(|| zim::Zim::new(path)))
        .map_err(|_| "ZIM parser panicked".to_string())?
        .map_err(|e| format!("cannot open ZIM: {}", e))?;

    let mut entries = Vec::new();
    for entry_result in zim.iterate_by_urls() {
        let entry = entry_result.map_err(|e| format!("ZIM entry error: {}", e))?;

        // Only include article entries
        if !matches!(entry.namespace, zim::Namespace::Articles) {
            continue;
        }

        let path = normalize_zim_url(&entry.url);
        if path.is_empty() || path.starts_with("_assets_/") {
            continue;
        }

        let title = if entry.title.is_empty() {
            path.replace('_', " ")
        } else {
            entry.title.clone()
        };

        // Estimate depth from path segments
        let depth = path.matches('/').count() as u32;

        entries.push(TocEntry {
            id: path,
            title,
            depth,
        });
    }

    Ok(entries)
}

fn search_zim(path: &Path, needle: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    use std::panic::AssertUnwindSafe;

    let zim = std::panic::catch_unwind(AssertUnwindSafe(|| zim::Zim::new(path)))
        .map_err(|_| "ZIM parser panicked".to_string())?
        .map_err(|e| format!("cannot open ZIM: {}", e))?;

    let needle_lower = needle.to_lowercase();
    let mut results = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for entry_result in zim.iterate_by_urls() {
        if results.len() >= limit {
            break;
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !matches!(entry.namespace, zim::Namespace::Articles) {
            continue;
        }

        let path = normalize_zim_url(&entry.url);
        if path.is_empty() || path.starts_with("_assets_/") {
            continue;
        }

        let title = if entry.title.is_empty() {
            entry.url.replace('_', " ")
        } else {
            entry.title.clone()
        };

        let title_norm = title.to_lowercase();
        let path_norm = path.to_lowercase();
        if !title_norm.contains(&needle_lower) && !path_norm.contains(&needle_lower) {
            continue;
        }

        if !seen_paths.insert(path.clone()) {
            continue;
        }

        results.push(SearchResult {
            path,
            title,
            snippet: None,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// PDF helpers
// ---------------------------------------------------------------------------

fn extract_pdf_title(path: &Path) -> Option<String> {
    // Read the first few KB of the PDF and look for /Title metadata
    let bytes = std::fs::read(path).ok()?;
    let content = String::from_utf8_lossy(&bytes);
    extract_pdf_info_dict_field(&content, "Title")
}

fn extract_pdf_author(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let content = String::from_utf8_lossy(&bytes);
    extract_pdf_info_dict_field(&content, "Author")
}

fn extract_pdf_info_dict_field(pdf_text: &str, field: &str) -> Option<String> {
    // Look for /Info dictionary containing /Field (value)
    // Pattern: /Field (value) or /Field <hex>
    let pattern = format!("/{}", field);
    let idx = pdf_text.find(&pattern)?;
    let after = &pdf_text[idx + pattern.len()..];

    // Skip optional whitespace and newlines
    let after = after.trim_start();

    if let Some(parenthetical) = after.strip_prefix('(') {
        // Find matching closing paren (simple: no nesting)
        let mut depth = 1i32;
        let mut end = 0;
        for (i, ch) in parenthetical.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end > 0 {
            let value = &parenthetical[..end];
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    None
}

fn extract_pdf_toc(_path: &Path) -> Result<Vec<TocEntry>, String> {
    // PDF outline extraction is limited without a full PDF library.
    // Return an empty TOC for now — this can be enhanced with `lopdf` later.
    warn!("PDF TOC extraction not yet implemented (requires lopdf crate)");
    Ok(Vec::new())
}

// ---------------------------------------------------------------------------
// Markdown helpers
// ---------------------------------------------------------------------------

fn extract_markdown_title(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    // First heading (# Title)
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

fn extract_markdown_toc(path: &Path) -> Result<Vec<TocEntry>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read markdown: {}", e))?;
    let mut entries = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Count leading # to determine depth
        let hash_count = trimmed.chars().take_while(|c| *c == '#').count();
        if hash_count == 0 || hash_count > 6 {
            continue;
        }

        let title = trimmed[hash_count..].trim();
        if title.is_empty() {
            continue;
        }

        // Generate an anchor ID (lowercase, spaces to hyphens, strip non-alphanumeric)
        let id = title
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();

        entries.push(TocEntry {
            id,
            title: title.to_string(),
            depth: hash_count as u32 - 1,
        });
    }

    Ok(entries)
}

fn search_markdown(path: &Path, needle: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read markdown: {}", e))?;
    let needle_lower = needle.to_lowercase();
    let mut results = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        if results.len() >= limit {
            break;
        }

        let line_lower = line.to_lowercase();
        if !line_lower.contains(&needle_lower) {
            continue;
        }

        // Use the line as the snippet
        let snippet = line.trim().to_string();
        if snippet.is_empty() {
            continue;
        }

        results.push(SearchResult {
            path: format!("#{}", line_no + 1),
            title: snippet.chars().take(80).collect(),
            snippet: Some(snippet),
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Shared XML helpers
// ---------------------------------------------------------------------------

fn extract_xml_attr(xml: &str, attr: &str) -> Option<String> {
    let attr_start = xml.find(attr)?;
    let after_attr = xml[attr_start + attr.len()..].trim_start();
    let after_eq = after_attr.strip_prefix('=')?;
    let rest = after_eq.trim_start();
    let (quote, inner) = if let Some(s) = rest.strip_prefix('"') {
        ('"', s)
    } else if let Some(s) = rest.strip_prefix('\'') {
        ('\'', s)
    } else {
        return None;
    };
    let end = inner.find(quote)?;
    let value = inner[..end].trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn extract_xml_text_content(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);
    let tag_start = xml.find(&open_tag)?;
    let content_start = xml[tag_start..].find('>')? + tag_start + 1;
    let content_end = xml.find(&close_tag)?;
    if content_end <= content_start {
        return None;
    }
    let text = xml[content_start..content_end].trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Find the closing tag matching an opening tag, handling nesting.
fn find_matching_tag(xml: &str, tag: &str) -> Option<usize> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut depth = 1i32;
    // Start at 1 to skip the opening tag's '<' — depth already accounts for it
    let mut pos = 1;

    while depth > 0 {
        let next_open = xml[pos..].find(&open);
        let next_close = xml[pos..].find(&close);

        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                pos += o + 1;
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos + c + close.len() - 1);
                }
                pos += c + 1;
            }
            _ => return None,
        }
    }

    None
}

fn read_zip_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    path: &str,
) -> Result<String, String> {
    use std::io::Read;

    let mut entry = archive
        .by_name(path)
        .map_err(|e| format!("cannot read '{}' from zip: {}", path, e))?;
    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .map_err(|e| format!("cannot read '{}' content: {}", path, e))?;
    Ok(content)
}

fn extract_snippet(html: &str, needle_lower: &str) -> Option<String> {
    // Strip HTML tags for a plain-text snippet
    let text = strip_html_tags(html);
    let text_lower = text.to_lowercase();

    let idx = text_lower.find(needle_lower)?;
    let start = idx.saturating_sub(60);
    let end = (idx + needle_lower.len() + 60).min(text.len());

    let snippet = if start > 0 {
        format!("…{}…", &text[start..end])
    } else {
        text[start..end].to_string()
    };

    Some(snippet)
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    // Collapse whitespace
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
                prev_space = true;
            }
        } else {
            collapsed.push(ch);
            prev_space = false;
        }
    }

    collapsed.trim().to_string()
}

fn normalize_zim_url(value: &str) -> String {
    let raw = value
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_start_matches('/');

    let mut out = raw.to_string();
    for _ in 0..3 {
        let decoded = decode_percent_once(&out);
        if decoded == out {
            break;
        }
        out = decoded;
    }

    out
}

fn decode_percent_once(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            if let (Some(hi), Some(lo)) = (hex_nibble(hi), hex_nibble(lo)) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }

        out.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_formats() {
        assert_eq!(detect_format("book.epub"), Some("epub"));
        assert_eq!(detect_format("archive.zim"), Some("zim"));
        assert_eq!(detect_format("doc.pdf"), Some("pdf"));
        assert_eq!(detect_format("readme.md"), Some("md"));
        assert_eq!(detect_format("unknown.gguf"), None);
    }

    #[test]
    fn extracts_markdown_title_from_content() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-title.md");
        std::fs::write(&path, "# My Book Title\n\nSome content.\n").unwrap();
        assert_eq!(extract_markdown_title(&path), Some("My Book Title".to_string()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn extracts_markdown_toc() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-toc.md");
        std::fs::write(
            &path,
            "# Chapter 1\n\n## Section 1.1\n\n### Subsection\n\n# Chapter 2\n",
        )
        .unwrap();
        let toc = extract_markdown_toc(&path).unwrap();
        assert_eq!(toc.len(), 4);
        assert_eq!(toc[0].title, "Chapter 1");
        assert_eq!(toc[0].depth, 0);
        assert_eq!(toc[1].title, "Section 1.1");
        assert_eq!(toc[1].depth, 1);
        assert_eq!(toc[2].title, "Subsection");
        assert_eq!(toc[2].depth, 2);
        assert_eq!(toc[3].title, "Chapter 2");
        assert_eq!(toc[3].depth, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn searches_markdown() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-search.md");
        std::fs::write(
            &path,
            "# Title\n\nHello world.\n\nThis is a test document.\n\nAnother line.\n",
        )
        .unwrap();
        let results = search_markdown(&path, "test", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].snippet.as_deref().unwrap().contains("test"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn extracts_pdf_title_from_raw_text() {
        let pdf_text = "1 0 obj\n<< /Title (My PDF Document) /Author (Test Author) >>\nendobj";
        assert_eq!(
            extract_pdf_info_dict_field(pdf_text, "Title"),
            Some("My PDF Document".to_string())
        );
        assert_eq!(
            extract_pdf_info_dict_field(pdf_text, "Author"),
            Some("Test Author".to_string())
        );
    }

    #[test]
    fn strips_html_tags() {
        let html = "<p>Hello <b>world</b>!</p>";
        assert_eq!(strip_html_tags(html), "Hello world!");
    }

    #[test]
    fn finds_matching_tag() {
        let xml = "<nav><ol><li>item</li></ol></nav>";
        let close = find_matching_tag(xml, "nav");
        assert!(close.is_some());
        assert_eq!(&xml[close.unwrap() - 5..=close.unwrap()], "</nav>");
    }
}