//! Document loading and processing functionality
//!
//! This module provides utilities for loading and extracting content from various
//! document formats including PDF, TXT, Word, JSON, CSV, XML, HTML, and Excel.

use anyhow::{Context, Result};
use async_trait::async_trait;
use calamine::Data;
use csv::ReaderBuilder;
use quick_xml::events::Event;
use quick_xml::Reader;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write;
use std::io::Cursor;
use std::path::Path;

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Document loader configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentLoaderConfig {
    /// Maximum file size to process (in bytes)
    pub max_file_size: usize,
    /// Character encoding for text files
    pub default_encoding: String,
    /// Whether to preserve formatting
    pub preserve_formatting: bool,
    /// Enable OCR for scanned PDFs and images (requires tesseract)
    pub enable_ocr: bool,
    /// OCR language (e.g., "eng" for English, "eng+fra" for multiple)
    pub ocr_language: String,
}

impl Default for DocumentLoaderConfig {
    fn default() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024, // 10MB
            default_encoding: "utf-8".to_string(),
            preserve_formatting: false,
            enable_ocr: cfg!(feature = "ocr"), // Auto-enable if OCR feature is compiled
            ocr_language: "eng".to_string(),
        }
    }
}

/// Loaded document content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentContent {
    /// Original file path or URL
    pub source: String,
    /// Document type
    pub document_type: String,
    /// Extracted text content
    pub content: String,
    /// Document metadata
    pub metadata: HashMap<String, Value>,
    /// File size in bytes
    pub file_size: usize,
    /// Extraction timestamp
    pub extracted_at: String,
}

/// Document loader for processing various file formats
pub struct DocumentLoader {
    config: DocumentLoaderConfig,
}

impl DocumentLoader {
    /// Create a new document loader with default configuration
    pub fn new() -> Self {
        Self {
            config: DocumentLoaderConfig::default(),
        }
    }

    /// Create a new document loader with custom configuration
    pub fn with_config(config: DocumentLoaderConfig) -> Self {
        Self { config }
    }

    /// Load and extract content from a document
    pub async fn load_document(
        &self,
        source_path: &str,
        document_type: &str,
    ) -> Result<DocumentContent> {
        // Validate document type
        let supported_types = [
            "pdf", "txt", "md", "docx", "json", "csv", "xml", "html", "xlsb", "xlsx", "xls",
        ];
        if !supported_types.contains(&document_type.to_lowercase().as_str()) {
            anyhow::bail!(
                "Unsupported document type: {document_type}. Supported: {:?}",
                supported_types
            );
        }

        // Check if source is a URL or file path
        let content = if source_path.starts_with("http://") || source_path.starts_with("https://") {
            self.load_from_url(source_path, document_type).await?
        } else if source_path.contains("://") {
            anyhow::bail!(
                "Invalid URL format: {source_path}. Only HTTP and HTTPS URLs are supported"
            );
        } else {
            self.load_from_file(source_path, document_type).await?
        };

        Ok(content)
    }

    /// Extract content from file bytes (for API file uploads)
    pub async fn extract_from_bytes(
        &self,
        bytes: &[u8],
        filename: &str,
        document_type: &str,
    ) -> Result<DocumentContent> {
        // Check file size
        if bytes.len() > self.config.max_file_size {
            anyhow::bail!(
                "File size ({} bytes) exceeds maximum allowed size ({} bytes)",
                bytes.len(),
                self.config.max_file_size
            );
        }

        // Extract content based on document type
        let content = match document_type.to_lowercase().as_str() {
            "txt" | "md" => {
                let text = String::from_utf8(bytes.to_vec())
                    .context("Failed to decode text content as UTF-8")?;
                format!("{}: {}\n\n{}", document_type.to_uppercase(), filename, text)
            }
            "pdf" => Self::extract_pdf_from_bytes_internal(bytes, Some(filename)).await?,
            "docx" => {
                let text = Self::extract_docx_from_bytes(bytes).await?;
                format!("DOCX: {}\n\n{}", filename, text)
            }
            "json" => {
                let text =
                    String::from_utf8(bytes.to_vec()).context("Failed to decode JSON as UTF-8")?;
                let formatted = Self::validate_and_format_json(&text)?;
                format!("JSON: {}\n\n{}", filename, formatted)
            }
            "csv" => {
                let text =
                    String::from_utf8(bytes.to_vec()).context("Failed to decode CSV as UTF-8")?;
                let formatted = Self::parse_csv_to_structured_text(&text)?;
                format!("CSV: {}\n\n{}", filename, formatted)
            }
            "xml" => {
                let text =
                    String::from_utf8(bytes.to_vec()).context("Failed to decode XML as UTF-8")?;
                let formatted = Self::parse_xml_to_structured_text(&text)?;
                format!("XML: {}\n\n{}", filename, formatted)
            }
            "html" => {
                let text =
                    String::from_utf8(bytes.to_vec()).context("Failed to decode HTML as UTF-8")?;
                let formatted = Self::parse_html_to_structured_text(&text)?;
                format!("HTML: {}\n\n{}", filename, formatted)
            }
            "xlsb" | "xlsx" | "xls" => {
                let text = Self::extract_excel_from_bytes(bytes).await?;
                format!(
                    "Excel ({}): {}\n\n{}",
                    document_type.to_uppercase(),
                    filename,
                    text
                )
            }
            // Image formats (requires OCR feature)
            #[cfg(feature = "ocr")]
            "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff" | "tif" | "gif" => {
                Self::extract_image_with_ocr_internal(bytes, document_type, Some(filename)).await?
            }
            #[cfg(not(feature = "ocr"))]
            "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff" | "tif" | "gif" => {
                anyhow::bail!(
                    "Image OCR is not enabled. To extract text from images, \
                    compile with --features ocr and install Tesseract OCR"
                );
            }
            _ => {
                anyhow::bail!("Unsupported document type: {document_type}");
            }
        };

        let mut metadata = HashMap::new();
        metadata.insert("file_size".to_string(), json!(bytes.len()));
        metadata.insert("filename".to_string(), json!(filename));

        Ok(DocumentContent {
            source: filename.to_string(),
            document_type: document_type.to_string(),
            content,
            metadata,
            file_size: bytes.len(),
            extracted_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Load document from file path
    async fn load_from_file(
        &self,
        file_path: &str,
        document_type: &str,
    ) -> Result<DocumentContent> {
        let path = Path::new(file_path);

        // Check if file exists
        if !path.exists() {
            anyhow::bail!("File not found: {file_path}");
        }

        // Check file size
        let metadata = tokio::fs::metadata(path)
            .await
            .context("Failed to read file metadata")?;

        let file_size = metadata.len() as usize;
        if file_size > self.config.max_file_size {
            anyhow::bail!(
                "File size ({file_size} bytes) exceeds maximum allowed size ({} bytes)",
                self.config.max_file_size
            );
        }

        // Extract content based on document type
        let content = match document_type.to_lowercase().as_str() {
            "txt" | "md" => Self::extract_text_content(file_path).await?,
            "pdf" => Self::extract_pdf_content(file_path).await?,
            "docx" => Self::extract_docx_content(file_path).await?,
            "json" => Self::extract_json_content(file_path).await?,
            "csv" => Self::extract_csv_content(file_path).await?,
            "xml" => Self::extract_xml_content(file_path).await?,
            "html" => Self::extract_html_content(file_path).await?,
            "xlsb" | "xlsx" | "xls" => Self::extract_excel_content(file_path).await?,
            _ => {
                anyhow::bail!("Unsupported document type: {document_type}");
            }
        };

        let mut doc_metadata = HashMap::new();
        doc_metadata.insert("file_size".to_string(), json!(file_size));
        doc_metadata.insert("file_path".to_string(), json!(file_path));

        Ok(DocumentContent {
            source: file_path.to_string(),
            document_type: document_type.to_string(),
            content,
            metadata: doc_metadata,
            file_size,
            extracted_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Load document from URL
    async fn load_from_url(&self, url: &str, document_type: &str) -> Result<DocumentContent> {
        // Create HTTP client with timeout and user agent
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("GMV-Agent Document Loader/1.0")
            .build()
            .context("Failed to create HTTP client")?;

        // Fetch the document
        let response = client
            .get(url)
            .send()
            .await
            .context(format!("Failed to fetch URL: {url}"))?;

        // Check response status
        if !response.status().is_success() {
            anyhow::bail!("HTTP error {}: {url}", response.status());
        }

        // Check content length
        if let Some(content_length) = response.content_length() {
            if content_length as usize > self.config.max_file_size {
                anyhow::bail!(
                    "Remote file size ({content_length} bytes) exceeds maximum ({} bytes)",
                    self.config.max_file_size
                );
            }
        }

        // Get content type from response headers
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Download the content
        let content_bytes = response
            .bytes()
            .await
            .context("Failed to read response body")?;

        // Check actual size
        if content_bytes.len() > self.config.max_file_size {
            anyhow::bail!(
                "Downloaded file size ({} bytes) exceeds maximum ({} bytes)",
                content_bytes.len(),
                self.config.max_file_size
            );
        }

        // Process content based on type
        let content = match document_type.to_lowercase().as_str() {
            "txt" | "md" | "json" | "csv" | "xml" | "html" => {
                let text = String::from_utf8(content_bytes.to_vec())
                    .context("Failed to decode text content")?;

                match document_type.to_lowercase().as_str() {
                    "json" => Self::validate_and_format_json(&text)?,
                    "csv" => Self::parse_csv_to_structured_text(&text)?,
                    "xml" => Self::parse_xml_to_structured_text(&text)?,
                    "html" => Self::parse_html_to_structured_text(&text)?,
                    _ => text,
                }
            }
            "pdf" => Self::extract_pdf_from_bytes(&content_bytes).await?,
            "docx" => Self::extract_docx_from_bytes(&content_bytes).await?,
            "xlsb" | "xlsx" | "xls" => Self::extract_excel_from_bytes(&content_bytes).await?,
            _ => {
                anyhow::bail!("Unsupported document type for URL loading: {document_type}");
            }
        };

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("file_size".to_string(), json!(content_bytes.len()));
        metadata.insert("url".to_string(), json!(url));
        metadata.insert("content_type".to_string(), json!(content_type));

        Ok(DocumentContent {
            source: url.to_string(),
            document_type: document_type.to_string(),
            content,
            metadata,
            file_size: content_bytes.len(),
            extracted_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Extract content from plain text files
    async fn extract_text_content(file_path: &str) -> Result<String> {
        tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read text file")
    }

    /// Extract content from JSON files
    async fn extract_json_content(file_path: &str) -> Result<String> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read JSON file")?;
        Self::validate_and_format_json(&content)
    }

    /// Validate and format JSON content
    fn validate_and_format_json(content: &str) -> Result<String> {
        let json_value: Value = serde_json::from_str(content).context("Invalid JSON content")?;
        serde_json::to_string_pretty(&json_value).context("Failed to format JSON")
    }

    /// Extract content from CSV files
    async fn extract_csv_content(file_path: &str) -> Result<String> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read CSV file")?;
        Self::parse_csv_to_structured_text(&content)
    }

    /// Parse CSV content into structured, readable text format
    fn parse_csv_to_structured_text(csv_content: &str) -> Result<String> {
        let mut reader = ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_reader(Cursor::new(csv_content));

        let mut result = String::new();

        // Get headers
        let headers = reader
            .headers()
            .context("Failed to read CSV headers")?
            .clone();
        let header_count = headers.len();

        result.push_str("CSV Document Content:\n");
        writeln!(
            result,
            "Columns ({}): {}\n",
            header_count,
            headers.iter().collect::<Vec<_>>().join(", ")
        )
        .ok();

        // Process records
        let mut row_count = 0;
        for (index, record) in reader.records().enumerate() {
            let record = record.context("Failed to read CSV record")?;
            row_count += 1;

            writeln!(result, "Row {}:", index + 1).ok();

            for (i, field) in record.iter().enumerate() {
                if i < header_count {
                    let header = headers.get(i).unwrap_or("Unknown");
                    writeln!(result, "  {}: {}", header, field.trim()).ok();
                }
            }
            result.push('\n');

            // Limit output for very large CSV files
            if row_count >= 100 {
                writeln!(result, "... (truncated at 100 rows for readability)").ok();
                break;
            }
        }

        writeln!(result, "Total rows processed: {}", row_count).ok();
        Ok(result)
    }

    /// Extract content from XML files
    async fn extract_xml_content(file_path: &str) -> Result<String> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read XML file")?;
        Self::parse_xml_to_structured_text(&content)
    }

    /// Parse XML content into structured, readable text format
    fn parse_xml_to_structured_text(xml_content: &str) -> Result<String> {
        let mut reader = Reader::from_str(xml_content);
        reader.config_mut().trim_text(true);

        let mut result = String::new();
        let mut buf = Vec::new();
        let mut current_path = Vec::new();
        let mut text_content = Vec::new();

        result.push_str("XML Document Content:\n\n");

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let name = std::str::from_utf8(e.name().as_ref())
                        .context("Invalid UTF-8 in XML element name")?
                        .to_string();
                    current_path.push(name.clone());

                    let indent = "  ".repeat(current_path.len().saturating_sub(1));
                    writeln!(result, "{}Element: {}", indent, name).ok();

                    for attr in e.attributes() {
                        let attr = attr.context("Failed to read XML attribute")?;
                        let key = std::str::from_utf8(attr.key.as_ref())
                            .context("Invalid UTF-8 in attribute key")?;
                        let value = std::str::from_utf8(&attr.value)
                            .context("Invalid UTF-8 in attribute value")?;
                        writeln!(result, "{}  @{}: {}", indent, key, value).ok();
                    }
                }
                Ok(Event::End(_)) => {
                    if !text_content.is_empty() {
                        let content = text_content.join(" ").trim().to_string();
                        if !content.is_empty() {
                            let indent = "  ".repeat(current_path.len());
                            writeln!(result, "{}Text: {}", indent, content).ok();
                        }
                        text_content.clear();
                    }
                    current_path.pop();
                }
                Ok(Event::Text(e)) => {
                    let text = e.unescape().context("Failed to unescape XML text")?;
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        text_content.push(trimmed);
                    }
                }
                Ok(Event::CData(e)) => {
                    let text = std::str::from_utf8(&e).context("Invalid UTF-8 in CDATA")?;
                    if !text.trim().is_empty() {
                        text_content.push(text.to_string());
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => anyhow::bail!("Error parsing XML: {}", e),
                _ => {}
            }
            buf.clear();
        }

        result.push_str("\nXML parsing completed.\n");
        Ok(result)
    }

    /// Extract content from HTML files
    async fn extract_html_content(file_path: &str) -> Result<String> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read HTML file")?;
        Self::parse_html_to_structured_text(&content)
    }

    /// Parse HTML content into structured, readable text format
    fn parse_html_to_structured_text(html_content: &str) -> Result<String> {
        let document = Html::parse_document(html_content);
        let mut result = String::new();

        result.push_str("HTML Document Content:\n\n");

        // Extract title
        if let Ok(title_selector) = Selector::parse("title") {
            if let Some(title) = document.select(&title_selector).next() {
                writeln!(
                    result,
                    "Title: {}\n",
                    title.text().collect::<String>().trim()
                )
                .ok();
            }
        }

        // Extract meta description
        if let Ok(meta_selector) = Selector::parse("meta[name='description']") {
            if let Some(meta) = document.select(&meta_selector).next() {
                if let Some(content) = meta.value().attr("content") {
                    writeln!(result, "Description: {}\n", content.trim()).ok();
                }
            }
        }

        // Extract headings with hierarchy
        for level in 1..=6 {
            let selector_str = format!("h{}", level);
            if let Ok(heading_selector) = Selector::parse(&selector_str) {
                for heading in document.select(&heading_selector) {
                    let text = heading.text().collect::<String>().trim().to_string();
                    if !text.is_empty() {
                        let indent = "  ".repeat(level - 1);
                        writeln!(result, "{}H{}: {}", indent, level, text).ok();
                    }
                }
            }; // Added semicolon to make this an expression statement
        }

        // Extract paragraphs
        if let Ok(p_selector) = Selector::parse("p") {
            result.push_str("\nParagraphs:\n");
            for paragraph in document.select(&p_selector) {
                let text = paragraph.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    writeln!(result, "  {}\n", text).ok();
                }
            }
        }

        // Extract lists
        if let Ok(ul_selector) = Selector::parse("ul, ol") {
            result.push_str("Lists:\n");
            for list in document.select(&ul_selector) {
                let list_type = list.value().name();
                writeln!(
                    result,
                    "  {} List:",
                    if list_type == "ul" {
                        "Unordered"
                    } else {
                        "Ordered"
                    }
                )
                .ok();

                if let Ok(li_selector) = Selector::parse("li") {
                    for (index, item) in list.select(&li_selector).enumerate() {
                        let text = item.text().collect::<String>().trim().to_string();
                        if !text.is_empty() {
                            let prefix = if list_type == "ul" {
                                "•".to_string()
                            } else {
                                format!("{}.", index + 1)
                            };
                            writeln!(result, "    {} {}", prefix, text).ok();
                        }
                    }
                }
                result.push('\n');
            }
        }

        // Extract links
        if let Ok(a_selector) = Selector::parse("a[href]") {
            result.push_str("Links:\n");
            for link in document.select(&a_selector) {
                let text = link.text().collect::<String>().trim().to_string();
                if let Some(href) = link.value().attr("href") {
                    if !text.is_empty() && !href.is_empty() {
                        writeln!(result, "  {} -> {}", text, href).ok();
                    }
                }
            }
            result.push('\n');
        }

        result.push_str("HTML parsing completed.\n");
        Ok(result)
    }

    /// Extract content from PDF files
    async fn extract_pdf_content(file_path: &str) -> Result<String> {
        let bytes = tokio::fs::read(file_path)
            .await
            .context("Failed to read PDF file")?;
        Self::extract_pdf_from_bytes(&bytes).await
    }

    /// Extract content from DOCX files
    async fn extract_docx_content(file_path: &str) -> Result<String> {
        let buffer = tokio::fs::read(file_path)
            .await
            .context("Failed to read DOCX file")?;
        Self::extract_docx_from_bytes(&buffer).await
    }

    /// Extract DOCX content from bytes
    async fn extract_docx_from_bytes(buffer: &[u8]) -> Result<String> {
        let docx = docx_rs::read_docx(buffer).context("Failed to parse DOCX file")?;

        let mut text_content = String::new();

        for child in &docx.document.children {
            if let docx_rs::DocumentChild::Paragraph(paragraph) = child {
                for para_child in &paragraph.children {
                    if let docx_rs::ParagraphChild::Run(run_element) = para_child {
                        for run_child in &run_element.children {
                            if let docx_rs::RunChild::Text(text) = run_child {
                                text_content.push_str(&text.text);
                            }
                        }
                    }
                }
                text_content.push('\n');
            }
        }

        if text_content.trim().is_empty() {
            anyhow::bail!("No text content could be extracted from the DOCX file");
        }

        Ok(text_content.trim().to_string())
    }

    /// Extract content from Excel files
    async fn extract_excel_content(file_path: &str) -> Result<String> {
        use calamine::{open_workbook_auto, Reader};

        let mut workbook = open_workbook_auto(file_path)
            .context(format!("Failed to open Excel file: {}", file_path))?;

        let mut result = String::new();
        result.push_str("Excel Document Content:\n\n");

        let sheet_names = workbook.sheet_names().to_vec();
        for sheet_name in sheet_names {
            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                writeln!(result, "Sheet: {}", sheet_name).ok();
                result.push_str(&"-".repeat(sheet_name.len() + 7));
                result.push('\n');

                for row in range.rows() {
                    let row_str = row
                        .iter()
                        .map(|cell| match cell {
                            Data::Empty => "".to_string(),
                            Data::String(s) => s.clone(),
                            Data::Float(f) => f.to_string(),
                            Data::Int(i) => i.to_string(),
                            Data::Bool(b) => b.to_string(),
                            Data::DateTime(d) => d.to_string(),
                            Data::Error(e) => format!("Error({:?})", e),
                            _ => "".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(" | ");

                    if !row_str.trim().is_empty() {
                        writeln!(result, "{}", row_str).ok();
                    }
                }
                result.push('\n');
            }
        }

        if result.trim() == "Excel Document Content:" {
            anyhow::bail!("No content could be extracted from the Excel file");
        }

        Ok(result.trim().to_string())
    }

    /// Extract Excel content from bytes
    async fn extract_excel_from_bytes(bytes: &[u8]) -> Result<String> {
        use calamine::{open_workbook_auto_from_rs, Reader};
        use std::io::Cursor;

        let cursor = Cursor::new(bytes);
        let mut workbook =
            open_workbook_auto_from_rs(cursor).context("Failed to open Excel file from bytes")?;

        let mut result = String::new();
        result.push_str("Excel Document Content:\n\n");

        let sheet_names = workbook.sheet_names().to_vec();
        for sheet_name in sheet_names {
            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                writeln!(result, "Sheet: {}", sheet_name).ok();
                result.push_str(&"-".repeat(sheet_name.len() + 7));
                result.push('\n');

                for row in range.rows() {
                    let row_str = row
                        .iter()
                        .map(|cell| match cell {
                            Data::Empty => "".to_string(),
                            Data::String(s) => s.clone(),
                            Data::Float(f) => f.to_string(),
                            Data::Int(i) => i.to_string(),
                            Data::Bool(b) => b.to_string(),
                            Data::DateTime(d) => d.to_string(),
                            Data::Error(e) => format!("Error({:?})", e),
                            _ => "".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(" | ");

                    if !row_str.trim().is_empty() {
                        writeln!(result, "{}", row_str).ok();
                    }
                }
                result.push('\n');
            }
        }

        if result.trim() == "Excel Document Content:" {
            anyhow::bail!("No content could be extracted from the Excel file");
        }

        Ok(result.trim().to_string())
    }

    /// Get supported document types
    pub fn supported_types() -> Vec<&'static str> {
        let types = vec![
            "txt", "md", "pdf", "docx", "json", "csv", "xml", "html", "xlsb", "xlsx", "xls",
        ];

        // Add image types if OCR is enabled
        #[cfg(feature = "ocr")]
        {
            let mut types_with_images = types;
            types_with_images
                .extend_from_slice(&["png", "jpg", "jpeg", "webp", "bmp", "tiff", "gif"]);
            return types_with_images;
        }

        #[cfg(not(feature = "ocr"))]
        types
    }

    /// Extract content from PDF files with OCR fallback
    async fn extract_pdf_from_bytes(bytes: &[u8]) -> Result<String> {
        Self::extract_pdf_from_bytes_internal(bytes, None).await
    }

    /// Extract content from PDF files with OCR fallback and optional filename
    async fn extract_pdf_from_bytes_internal(
        bytes: &[u8],
        filename: Option<&str>,
    ) -> Result<String> {
        // Try standard PDF text extraction first
        match pdf_extract::extract_text_from_mem(bytes) {
            Ok(text) if !text.trim().is_empty() => {
                let text_len = text.trim().len();
                let file_size = bytes.len();

                // Check if extracted text is reasonable compared to file size
                // If we extracted less than 0.1% of file size (or less than 100 bytes for large files),
                // it's probably just metadata and we should try OCR
                let min_expected = if file_size > 100000 {
                    100
                } else {
                    file_size / 1000
                };

                if text_len >= min_expected {
                    tracing::info!(
                        "PDF text extracted successfully using standard method ({} bytes)",
                        text_len
                    );
                    let result = if let Some(fname) = filename {
                        format!("PDF: {}\n\n{}", fname, text.trim())
                    } else {
                        text.trim().to_string()
                    };
                    return Ok(result);
                } else {
                    tracing::warn!(
                        "PDF text extraction returned only {} bytes from {} byte file ({}%), likely scanned - attempting OCR",
                        text_len,
                        file_size,
                        (text_len as f64 / file_size as f64 * 100.0) as u32
                    );
                }
            }
            Ok(_) => {
                tracing::warn!("PDF contains no extractable text, attempting OCR fallback");
            }
            Err(e) => {
                tracing::warn!(
                    "Standard PDF extraction failed: {}, attempting OCR fallback",
                    e
                );
            }
        }

        // Fallback to OCR if standard extraction failed or returned empty
        #[cfg(feature = "ocr")]
        {
            tracing::info!("Attempting OCR on PDF...");
            return Self::extract_pdf_with_ocr(bytes, filename).await;
        }

        #[cfg(not(feature = "ocr"))]
        {
            anyhow::bail!(
                "No text content could be extracted from the PDF. \
                This may be a scanned PDF. To enable OCR support, \
                compile with --features ocr and install Tesseract OCR"
            );
        }
    }

    /// Extract text from images using OCR
    #[cfg(feature = "ocr")]
    async fn extract_image_with_ocr(bytes: &[u8], image_type: &str) -> Result<String> {
        Self::extract_image_with_ocr_internal(bytes, image_type, None).await
    }

    /// Extract text from images using OCR with optional filename
    #[cfg(feature = "ocr")]
    async fn extract_image_with_ocr_internal(
        bytes: &[u8],
        image_type: &str,
        filename: Option<&str>,
    ) -> Result<String> {
        use image::ImageFormat;

        tracing::info!("Loading image for OCR: type={}", image_type);

        // Determine image format
        let format = match image_type.to_lowercase().as_str() {
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "webp" => ImageFormat::WebP,
            "bmp" => ImageFormat::Bmp,
            "tiff" | "tif" => ImageFormat::Tiff,
            "gif" => ImageFormat::Gif,
            _ => anyhow::bail!("Unsupported image format: {}", image_type),
        };

        // Load image
        let img =
            image::load_from_memory_with_format(bytes, format).context("Failed to load image")?;

        // Convert to RGB8 for Tesseract
        let rgb_img = img.to_rgb8();
        let (width, height) = rgb_img.dimensions();

        // Initialize Tesseract
        let mut tesseract = tesseract::Tesseract::new(None, Some("eng"))
            .context("Failed to initialize Tesseract. Is Tesseract OCR installed?")?;

        // Set image with proper dimensions and channels using set_frame
        tesseract = tesseract
            .set_frame(
                rgb_img.as_raw(),
                width as i32,
                height as i32,
                3,                  // 3 bytes per pixel for RGB
                (width * 3) as i32, // bytes per line
            )
            .context("Failed to set image in Tesseract")?;

        // Extract text
        let text = tesseract
            .get_text()
            .context("Failed to extract text from image")?;

        if text.trim().is_empty() {
            anyhow::bail!("No text could be extracted from the image");
        }

        tracing::info!("OCR extracted {} bytes of text from image", text.len());

        // Format output with filename header if provided
        let result = if let Some(fname) = filename {
            format!("Image: {}\nExtracted Text:\n\n{}", fname, text.trim())
        } else {
            text.trim().to_string()
        };

        Ok(result)
    }

    /// Extract text from scanned PDF using OCR
    #[cfg(feature = "ocr")]
    async fn extract_pdf_with_ocr(pdf_bytes: &[u8], filename: Option<&str>) -> Result<String> {
        use std::io::Write;
        use std::process::Command;
        use tempfile::NamedTempFile;

        tracing::info!("Attempting PDF to image conversion for OCR...");

        // Create temporary files
        let mut temp_pdf = NamedTempFile::new().context("Failed to create temp PDF file")?;
        temp_pdf
            .write_all(pdf_bytes)
            .context("Failed to write PDF data")?;
        let pdf_path = temp_pdf.path();

        // Create temporary directory for output images
        let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
        let output_prefix = temp_dir.path().join("page");

        // Convert PDF to images using pdftoppm
        tracing::info!("Converting PDF to images using pdftoppm...");
        let output = Command::new("pdftoppm")
            .arg("-png")
            .arg(pdf_path)
            .arg(&output_prefix)
            .output();

        match output {
            Ok(result) if result.status.success() => {
                // Find all generated PNG files
                let entries =
                    std::fs::read_dir(temp_dir.path()).context("Failed to read temp directory")?;

                let mut png_files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s == "png")
                            .unwrap_or(false)
                    })
                    .collect();

                // Sort by filename to maintain page order
                png_files.sort_by_key(|e| e.path());

                if png_files.is_empty() {
                    anyhow::bail!("PDF conversion produced no images");
                }

                tracing::info!(
                    "Converted PDF to {} page(s), running OCR...",
                    png_files.len()
                );

                // OCR each page
                let mut all_text = String::new();
                for (i, entry) in png_files.iter().enumerate() {
                    let png_bytes = std::fs::read(entry.path())
                        .context(format!("Failed to read page {}", i + 1))?;

                    match Self::extract_image_with_ocr(&png_bytes, "png").await {
                        Ok(text) => {
                            if !text.trim().is_empty() {
                                if i > 0 {
                                    all_text.push_str("\n\n--- Page ");
                                    all_text.push_str(&(i + 1).to_string());
                                    all_text.push_str(" ---\n\n");
                                }
                                all_text.push_str(&text);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to OCR page {}: {}", i + 1, e);
                        }
                    }
                }

                if all_text.trim().is_empty() {
                    anyhow::bail!("OCR could not extract any text from the PDF pages");
                }

                tracing::info!(
                    "Successfully extracted {} bytes from {} pages",
                    all_text.len(),
                    png_files.len()
                );

                // Format output with filename header if provided
                let result = if let Some(fname) = filename {
                    format!("Scanned PDF: {}\n\n{}", fname, all_text.trim())
                } else {
                    all_text.trim().to_string()
                };

                Ok(result)
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                tracing::error!("pdftoppm failed: {}", stderr);
                anyhow::bail!(
                    "PDF to image conversion failed. Is poppler installed?\n\
                    Install: brew install poppler (macOS) or apt-get install poppler-utils (Linux)"
                )
            }
            Err(e) => {
                tracing::error!("Failed to run pdftoppm: {}", e);
                anyhow::bail!(
                    "Could not run pdftoppm command. Is poppler installed?\n\
                    Install: brew install poppler (macOS) or apt-get install poppler-utils (Linux)"
                )
            }
        }
    }
}

impl Default for DocumentLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Document Loader Tool for LLM integration
pub struct DocumentLoaderTool {
    loader: DocumentLoader,
}

impl DocumentLoaderTool {
    pub fn new() -> Self {
        Self {
            loader: DocumentLoader::new(),
        }
    }

    pub fn with_config(config: DocumentLoaderConfig) -> Self {
        Self {
            loader: DocumentLoader::with_config(config),
        }
    }

    /// Get the underlying loader (for API use)
    pub fn loader(&self) -> &DocumentLoader {
        &self.loader
    }
}

impl Default for DocumentLoaderTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for DocumentLoaderTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "load_document".to_string(),
            description: "Load and extract text content from various document formats (PDF, DOCX, TXT, JSON, CSV, XML, HTML, Excel). Supports both file paths and URLs (HTTP/HTTPS). Returns the extracted text content that can be analyzed or processed.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source_path": {
                        "type": "string",
                        "description": "File path (absolute or relative) or URL (http:// or https://) to the document"
                    },
                    "document_type": {
                        "type": "string",
                        "enum": ["pdf", "docx", "txt", "md", "json", "csv", "xml", "html", "xlsx", "xls", "xlsb"],
                        "description": "Document format type. Choose: pdf (Adobe PDF), docx (Microsoft Word), txt (plain text), md (Markdown), json (JSON data), csv (comma-separated values), xml (XML markup), html (web pages), xlsx/xls/xlsb (Microsoft Excel)"
                    }
                },
                "required": ["source_path", "document_type"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Extract parameters
        let source_path = params
            .get("source_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: source_path"))?;

        let document_type = params
            .get("document_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: document_type"))?;

        // Load and extract document
        match self.loader.load_document(source_path, document_type).await {
            Ok(doc_content) => {
                let output = format!(
                    "✅ Document loaded successfully!\n\n\
                     📄 Source: {}\n\
                     📋 Type: {}\n\
                     📏 Size: {} bytes\n\
                     ⏰ Extracted at: {}\n\n\
                     📝 Content Preview:\n\
                     {}\n\n\
                     {} characters extracted.",
                    doc_content.source,
                    doc_content.document_type,
                    doc_content.file_size,
                    doc_content.extracted_at,
                    if doc_content.content.len() > 500 {
                        format!("{}...", &doc_content.content[..500])
                    } else {
                        doc_content.content.clone()
                    },
                    doc_content.content.len()
                );

                Ok(ToolResult::ok(output))
            }
            Err(e) => Ok(ToolResult::err(format!(
                "❌ Failed to load document: {}\n\n\
                 Supported formats: PDF, DOCX, TXT, JSON, CSV, XML, HTML, Excel (XLSX/XLS/XLSB)\n\
                 Supported sources: Local file paths, HTTP/HTTPS URLs",
                e
            ))),
        }
    }
}

/// Helper function to detect document type from file extension or filename
pub fn detect_document_type(filename: &str) -> Option<String> {
    let supported_types = DocumentLoader::supported_types();
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .filter(|ext| supported_types.contains(&ext.as_str()))
}
