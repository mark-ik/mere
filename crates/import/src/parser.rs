use super::*;
use serde_json::Value;

pub(super) fn parse_chrome_bookmark_json(
    contents: &str,
) -> Result<Vec<ImportedBookmarkItem>, BookmarkImportError> {
    let root: Value = serde_json::from_str(contents)
        .map_err(|error| BookmarkImportError::InvalidJson(error.to_string()))?;
    let mut items = Vec::new();
    let Some(roots) = root.get("roots") else {
        return Ok(items);
    };

    collect_chrome_root(
        roots.get("bookmark_bar"),
        BookmarkLocation::Toolbar,
        &mut Vec::new(),
        &mut items,
    );
    collect_chrome_root(
        roots.get("other"),
        BookmarkLocation::Other,
        &mut Vec::new(),
        &mut items,
    );
    collect_chrome_root(
        roots.get("synced"),
        BookmarkLocation::Other,
        &mut Vec::new(),
        &mut items,
    );

    Ok(items)
}

fn collect_chrome_root(
    node: Option<&Value>,
    location: BookmarkLocation,
    folder_path: &mut Vec<ImportedFolderSegment>,
    items: &mut Vec<ImportedBookmarkItem>,
) {
    let Some(node) = node else {
        return;
    };
    // The root folder itself (e.g. "Bookmarks Bar") is represented by the
    // `location` discriminant, so its label should not appear in folder_path.
    // Recurse into its children directly instead of pushing it as a folder.
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            collect_chrome_node(child, location, folder_path, items);
        }
    }
}

fn collect_chrome_node(
    node: &Value,
    location: BookmarkLocation,
    folder_path: &mut Vec<ImportedFolderSegment>,
    items: &mut Vec<ImportedBookmarkItem>,
) {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();
    match node_type {
        "url" => {
            let Some(raw_url) = node.get("url").and_then(Value::as_str) else {
                return;
            };
            if let Some(page) =
                build_page_seed(raw_url, node.get("name").and_then(Value::as_str), None)
            {
                items.push(ImportedBookmarkItem {
                    page,
                    bookmark_id: node.get("id").and_then(Value::as_str).map(str::to_string),
                    folder_path: folder_path.clone(),
                    location,
                    created_at_unix_secs: parse_chrome_timestamp(node.get("date_added")),
                    modified_at_unix_secs: parse_chrome_timestamp(node.get("date_last_used")),
                    tags: Vec::new(),
                });
            }
        }
        "folder" => {
            let label = normalize_optional_text(node.get("name").and_then(Value::as_str));
            let next_depth = folder_path.len();
            let stable_id = node.get("id").and_then(Value::as_str).map(str::to_string);
            if let Some(label) = label {
                folder_path.push(ImportedFolderSegment {
                    stable_id,
                    label,
                    position: next_depth,
                });
            }
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for child in children {
                    collect_chrome_node(child, location, folder_path, items);
                }
            }
            if node.get("name").and_then(Value::as_str).is_some() && !folder_path.is_empty() {
                folder_path.pop();
            }
        }
        _ => {
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for child in children {
                    collect_chrome_node(child, location, folder_path, items);
                }
            }
        }
    }
}

pub(super) fn parse_netscape_bookmark_html(contents: &str) -> Vec<ImportedBookmarkItem> {
    let mut items = Vec::new();
    let mut folder_path: Vec<ImportedFolderSegment> = Vec::new();
    let mut pending_folder_label: Option<String> = None;
    let mut dl_stack: Vec<bool> = Vec::new();

    let tokens = tokenize_html(contents);
    let mut index = 0usize;
    while index < tokens.len() {
        match &tokens[index] {
            HtmlToken::Tag(tag) if tag.is_opening("h3") => {
                let mut label = String::new();
                index += 1;
                while index < tokens.len() {
                    match &tokens[index] {
                        HtmlToken::Tag(next) if next.is_closing("h3") => break,
                        HtmlToken::Text(text) => label.push_str(text),
                        _ => {}
                    }
                    index += 1;
                }
                let decoded_label = decode_html_entities(&label);
                pending_folder_label = normalize_optional_text(Some(decoded_label.as_str()));
            }
            HtmlToken::Tag(tag) if tag.is_opening("dl") => {
                if let Some(label) = pending_folder_label.take() {
                    let depth = folder_path.len();
                    folder_path.push(ImportedFolderSegment {
                        stable_id: None,
                        label,
                        position: depth,
                    });
                    dl_stack.push(true);
                } else {
                    dl_stack.push(false);
                }
            }
            HtmlToken::Tag(tag) if tag.is_closing("dl") => {
                if dl_stack.pop().unwrap_or(false) {
                    folder_path.pop();
                }
            }
            HtmlToken::Tag(tag) if tag.is_opening("a") => {
                let Some(raw_url) = tag.attr("href") else {
                    index += 1;
                    continue;
                };
                let mut title = String::new();
                index += 1;
                while index < tokens.len() {
                    match &tokens[index] {
                        HtmlToken::Tag(next) if next.is_closing("a") => break,
                        HtmlToken::Text(text) => title.push_str(text),
                        _ => {}
                    }
                    index += 1;
                }
                let decoded_url = decode_html_entities(raw_url);
                let decoded_title = decode_html_entities(&title);
                if let Some(page) =
                    build_page_seed(&decoded_url, Some(decoded_title.as_str()), None)
                {
                    items.push(ImportedBookmarkItem {
                        page,
                        bookmark_id: None,
                        folder_path: folder_path.clone(),
                        location: BookmarkLocation::Unknown,
                        created_at_unix_secs: None,
                        modified_at_unix_secs: None,
                        tags: Vec::new(),
                    });
                }
            }
            _ => {}
        }
        index += 1;
    }

    items
}

fn build_page_seed(
    raw_url: &str,
    raw_title: Option<&str>,
    favicon_url: Option<&str>,
) -> Option<ImportedPageSeed> {
    let canonical_url = normalize_import_url(raw_url)?;
    let normalized_title = normalize_optional_text(raw_title);
    Some(ImportedPageSeed {
        canonical_url,
        normalized_title,
        raw_url: normalize_optional_text(Some(raw_url)),
        raw_title: normalize_optional_text(raw_title),
        favicon_url: normalize_optional_text(favicon_url),
    })
}

fn normalize_import_url(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    match url::Url::parse(trimmed) {
        Ok(parsed) => Some(parsed.to_string()),
        Err(_) => Some(trimmed.to_string()),
    }
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    let text = raw?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn parse_chrome_timestamp(value: Option<&Value>) -> Option<i64> {
    let raw = value?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    raw.parse::<i64>().ok().map(chrome_time_to_unix_secs)
}

fn chrome_time_to_unix_secs(chrome_micros: i64) -> i64 {
    (chrome_micros - 11_644_473_600_000_000) / 1_000_000
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HtmlToken {
    Tag(HtmlTag),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HtmlTag {
    name: String,
    raw: String,
    closing: bool,
}

impl HtmlTag {
    fn is_opening(&self, name: &str) -> bool {
        !self.closing && self.name.eq_ignore_ascii_case(name)
    }

    fn is_closing(&self, name: &str) -> bool {
        self.closing && self.name.eq_ignore_ascii_case(name)
    }

    fn attr(&self, name: &str) -> Option<&str> {
        extract_attr(&self.raw, name)
    }
}

fn tokenize_html(contents: &str) -> Vec<HtmlToken> {
    let mut tokens = Vec::new();
    let mut cursor = 0usize;
    while cursor < contents.len() {
        let rest = &contents[cursor..];
        let Some(start) = rest.find('<') else {
            let text = &contents[cursor..];
            if !text.is_empty() {
                tokens.push(HtmlToken::Text(text.to_string()));
            }
            break;
        };
        if start > 0 {
            tokens.push(HtmlToken::Text(rest[..start].to_string()));
        }
        let tag_start = cursor + start;
        let after_start = &contents[tag_start..];
        let Some(end) = after_start.find('>') else {
            break;
        };
        let tag_body = &contents[tag_start + 1..tag_start + end];
        let raw = tag_body.trim().to_string();
        if !raw.starts_with('!') {
            let closing = raw.starts_with('/');
            let name_source = raw.trim_start_matches('/').trim_start();
            let name = name_source
                .split(|character: char| character.is_whitespace() || character == '/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !name.is_empty() {
                tokens.push(HtmlToken::Tag(HtmlTag { name, raw, closing }));
            }
        }
        cursor = tag_start + end + 1;
    }
    tokens
}

fn extract_attr<'a>(raw_tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let mut rest = raw_tag;
    while !rest.is_empty() {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        rest = trimmed;
        let equals_index = rest.find('=')?;
        // The segment before `=` may include the tag name (on the first attribute)
        // or leading junk from prior parses; the attribute key is the last
        // whitespace-separated token.
        let key = rest[..equals_index]
            .trim()
            .split_whitespace()
            .next_back()
            .unwrap_or("");
        let value_start = equals_index + 1;
        let value_rest = rest[value_start..].trim_start();
        if let Some(stripped) = value_rest.strip_prefix('"') {
            let end_quote = stripped.find('"')?;
            if key.eq_ignore_ascii_case(attr_name) {
                return Some(&stripped[..end_quote]);
            }
            rest = &stripped[end_quote + 1..];
            continue;
        }
        if let Some(stripped) = value_rest.strip_prefix('\'') {
            let end_quote = stripped.find('\'')?;
            if key.eq_ignore_ascii_case(attr_name) {
                return Some(&stripped[..end_quote]);
            }
            rest = &stripped[end_quote + 1..];
            continue;
        }
        let end = value_rest
            .find(char::is_whitespace)
            .unwrap_or(value_rest.len());
        if key.eq_ignore_ascii_case(attr_name) {
            return Some(&value_rest[..end]);
        }
        rest = &value_rest[end..];
    }
    None
}

fn decode_html_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let entity_rest = &rest[start + 1..];
        if let Some(end) = entity_rest.find(';') {
            let entity = &entity_rest[..end];
            if let Some(decoded) = decode_html_entity(entity) {
                output.push(decoded);
                rest = &entity_rest[end + 1..];
                continue;
            }
        }
        output.push('&');
        rest = &rest[start + 1..];
    }
    output.push_str(rest);
    output
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        _ => {
            if let Some(value) = entity.strip_prefix("#x") {
                u32::from_str_radix(value, 16).ok().and_then(char::from_u32)
            } else if let Some(value) = entity.strip_prefix('#') {
                value.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    }
}
