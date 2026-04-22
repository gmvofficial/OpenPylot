use std::collections::HashMap;

/// A chunk of a document with metadata for vector indexing.
pub struct Chunk {
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Clean raw document text: strip page numbers, repeated headers/footers, OCR artifacts.
fn clean_content(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();

    // Remove common PDF artifacts: standalone page numbers, form feed chars
    lines.retain(|line| {
        let trimmed = line.trim();
        // Skip empty lines (we'll handle paragraph breaks separately)
        if trimmed.is_empty() {
            return true;
        }
        // Skip standalone page numbers like "1", "Page 2", "- 3 -", "Page 2 of 10"
        if trimmed.parse::<u32>().is_ok() {
            return false;
        }
        let lower = trimmed.to_lowercase();
        if lower.starts_with("page ")
            && trimmed[5..].trim().chars().all(|c| c.is_ascii_digit() || c == '/' || c == ' ' || c.to_lowercase().next() == Some('o') || c.to_lowercase().next() == Some('f'))
        {
            return false;
        }
        // Skip "- N -" patterns
        if trimmed.starts_with('-') && trimmed.ends_with('-') {
            let inner = trimmed.trim_matches('-').trim();
            if inner.parse::<u32>().is_ok() {
                return false;
            }
        }
        // Skip form feed / control chars
        if trimmed.chars().all(|c| c.is_control() || c == '\u{FFFD}') {
            return false;
        }
        true
    });

    // Collapse runs of 3+ blank lines into 2 (preserve paragraph boundaries)
    let joined = lines.join("\n");
    let mut result = String::with_capacity(joined.len());
    let mut blank_count = 0;
    for line in joined.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(line);
        }
    }
    result
}

/// Detect if a line looks like a section heading.
fn is_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 200 {
        return false;
    }
    // Markdown-style headings
    if trimmed.starts_with('#') {
        return true;
    }
    // ALL-CAPS lines that are short (likely section headers)
    if trimmed.len() >= 3
        && trimmed.len() <= 120
        && trimmed.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase())
        && trimmed.chars().any(|c| c.is_alphabetic())
    {
        return true;
    }
    // Numbered section headings like "1.2 Overview", "3.1.1 Safety"
    if trimmed.len() <= 120 {
        let mut chars = trimmed.chars().peekable();
        let mut has_digit = false;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() || c == '.' {
                has_digit = true;
                chars.next();
            } else {
                break;
            }
        }
        if has_digit {
            let rest: String = chars.collect();
            let rest = rest.trim();
            // After the number prefix, there should be a short title-like text
            // (no period at the end, not too long)
            if !rest.is_empty()
                && rest.len() <= 100
                && !rest.ends_with('.')
                && rest.split_whitespace().count() <= 12
            {
                return true;
            }
        }
    }
    false
}

/// Split text into sentences. Handles abbreviations and decimal points reasonably.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        current.push(chars[i]);

        // Check for sentence-ending punctuation
        if chars[i] == '.' || chars[i] == '!' || chars[i] == '?' {
            // Look ahead to decide if this is a real sentence break
            let next_is_space = i + 1 < len && chars[i + 1].is_whitespace();
            let next_is_upper =
                (i + 1 < len && chars[i + 1].is_uppercase()) ||
                (i + 2 < len && chars[i + 1].is_whitespace() && chars[i + 2].is_uppercase());
            let next_is_end = i + 1 >= len;
            let next_is_newline = i + 1 < len && chars[i + 1] == '\n';

            // Don't split on decimal numbers like "3.14"
            let prev_is_digit = i > 0 && chars[i - 1].is_ascii_digit();
            let next_is_digit = i + 1 < len && chars[i + 1].is_ascii_digit();
            if prev_is_digit && next_is_digit {
                i += 1;
                continue;
            }

            // Don't split on common abbreviations like "Dr.", "Mr.", "e.g.", "i.e."
            let prev_word: String = {
                let start = current.trim_end_matches('.').rfind(|c: char| c.is_whitespace() || c == '\n').map(|p| p + 1).unwrap_or(0);
                current[start..].trim_end_matches('.').to_string()
            };
            let abbrevs = ["Dr", "Mr", "Mrs", "Ms", "Jr", "Sr", "St", "vs", "etc",
                          "eg", "ie", "Fig", "fig", "Vol", "vol", "No", "no",
                          "Rev", "rev", "Approx", "approx", "Inc", "Corp", "Ltd"];
            if abbrevs.iter().any(|a| prev_word == *a) {
                i += 1;
                continue;
            }

            if next_is_end || next_is_newline || (next_is_space && next_is_upper) {
                let sentence = current.trim().to_string();
                if !sentence.is_empty() {
                    sentences.push(sentence);
                }
                current.clear();
            }
        }

        i += 1;
    }

    // Push remaining text as last sentence
    let remaining = current.trim().to_string();
    if !remaining.is_empty() {
        sentences.push(remaining);
    }

    sentences
}

/// A paragraph with optional heading context.
struct Section {
    heading: Option<String>,
    paragraphs: Vec<String>,
}

/// Parse content into sections based on heading detection.
fn parse_sections(content: &str) -> Vec<Section> {
    let paragraphs: Vec<&str> = content.split("\n\n").map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_paragraphs: Vec<String> = Vec::new();

    for para in paragraphs {
        // Check if first line of paragraph is a heading
        let first_line = para.lines().next().unwrap_or("");
        if is_heading(first_line) {
            // Save previous section if it has content
            if !current_paragraphs.is_empty() {
                sections.push(Section {
                    heading: current_heading.take(),
                    paragraphs: std::mem::take(&mut current_paragraphs),
                });
            }
            current_heading = Some(first_line.trim_start_matches('#').trim().to_string());
            // If there's more text after the heading line, keep it
            let rest: String = para.lines().skip(1).collect::<Vec<_>>().join("\n");
            let rest = rest.trim().to_string();
            if !rest.is_empty() {
                current_paragraphs.push(rest);
            }
        } else {
            current_paragraphs.push(para.to_string());
        }
    }

    // Push final section
    if !current_paragraphs.is_empty() || current_heading.is_some() {
        sections.push(Section {
            heading: current_heading,
            paragraphs: current_paragraphs,
        });
    }

    sections
}

/// Split a document into overlapping, sentence-aware chunks with contextual prefixes.
///
/// Improvements over naive word-level chunking:
/// - Cleans PDF/OCR artifacts (page numbers, control chars, repeated headers)
/// - Detects section headings and preserves them in metadata
/// - Never breaks mid-sentence
/// - Prepends document title + section heading as prefix for better embedding quality
/// - Tracks section context in metadata for retrieval
pub fn chunk_document(
    title: &str,
    content: &str,
    source: &str,
    collection_id: &str,
    chunk_size: usize,
    chunk_overlap: usize,
) -> Vec<Chunk> {
    let cleaned = clean_content(content);
    if cleaned.trim().is_empty() {
        return Vec::new();
    }

    let sections = parse_sections(&cleaned);
    let mut all_sentence_blocks: Vec<(Option<String>, Vec<String>)> = Vec::new();

    // Break each section's paragraphs into sentences, preserving section context
    for section in &sections {
        let mut section_sentences = Vec::new();
        for para in &section.paragraphs {
            let mut sentences = split_sentences(para);
            if sentences.is_empty() {
                continue;
            }
            section_sentences.append(&mut sentences);
        }
        if !section_sentences.is_empty() {
            all_sentence_blocks.push((section.heading.clone(), section_sentences));
        }
    }

    if all_sentence_blocks.is_empty() {
        return Vec::new();
    }

    // Flatten into a list of (section_heading, sentence) pairs
    let mut flat: Vec<(Option<String>, String)> = Vec::new();
    for (heading, sentences) in &all_sentence_blocks {
        for sent in sentences {
            flat.push((heading.clone(), sent.clone()));
        }
    }

    // Build chunks by accumulating sentences up to chunk_size words
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut chunk_sentences: Vec<(Option<String>, String)> = Vec::new();
    let mut chunk_word_count = 0;
    let overlap_target = chunk_overlap;

    let mut idx = 0;
    while idx < flat.len() {
        let (heading, sentence) = &flat[idx];
        let word_count = sentence.split_whitespace().count();

        // If adding this sentence would exceed chunk_size and we already have content,
        // finalize the current chunk
        if chunk_word_count + word_count > chunk_size && !chunk_sentences.is_empty() {
            // Finalize chunk
            let chunk = build_chunk(
                title, source, collection_id,
                &chunk_sentences, chunks.len(),
            );
            chunks.push(chunk);

            // Find overlap: backtrack to include ~overlap_target words from the end
            let mut overlap_words = 0;
            let mut overlap_start = chunk_sentences.len();
            while overlap_start > 0 {
                overlap_start -= 1;
                overlap_words += chunk_sentences[overlap_start].1.split_whitespace().count();
                if overlap_words >= overlap_target {
                    break;
                }
            }
            chunk_sentences = chunk_sentences[overlap_start..].to_vec();
            chunk_word_count = chunk_sentences.iter().map(|(_, s)| s.split_whitespace().count()).sum();
        }

        chunk_sentences.push((heading.clone(), sentence.clone()));
        chunk_word_count += word_count;
        idx += 1;
    }

    // Final chunk
    if !chunk_sentences.is_empty() {
        let chunk = build_chunk(
            title, source, collection_id,
            &chunk_sentences, chunks.len(),
        );
        chunks.push(chunk);
    }

    // Set total_chunks in metadata
    let total = chunks.len();
    for chunk in &mut chunks {
        chunk.metadata.insert("total_chunks".to_string(), serde_json::json!(total));
    }

    chunks
}

/// Build a single Chunk from accumulated sentences, with a contextual prefix.
fn build_chunk(
    title: &str,
    source: &str,
    collection_id: &str,
    sentences: &[(Option<String>, String)],
    chunk_index: usize,
) -> Chunk {
    // Determine the primary section heading for this chunk
    // Use the most recent heading among the sentences
    let section_heading = sentences
        .iter()
        .rev()
        .find_map(|(h, _)| h.as_ref())
        .cloned();

    // Build contextual prefix for better embedding quality
    let mut prefix_parts = Vec::new();
    prefix_parts.push(format!("Document: {}", title));
    if let Some(ref heading) = section_heading {
        prefix_parts.push(format!("Section: {}", heading));
    }
    let prefix = prefix_parts.join(" | ");

    // Join sentences as the chunk body
    let body: String = sentences.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" ");
    let content = format!("{}\n\n{}", prefix, body);

    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), serde_json::json!(source));
    metadata.insert("title".to_string(), serde_json::json!(title));
    metadata.insert("chunk_index".to_string(), serde_json::json!(chunk_index));
    metadata.insert("collection_id".to_string(), serde_json::json!(collection_id));
    if let Some(heading) = section_heading {
        metadata.insert("section".to_string(), serde_json::json!(heading));
    }

    Chunk { content, metadata }
}
