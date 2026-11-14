//! Pure-Rust, `unsafe`-free EPUB 2 & EPUB 3 parser and plain-text chapter extractor.
//!
//! Reads bounded EPUB ZIP archives using `zip` and `std` without any WebView,
//! HTML/DOM engine, or Core/UI dependencies.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// Maximum EPUB archive size accepted on mobile/desktop (64 MiB).
pub const MAX_EPUB_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of spine chapters extracted from a single EPUB.
pub const MAX_CHAPTERS: usize = 500;
/// Maximum UTF-8 plain-text byte length allowed per chapter (2 MiB).
pub const MAX_CHAPTER_TEXT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum raw XML/XHTML entry size read from the ZIP container (8 MiB).
pub const MAX_ENTRY_RAW_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of table-of-contents entries extracted.
pub const MAX_TOC_ENTRIES: usize = 512;
/// Maximum nesting depth for table-of-contents entries.
pub const MAX_TOC_DEPTH: u8 = 8;

/// A single entry in the EPUB table of contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubTocEntry {
    pub title: String,
    pub chapter_index: usize,
    pub href: String,
    pub depth: u8,
}

/// An extracted plain-text chapter from the EPUB spine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubChapter {
    pub index: usize,
    pub id: String,
    pub href: String,
    pub title: Option<String>,
    pub text: String,
}

/// Parsed metadata, ordered spine chapters, and table of contents of an EPUB book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubBook {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub chapters: Vec<EpubChapter>,
    pub toc: Vec<EpubTocEntry>,
}

#[derive(Clone, Debug)]
struct ManifestItem {
    href: String,
    media_type: String,
    properties: String,
}

#[derive(Clone, Debug)]
struct OpfPackage {
    title: Option<String>,
    creator: Option<String>,
    manifest: HashMap<String, ManifestItem>,
    spine_idrefs: Vec<String>,
    spine_toc_id: Option<String>,
}

#[derive(Clone, Debug)]
struct RawTocItem {
    title: String,
    href: String,
    depth: u8,
}

#[derive(Default)]
struct NavPointFrame {
    depth: usize,
    title: String,
    href: Option<String>,
    emitted: bool,
}

/// Parses an EPUB 2 or EPUB 3 archive from raw bytes into metadata, TOC, and plain-text chapters.
pub fn parse_epub(bytes: &[u8]) -> Result<EpubBook, String> {
    if bytes.is_empty() {
        return Err("EPUB input is empty".into());
    }
    if bytes.len() > MAX_EPUB_BYTES {
        return Err(format!(
            "EPUB size {} exceeds maximum budget of {} bytes",
            bytes.len(),
            MAX_EPUB_BYTES
        ));
    }

    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|err| format!("invalid EPUB ZIP archive: {err}"))?;

    let container_xml = read_zip_utf8(&mut archive, "META-INF/container.xml", MAX_ENTRY_RAW_BYTES)?;
    let opf_path = parse_container_rootfile(&container_xml)?;
    let opf_xml = read_zip_utf8(&mut archive, &opf_path, MAX_ENTRY_RAW_BYTES)?;
    let opf_dir = parent_dir(&opf_path);
    let package = parse_opf(&opf_xml, &opf_dir)?;

    if package.spine_idrefs.is_empty() {
        return Err("EPUB spine contains no chapters".into());
    }

    let mut chapters = Vec::new();
    let mut href_to_chapter = HashMap::new();
    for idref in &package.spine_idrefs {
        if chapters.len() >= MAX_CHAPTERS {
            return Err(format!(
                "EPUB spine exceeds maximum of {MAX_CHAPTERS} chapters"
            ));
        }
        let item = package
            .manifest
            .get(idref)
            .ok_or_else(|| format!("spine idref '{idref}' missing from OPF manifest"))?;
        if !is_html_media_type(&item.media_type) {
            continue;
        }
        let html = read_zip_utf8(&mut archive, &item.href, MAX_ENTRY_RAW_BYTES)?;
        let (text, head_title) = extract_chapter_text_and_title(&html);
        if text.len() > MAX_CHAPTER_TEXT_BYTES {
            return Err(format!(
                "chapter '{}' text length {} exceeds {} byte budget",
                item.href,
                text.len(),
                MAX_CHAPTER_TEXT_BYTES
            ));
        }
        let index = chapters.len();
        href_to_chapter.entry(item.href.clone()).or_insert(index);
        chapters.push(EpubChapter {
            index,
            id: idref.clone(),
            href: item.href.clone(),
            title: head_title,
            text,
        });
    }

    if chapters.is_empty() {
        return Err("EPUB spine produced no readable chapters".into());
    }

    let toc = extract_toc(&mut archive, &package, &href_to_chapter)?;
    for entry in &toc {
        if let Some(chapter) = chapters.get_mut(entry.chapter_index)
            && chapter.title.is_none()
        {
            chapter.title = Some(entry.title.clone());
        }
    }

    Ok(EpubBook {
        title: package.title,
        creator: package.creator,
        chapters,
        toc,
    })
}

/// Converts an XHTML/HTML chapter document into normalized UTF-8 plain text.
pub fn html_to_plain_text(html: &str) -> String {
    extract_chapter_text_and_title(html).0
}

fn read_zip_utf8(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
    max_bytes: usize,
) -> Result<String, String> {
    let normalized = sanitize_archive_path(path)?;
    let mut entry = archive
        .by_name(&normalized)
        .map_err(|err| format!("missing or unreadable EPUB entry '{normalized}': {err}"))?;
    if entry.is_dir() {
        return Err(format!("EPUB entry '{normalized}' is a directory"));
    }
    if entry.size() > max_bytes as u64 {
        return Err(format!(
            "EPUB entry '{normalized}' size {} exceeds {max_bytes} byte budget",
            entry.size()
        ));
    }
    let mut buf = Vec::with_capacity(entry.size().min(64 * 1024) as usize);
    entry
        .by_ref()
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|err| format!("failed reading EPUB entry '{normalized}': {err}"))?;
    if buf.len() > max_bytes {
        return Err(format!(
            "EPUB entry '{normalized}' expanded beyond {max_bytes} byte budget"
        ));
    }
    let text = String::from_utf8(buf)
        .map_err(|err| format!("EPUB entry '{normalized}' is not valid UTF-8: {err}"))?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

fn sanitize_archive_path(raw: &str) -> Result<String, String> {
    let without_fragment = raw.split('#').next().unwrap_or(raw);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let decoded = percent_decode(without_query)?;
    let trimmed = decoded.trim();
    if trimmed.is_empty() {
        return Err("empty EPUB path".into());
    }
    if trimmed.starts_with('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(format!("invalid EPUB path '{raw}'"));
    }
    let mut parts = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(format!("path traversal rejected in EPUB path '{raw}'"));
        }
        parts.push(segment);
    }
    if parts.is_empty() {
        return Err(format!("empty EPUB path '{raw}'"));
    }
    Ok(parts.join("/"))
}

fn resolve_relative_path(base_dir: &str, href: &str) -> Result<String, String> {
    let cleaned = sanitize_archive_path(href)?;
    if base_dir.is_empty() {
        return Ok(cleaned);
    }
    Ok(format!("{}/{}", base_dir.trim_end_matches('/'), cleaned))
}

fn parent_dir(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => String::new(),
    }
}

fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("invalid percent-encoding in EPUB path '{input}'"));
            }
            let hi = hex_val(bytes[i + 1])
                .ok_or_else(|| format!("invalid percent-encoding in EPUB path '{input}'"))?;
            let lo = hex_val(bytes[i + 2])
                .ok_or_else(|| format!("invalid percent-encoding in EPUB path '{input}'"))?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| format!("non-UTF-8 EPUB path '{input}'"))
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_html_media_type(media_type: &str) -> bool {
    let mt = media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        mt.as_str(),
        "application/xhtml+xml" | "text/html" | "application/x-dtbook+xml" | ""
    )
}

fn parse_container_rootfile(xml: &str) -> Result<String, String> {
    let mut fallback = None;
    for token in tokenize_xml(xml) {
        if let XmlToken::Start {
            local_name, attrs, ..
        } = token
            && local_name == "rootfile"
            && let Some(full_path) = attr_value(&attrs, "full-path")
        {
            let sanitized = sanitize_archive_path(&full_path)?;
            let media = attr_value(&attrs, "media-type").unwrap_or_default();
            if media == "application/oebps-package+xml" {
                return Ok(sanitized);
            }
            if fallback.is_none() {
                fallback = Some(sanitized);
            }
        }
    }
    fallback.ok_or_else(|| "META-INF/container.xml has no OPF rootfile".into())
}

fn parse_opf(xml: &str, opf_dir: &str) -> Result<OpfPackage, String> {
    let mut title: Option<String> = None;
    let mut creator: Option<String> = None;
    let mut manifest = HashMap::new();
    let mut spine_idrefs = Vec::new();
    let mut spine_toc_id: Option<String> = None;

    let mut in_metadata = false;
    let mut in_manifest = false;
    let mut in_spine = false;
    let mut capturing_title = false;
    let mut capturing_creator = false;
    let mut text_buf = String::new();

    for token in tokenize_xml(xml) {
        match token {
            XmlToken::Start {
                full_name,
                local_name,
                attrs,
                self_closing,
            } => {
                if local_name == "metadata" {
                    if !self_closing {
                        in_metadata = true;
                    }
                } else if local_name == "manifest" {
                    if !self_closing {
                        in_manifest = true;
                    }
                } else if local_name == "spine" {
                    spine_toc_id = attr_value(&attrs, "toc");
                    if !self_closing {
                        in_spine = true;
                    }
                } else if in_metadata && !self_closing {
                    if (full_name == "dc:title" || local_name == "title") && title.is_none() {
                        capturing_title = true;
                        text_buf.clear();
                    } else if (full_name == "dc:creator" || local_name == "creator")
                        && creator.is_none()
                    {
                        capturing_creator = true;
                        text_buf.clear();
                    }
                } else if in_manifest && local_name == "item" {
                    if let (Some(id), Some(href)) =
                        (attr_value(&attrs, "id"), attr_value(&attrs, "href"))
                    {
                        let resolved = resolve_relative_path(opf_dir, &href)?;
                        let media_type = attr_value(&attrs, "media-type").unwrap_or_default();
                        let properties = attr_value(&attrs, "properties").unwrap_or_default();
                        manifest.insert(
                            id,
                            ManifestItem {
                                href: resolved,
                                media_type,
                                properties,
                            },
                        );
                    }
                } else if in_spine
                    && local_name == "itemref"
                    && let Some(idref) = attr_value(&attrs, "idref")
                    && !idref.trim().is_empty()
                {
                    spine_idrefs.push(idref);
                }
            }
            XmlToken::End {
                full_name,
                local_name,
            } => {
                if local_name == "metadata" {
                    in_metadata = false;
                    capturing_title = false;
                    capturing_creator = false;
                } else if local_name == "manifest" {
                    in_manifest = false;
                } else if local_name == "spine" {
                    in_spine = false;
                } else if capturing_title && (full_name == "dc:title" || local_name == "title") {
                    capturing_title = false;
                    let cleaned = collapse_whitespace(&text_buf);
                    if !cleaned.is_empty() {
                        title = Some(cleaned);
                    }
                } else if capturing_creator
                    && (full_name == "dc:creator" || local_name == "creator")
                {
                    capturing_creator = false;
                    let cleaned = collapse_whitespace(&text_buf);
                    if !cleaned.is_empty() {
                        creator = Some(cleaned);
                    }
                }
            }
            XmlToken::Text(t) => {
                if capturing_title || capturing_creator {
                    text_buf.push_str(&decode_entities(&t));
                }
            }
        }
    }

    Ok(OpfPackage {
        title,
        creator,
        manifest,
        spine_idrefs,
        spine_toc_id,
    })
}

fn extract_toc(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    package: &OpfPackage,
    href_to_chapter: &HashMap<String, usize>,
) -> Result<Vec<EpubTocEntry>, String> {
    let nav_item = package.manifest.values().find(|item| {
        is_html_media_type(&item.media_type)
            && item.properties.split_whitespace().any(|prop| prop == "nav")
    });

    let mut raw_items = Vec::new();
    if let Some(nav) = nav_item {
        let nav_xml = read_zip_utf8(archive, &nav.href, MAX_ENTRY_RAW_BYTES)?;
        let nav_dir = parent_dir(&nav.href);
        raw_items = parse_epub3_nav(&nav_xml, &nav_dir)?;
    }

    if raw_items.is_empty() {
        let ncx_item = package
            .spine_toc_id
            .as_deref()
            .and_then(|id| package.manifest.get(id))
            .or_else(|| {
                package.manifest.values().find(|item| {
                    item.media_type
                        .eq_ignore_ascii_case("application/x-dtbncx+xml")
                })
            });
        if let Some(ncx) = ncx_item {
            let ncx_xml = read_zip_utf8(archive, &ncx.href, MAX_ENTRY_RAW_BYTES)?;
            let ncx_dir = parent_dir(&ncx.href);
            raw_items = parse_epub2_ncx(&ncx_xml, &ncx_dir)?;
        }
    }

    let mut toc = Vec::new();
    for item in raw_items {
        if toc.len() >= MAX_TOC_ENTRIES {
            break;
        }
        if item.title.is_empty() || item.depth > MAX_TOC_DEPTH {
            continue;
        }
        if let Some(&chapter_index) = href_to_chapter.get(&item.href) {
            toc.push(EpubTocEntry {
                title: item.title,
                chapter_index,
                href: item.href,
                depth: item.depth,
            });
        }
    }
    Ok(toc)
}

fn parse_epub3_nav(xhtml: &str, base_dir: &str) -> Result<Vec<RawTocItem>, String> {
    let mut toc_items = Vec::new();
    let mut fallback_items = Vec::new();
    let mut found_explicit_toc = false;

    let mut in_nav = false;
    let mut is_toc_nav = false;
    let mut nav_depth: usize = 0;
    let mut ol_depth: usize = 0;
    let mut active_href: Option<String> = None;
    let mut active_text = String::new();

    for token in tokenize_xml(xhtml) {
        match token {
            XmlToken::Start {
                local_name,
                attrs,
                self_closing,
                ..
            } => {
                if local_name == "nav" {
                    if !in_nav && !self_closing {
                        in_nav = true;
                        nav_depth = 1;
                        ol_depth = 0;
                        let epub_type = attr_value_any(&attrs, &["epub:type", "type"])
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                        is_toc_nav = epub_type.split_whitespace().any(|t| t == "toc");
                        if is_toc_nav {
                            found_explicit_toc = true;
                        }
                    } else if in_nav && !self_closing {
                        nav_depth += 1;
                    }
                } else if in_nav && !self_closing {
                    if local_name == "ol" {
                        ol_depth += 1;
                    } else if local_name == "a" && ol_depth > 0 {
                        active_href = attr_value(&attrs, "href");
                        active_text.clear();
                    }
                }
            }
            XmlToken::End { local_name, .. } => {
                if in_nav {
                    if local_name == "nav" {
                        nav_depth = nav_depth.saturating_sub(1);
                        if nav_depth == 0 {
                            in_nav = false;
                            is_toc_nav = false;
                            ol_depth = 0;
                            active_href = None;
                        }
                    } else if local_name == "ol" {
                        ol_depth = ol_depth.saturating_sub(1);
                    } else if local_name == "a" {
                        if let Some(href) = active_href.take() {
                            let title = collapse_whitespace(&active_text);
                            let depth = ol_depth.saturating_sub(1);
                            if !title.is_empty() && depth <= MAX_TOC_DEPTH as usize {
                                let resolved = resolve_relative_path(base_dir, &href)?;
                                let item = RawTocItem {
                                    title,
                                    href: resolved,
                                    depth: depth as u8,
                                };
                                if is_toc_nav {
                                    if toc_items.len() < MAX_TOC_ENTRIES {
                                        toc_items.push(item);
                                    }
                                } else if !found_explicit_toc
                                    && fallback_items.len() < MAX_TOC_ENTRIES
                                {
                                    fallback_items.push(item);
                                }
                            }
                        }
                        active_text.clear();
                    }
                }
            }
            XmlToken::Text(t) => {
                if in_nav && active_href.is_some() {
                    active_text.push_str(&decode_entities(&t));
                }
            }
        }
    }

    if found_explicit_toc {
        Ok(toc_items)
    } else {
        Ok(fallback_items)
    }
}

fn parse_epub2_ncx(ncx: &str, base_dir: &str) -> Result<Vec<RawTocItem>, String> {
    let mut items = Vec::new();
    let mut in_nav_map = false;
    let mut in_nav_label = false;
    let mut in_text = false;
    let mut text_buf = String::new();
    let mut frames: Vec<NavPointFrame> = Vec::new();

    for token in tokenize_xml(ncx) {
        match token {
            XmlToken::Start {
                local_name,
                attrs,
                self_closing,
                ..
            } => {
                if local_name == "navmap" {
                    if !self_closing {
                        in_nav_map = true;
                    }
                } else if in_nav_map {
                    if local_name == "navpoint" {
                        if !self_closing {
                            let depth = frames.len();
                            frames.push(NavPointFrame {
                                depth,
                                ..NavPointFrame::default()
                            });
                        }
                    } else if local_name == "navlabel" && !self_closing {
                        in_nav_label = true;
                    } else if local_name == "text" && in_nav_label && !self_closing {
                        in_text = true;
                        text_buf.clear();
                    } else if local_name == "content"
                        && let Some(src) = attr_value(&attrs, "src")
                        && let Some(frame) = frames.last_mut()
                        && frame.href.is_none()
                    {
                        frame.href = Some(src);
                        emit_ncx_frame(frame, base_dir, &mut items)?;
                    }
                }
            }
            XmlToken::End { local_name, .. } => {
                if local_name == "navmap" {
                    in_nav_map = false;
                    in_nav_label = false;
                    in_text = false;
                    frames.clear();
                } else if in_nav_map {
                    if local_name == "text" && in_text {
                        in_text = false;
                        let title = collapse_whitespace(&text_buf);
                        if let Some(frame) = frames.last_mut()
                            && frame.title.is_empty()
                            && !title.is_empty()
                        {
                            frame.title = title;
                            emit_ncx_frame(frame, base_dir, &mut items)?;
                        }
                    } else if local_name == "navlabel" {
                        in_nav_label = false;
                    } else if local_name == "navpoint"
                        && let Some(mut frame) = frames.pop()
                    {
                        emit_ncx_frame(&mut frame, base_dir, &mut items)?;
                    }
                }
            }
            XmlToken::Text(t) => {
                if in_nav_map && in_text {
                    text_buf.push_str(&decode_entities(&t));
                }
            }
        }
    }

    Ok(items)
}

fn emit_ncx_frame(
    frame: &mut NavPointFrame,
    base_dir: &str,
    items: &mut Vec<RawTocItem>,
) -> Result<(), String> {
    if frame.emitted || frame.title.is_empty() {
        return Ok(());
    }
    let Some(src) = frame.href.as_deref() else {
        return Ok(());
    };
    frame.emitted = true;
    if items.len() >= MAX_TOC_ENTRIES || frame.depth > MAX_TOC_DEPTH as usize {
        return Ok(());
    }
    let resolved = resolve_relative_path(base_dir, src)?;
    items.push(RawTocItem {
        title: frame.title.clone(),
        href: resolved,
        depth: frame.depth as u8,
    });
    Ok(())
}

fn extract_chapter_text_and_title(html: &str) -> (String, Option<String>) {
    let mut out = String::new();
    let mut head_title: Option<String> = None;
    let mut skip_stack: Vec<String> = Vec::new();
    let mut in_head = false;
    let mut in_head_title = false;
    let mut head_title_buf = String::new();
    let mut pending_space = false;
    let mut pending_newlines: u8 = 0;

    for token in tokenize_xml(html) {
        match token {
            XmlToken::Start {
                local_name,
                self_closing,
                ..
            } => {
                if skip_stack.is_empty() {
                    if is_skipped_container(&local_name) {
                        if local_name == "head" {
                            in_head = !self_closing;
                        }
                        if !self_closing {
                            skip_stack.push(local_name);
                        }
                    } else if let Some(breaks) = block_break_newlines(&local_name) {
                        pending_newlines = pending_newlines.max(breaks);
                        pending_space = false;
                    }
                } else {
                    if in_head && local_name == "title" && head_title.is_none() && !self_closing {
                        in_head_title = true;
                        head_title_buf.clear();
                    }
                    if !self_closing && local_name == *skip_stack.last().unwrap() {
                        skip_stack.push(local_name);
                    }
                }
            }
            XmlToken::End { local_name, .. } => {
                if !skip_stack.is_empty() {
                    if in_head_title && local_name == "title" {
                        in_head_title = false;
                        let cleaned = collapse_whitespace(&head_title_buf);
                        if !cleaned.is_empty() {
                            head_title = Some(cleaned);
                        }
                    }
                    if skip_stack.last().is_some_and(|top| top == &local_name) {
                        skip_stack.pop();
                        if skip_stack.is_empty() {
                            in_head = false;
                            in_head_title = false;
                        }
                    }
                } else if let Some(breaks) = block_break_newlines(&local_name) {
                    pending_newlines = pending_newlines.max(breaks);
                    pending_space = false;
                }
            }
            XmlToken::Text(raw) => {
                let decoded = decode_entities(&raw);
                if in_head_title {
                    head_title_buf.push_str(&decoded);
                }
                if !skip_stack.is_empty() {
                    continue;
                }
                for ch in decoded.chars() {
                    if ch.is_whitespace() {
                        if !out.is_empty() && pending_newlines == 0 {
                            pending_space = true;
                        }
                    } else {
                        if !out.is_empty() {
                            if pending_newlines > 0 {
                                for _ in 0..pending_newlines {
                                    out.push('\n');
                                }
                            } else if pending_space {
                                out.push(' ');
                            }
                        }
                        pending_space = false;
                        pending_newlines = 0;
                        out.push(ch);
                    }
                }
            }
        }
    }

    (out, head_title)
}

fn is_skipped_container(local_name: &str) -> bool {
    matches!(local_name, "head" | "script" | "style")
}

fn block_break_newlines(local_name: &str) -> Option<u8> {
    match local_name {
        "br" | "li" | "tr" => Some(1),
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "hr" => Some(2),
        _ => None,
    }
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_ws = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                in_ws = true;
            }
        } else {
            if in_ws {
                out.push(' ');
                in_ws = false;
            }
            out.push(ch);
        }
    }
    out
}

fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after_amp = &rest[amp + 1..];
        if let Some(semi) = after_amp.find(';')
            && semi > 0
            && semi <= 16
        {
            let entity = &after_amp[..semi];
            if let Some(decoded) = decode_single_entity(entity) {
                out.push(decoded);
                rest = &after_amp[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after_amp;
    }
    out.push_str(rest);
    out
}

fn decode_single_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "mdash" => Some('\u{2014}'),
        "ndash" => Some('\u{2013}'),
        "hellip" => Some('\u{2026}'),
        "lsquo" => Some('\u{2018}'),
        "rsquo" => Some('\u{2019}'),
        "ldquo" => Some('\u{201C}'),
        "rdquo" => Some('\u{201D}'),
        _ if entity.starts_with('#') => {
            let digits = &entity[1..];
            let code = if let Some(hex) = digits
                .strip_prefix('x')
                .or_else(|| digits.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                digits.parse::<u32>().ok()?
            };
            if code == 0 {
                return None;
            }
            char::from_u32(code)
        }
        _ => None,
    }
}

enum XmlToken {
    Start {
        full_name: String,
        local_name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    End {
        full_name: String,
        local_name: String,
    },
    Text(String),
}

fn tokenize_xml(input: &str) -> Vec<XmlToken> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] != b'<' {
            let start = i;
            while i < len && bytes[i] != b'<' {
                i += 1;
            }
            if start < i {
                tokens.push(XmlToken::Text(input[start..i].to_string()));
            }
            continue;
        }

        if input[i..].starts_with("<!--") {
            if let Some(end_rel) = input[i + 4..].find("-->") {
                i += 4 + end_rel + 3;
            } else {
                break;
            }
            continue;
        }

        if input[i..].starts_with("<![CDATA[") {
            let content_start = i + 9;
            if let Some(end_rel) = input[content_start..].find("]]>") {
                tokens.push(XmlToken::Text(
                    input[content_start..content_start + end_rel].to_string(),
                ));
                i = content_start + end_rel + 3;
            } else {
                tokens.push(XmlToken::Text(input[content_start..].to_string()));
                break;
            }
            continue;
        }

        if input[i..].starts_with("<?") {
            if let Some(end_rel) = input[i + 2..].find("?>") {
                i += 2 + end_rel + 2;
            } else {
                break;
            }
            continue;
        }

        if input[i..].starts_with("<!") {
            i = skip_declaration(input, i + 2);
            continue;
        }

        let Some(tag_end) = find_tag_end(input, i + 1) else {
            break;
        };
        let raw_tag = input[i + 1..tag_end].trim();
        i = tag_end + 1;

        if raw_tag.is_empty() {
            continue;
        }

        if let Some(closing) = raw_tag.strip_prefix('/') {
            let name = closing
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if !name.is_empty() {
                let local_name = local_tag_name(&name).to_string();
                tokens.push(XmlToken::End {
                    full_name: name,
                    local_name,
                });
            }
            continue;
        }

        let (body, self_closing) = if let Some(stripped) = raw_tag.strip_suffix('/') {
            (stripped.trim_end(), true)
        } else {
            (raw_tag, false)
        };
        if let Some((full_name, attrs)) = parse_tag_header(body) {
            let local_name = local_tag_name(&full_name).to_string();
            tokens.push(XmlToken::Start {
                full_name,
                local_name,
                attrs,
                self_closing,
            });
        }
    }

    tokens
}

fn skip_declaration(input: &str, mut pos: usize) -> usize {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut quote: Option<u8> = None;
    let mut bracket_depth: usize = 0;
    while pos < len {
        let b = bytes[pos];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else {
            match b {
                b'\'' | b'"' => quote = Some(b),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'>' if bracket_depth == 0 => return pos + 1,
                _ => {}
            }
        }
        pos += 1;
    }
    len
}

fn find_tag_end(input: &str, mut pos: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut quote: Option<u8> = None;
    while pos < len {
        let b = bytes[pos];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else {
            match b {
                b'\'' | b'"' => quote = Some(b),
                b'>' => return Some(pos),
                _ => {}
            }
        }
        pos += 1;
    }
    None
}

fn parse_tag_header(body: &str) -> Option<(String, Vec<(String, String)>)> {
    let bytes = body.as_bytes();
    let len = bytes.len();
    let mut pos = 0;
    while pos < len && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let name_start = pos;
    while pos < len && !bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    if name_start == pos {
        return None;
    }
    let full_name = body[name_start..pos].to_ascii_lowercase();
    let mut attrs = Vec::new();

    while pos < len {
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        let key_start = pos;
        while pos < len && bytes[pos] != b'=' && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let key = body[key_start..pos].to_ascii_lowercase();
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos < len && bytes[pos] == b'=' {
            pos += 1;
            while pos < len && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let val = if pos < len && (bytes[pos] == b'"' || bytes[pos] == b'\'') {
                let q = bytes[pos];
                pos += 1;
                let v_start = pos;
                while pos < len && bytes[pos] != q {
                    pos += 1;
                }
                let raw_val = &body[v_start..pos];
                if pos < len {
                    pos += 1;
                }
                decode_entities(raw_val)
            } else {
                let v_start = pos;
                while pos < len && !bytes[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                decode_entities(&body[v_start..pos])
            };
            if !key.is_empty() {
                attrs.push((key, val));
            }
        } else if !key.is_empty() {
            attrs.push((key, String::new()));
        }
    }

    Some((full_name, attrs))
}

fn local_tag_name(full_name: &str) -> &str {
    full_name
        .rsplit_once(':')
        .map_or(full_name, |(_, local)| local)
}

fn attr_value(attrs: &[(String, String)], key: &str) -> Option<String> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn attr_value_any(attrs: &[(String, String)], keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(val) = attr_value(attrs, key) {
            return Some(val);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for (name, content) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(content.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn parses_epub3_nav_metadata_and_clean_plain_text() {
        let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

        let opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>  The   EPUB 3   Test Book  </dc:title>
    <dc:creator>Ada &amp; Bob</dc:creator>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="style.css" media-type="text/css"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
  </spine>
</package>"#;

        let nav = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
  <body>
    <nav epub:type="toc">
      <ol>
        <li><a href="text/ch1.xhtml#top">Chapter <span>One</span></a>
          <ol>
            <li><a href="text/ch2.xhtml">Chapter Two</a></li>
          </ol>
        </li>
      </ol>
    </nav>
  </body>
</html>"#;

        let ch1 = r#"<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <title>Ignored Head Title</title>
    <style>body { color: red; }</style>
    <script>console.log("ignored");</script>
  </head>
  <body>
    <h1>Heading &amp; Intro</h1>
    <p>First   paragraph&nbsp;with <em>inline</em> &lt;tags&gt; &quot;quoted&quot; &apos;ok&apos;.</p>
    <div>Line 1<br/>Line 2&#33;&#x20;Done.</div>
    <blockquote>Quote block</blockquote>
    <ul><li>Item A</li><li>Item B</li></ul>
  </body>
</html>"#;

        let ch2 =
            r#"<html><body><p>Second chapter &mdash; &#8212; &#x4E2D;&#x6587;.</p></body></html>"#;

        let bytes = build_zip(&[
            ("META-INF/container.xml", container),
            ("OEBPS/package.opf", opf),
            ("OEBPS/nav.xhtml", nav),
            ("OEBPS/text/ch1.xhtml", ch1),
            ("OEBPS/text/ch2.xhtml", ch2),
            ("OEBPS/style.css", "body { margin: 0; }"),
        ]);

        let book = parse_epub(&bytes).unwrap();
        assert_eq!(book.title.as_deref(), Some("The EPUB 3 Test Book"));
        assert_eq!(book.creator.as_deref(), Some("Ada & Bob"));
        assert_eq!(book.chapters.len(), 2);
        assert_eq!(
            book.chapters[0].text,
            "Heading & Intro\n\nFirst paragraph with inline <tags> \"quoted\" 'ok'.\n\nLine 1\nLine 2! Done.\n\nQuote block\n\nItem A\nItem B"
        );
        assert_eq!(
            book.chapters[1].text,
            "Second chapter \u{2014} \u{2014} 中文."
        );
        assert_eq!(
            book.toc,
            vec![
                EpubTocEntry {
                    title: "Chapter One".into(),
                    chapter_index: 0,
                    href: "OEBPS/text/ch1.xhtml".into(),
                    depth: 0,
                },
                EpubTocEntry {
                    title: "Chapter Two".into(),
                    chapter_index: 1,
                    href: "OEBPS/text/ch2.xhtml".into(),
                    depth: 1,
                },
            ]
        );
    }

    #[test]
    fn parses_epub2_ncx_and_nested_navpoints() {
        let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

        let opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>EPUB 2 Classic</dc:title>
    <dc:creator>Jane Doe</dc:creator>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="p1" href="part%201.html" media-type="text/html"/>
    <item id="p2" href="part2.html" media-type="text/html"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="p1"/>
    <itemref idref="p2"/>
  </spine>
</package>"#;

        let ncx = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="np-1" playOrder="1">
      <navLabel><text>Part One</text></navLabel>
      <content src="part%201.html"/>
      <navPoint id="np-2" playOrder="2">
        <navLabel><text>Subsection</text></navLabel>
        <content src="part2.html#s1"/>
      </navPoint>
    </navPoint>
  </navMap>
</ncx>"#;

        let p1 = "<html><head><title>P1</title></head><body><table><tr><td>Cell 1</td></tr><tr><td>Cell 2</td></tr></table><hr/><p>End</p></body></html>";
        let p2 = "<html><body><p>Part two body.</p></body></html>";

        let bytes = build_zip(&[
            ("META-INF/container.xml", container),
            ("OPS/content.opf", opf),
            ("OPS/toc.ncx", ncx),
            ("OPS/part 1.html", p1),
            ("OPS/part2.html", p2),
        ]);

        let book = parse_epub(&bytes).unwrap();
        assert_eq!(book.title.as_deref(), Some("EPUB 2 Classic"));
        assert_eq!(book.creator.as_deref(), Some("Jane Doe"));
        assert_eq!(book.chapters.len(), 2);
        assert_eq!(book.chapters[0].text, "Cell 1\nCell 2\n\nEnd");
        assert_eq!(book.chapters[1].text, "Part two body.");
        assert_eq!(
            book.toc,
            vec![
                EpubTocEntry {
                    title: "Part One".into(),
                    chapter_index: 0,
                    href: "OPS/part 1.html".into(),
                    depth: 0,
                },
                EpubTocEntry {
                    title: "Subsection".into(),
                    chapter_index: 1,
                    href: "OPS/part2.html".into(),
                    depth: 1,
                },
            ]
        );
    }

    #[test]
    fn rejects_path_traversal_and_chapter_budget() {
        let container = r#"<container><rootfiles><rootfile full-path="../secret.opf"/></rootfiles></container>"#;
        let traversal_bytes = build_zip(&[("META-INF/container.xml", container)]);
        let err = parse_epub(&traversal_bytes).unwrap_err();
        assert!(err.contains("traversal"), "unexpected error: {err}");

        let container_ok =
            r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#;
        let opf_traversal = r#"<package><manifest><item id="c1" href="sub/../evil.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let manifest_traversal = build_zip(&[
            ("META-INF/container.xml", container_ok),
            ("content.opf", opf_traversal),
        ]);
        assert!(
            parse_epub(&manifest_traversal)
                .unwrap_err()
                .contains("traversal")
        );

        let mut manifest_items = String::new();
        let mut spine_items = String::new();
        for i in 0..=MAX_CHAPTERS {
            manifest_items.push_str(&format!(
                "<item id=\"c{i}\" href=\"c{i}.xhtml\" media-type=\"application/xhtml+xml\"/>"
            ));
            spine_items.push_str(&format!("<itemref idref=\"c{i}\"/>"));
        }
        let opf_many = format!(
            "<package><manifest>{manifest_items}</manifest><spine>{spine_items}</spine></package>"
        );
        let mut entries: Vec<(String, String)> = vec![
            ("META-INF/container.xml".into(), container_ok.into()),
            ("content.opf".into(), opf_many),
        ];
        for i in 0..=MAX_CHAPTERS {
            entries.push((format!("c{i}.xhtml"), "<p>ok</p>".into()));
        }
        let refs: Vec<(&str, &str)> = entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let too_many_chapters = build_zip(&refs);
        assert!(
            parse_epub(&too_many_chapters)
                .unwrap_err()
                .contains("chapters")
        );
    }

    #[test]
    fn rejects_oversized_chapter_text_and_empty_input() {
        assert!(parse_epub(&[]).is_err());

        let container_ok =
            r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#;
        let opf = r#"<package><manifest><item id="c1" href="big.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
        let big_body = "a".repeat(MAX_CHAPTER_TEXT_BYTES + 1);
        let big_html = format!("<html><body><p>{big_body}</p></body></html>");
        let bytes = build_zip(&[
            ("META-INF/container.xml", container_ok),
            ("content.opf", opf),
            ("big.xhtml", &big_html),
        ]);
        let err = parse_epub(&bytes).unwrap_err();
        assert!(err.contains("budget"), "unexpected error: {err}");
    }
}
