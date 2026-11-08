//! Splitting markdown into sections, for `geniuz capture --split`.
//!
//! This file held the OpenClaw workspace import until 5.0.5 (Jack, 7 Nov 2026:
//! removed; an agent brings its own memory in with keep, and people use capture).

pub struct Section {
    pub header: Option<String>,
    pub content: String,
}

/// Split markdown content by ## headers into sections
pub fn split_sections(content: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current = String::new();
    let mut current_header: Option<String> = None;

    for line in content.lines() {
        if line.starts_with("## ") && !current.trim().is_empty() {
            sections.push(Section {
                header: current_header.take(),
                content: current.trim().to_string(),
            });
            current = String::new();
        }
        if line.starts_with("## ") {
            current_header = Some(line.trim_start_matches('#').trim().to_string());
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        sections.push(Section {
            header: current_header.take(),
            content: current.trim().to_string(),
        });
    }

    sections
}
