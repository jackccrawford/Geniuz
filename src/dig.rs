//! Exact match, line by line: `--grep` (search feature 2).
//!
//! Semantic search is for discovery; exact search is for recall — who, when,
//! did it happen, by proper name. A name is not a meaning: "Cubic" embeds near
//! "review", not near itself, and the deciding facts often sit deep in a body
//! a gist never mentions. So `--grep` reads FULL content and returns the
//! matching LINES, not whole memories, which keeps the reader's context small
//! (Ember's dig, docs/SEARCH-DESIGN.md).
//!
//! A memory qualifies when EVERY term appears in it, anywhere; from it, each
//! line holding ANY term is shown. Matching is case-blind, Unicode included.

/// Lines longer than this are cut around their first match.
const LINE_MAX: usize = 240;

/// The terms of one `--grep` (each flag is one term; a term may hold spaces).
#[derive(Debug, Clone, PartialEq)]
pub struct Terms {
    raw: Vec<String>,
    lower: Vec<String>,
}

impl Terms {
    pub fn parse<S: AsRef<str>>(raw: &[S]) -> Result<Self, String> {
        let raw: Vec<String> = raw.iter().map(|t| t.as_ref().trim().to_string()).filter(|t| !t.is_empty()).collect();
        if raw.is_empty() {
            return Err("--grep needs a term to look for".to_string());
        }
        let lower = raw.iter().map(|t| t.to_lowercase()).collect();
        Ok(Terms { raw, lower })
    }

    /// Every term appears somewhere in `text`.
    pub fn all_in(&self, text: &str) -> bool {
        let t = text.to_lowercase();
        self.lower.iter().all(|term| t.contains(term.as_str()))
    }

    /// The lines of `text` holding any term, trimmed and cut to [`LINE_MAX`]
    /// around the first match.
    pub fn matching_lines(&self, text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| {
                let l = line.to_lowercase();
                let at = self.lower.iter().filter_map(|term| l.find(term.as_str())).min()?;
                Some(clip(line.trim(), &l, at))
            })
            .collect()
    }

    /// A SQL prefilter on `expr`, numbered from `?first`: one case-blind
    /// `LIKE` per ASCII term. Terms with non-ASCII letters are left to
    /// [`Terms::all_in`], because SQLite folds only ASCII case and a `LIKE`
    /// on them could drop a true match. `("1", [])` when nothing narrows.
    pub fn sql(&self, expr: &str, first: usize) -> (String, Vec<String>) {
        let mut parts = Vec::new();
        let mut params = Vec::new();
        for term in self.raw.iter().filter(|t| t.is_ascii()) {
            parts.push(format!("{expr} LIKE ?{} ESCAPE '\\'", first + params.len()));
            params.push(format!("%{}%", escape_like(term)));
        }
        if parts.is_empty() {
            ("1".to_string(), params)
        } else {
            (parts.join(" AND "), params)
        }
    }

    /// How the search reads in a scope line: `grep "Cubic" "PR"`.
    pub fn describe(&self) -> String {
        let quoted: Vec<String> = self.raw.iter().map(|t| format!("\"{t}\"")).collect();
        format!("grep {}", quoted.join(" "))
    }
}

/// One matching line, with the memory it came from.
#[derive(Debug, Clone)]
pub struct Hit {
    pub memory_uuid: String,
    pub created_at: String,
    /// The chapter (Geniuz Team turns); `None` in Geniuz Free.
    pub chapter: Option<i64>,
    pub line: String,
}

/// The lines found, and how many memories matched in all — which may be more
/// than the lines shown, when the limit cut the list.
#[derive(Debug, Clone, Default)]
pub struct Dig {
    pub hits: Vec<Hit>,
    pub memories: usize,
    pub lines: usize,
}

impl Dig {
    /// Add one memory's text: counted if it qualifies, its lines kept while
    /// there is room under `limit`.
    pub fn take(&mut self, terms: &Terms, uuid: &str, created_at: &str, chapter: Option<i64>, text: &str, limit: usize) {
        if !terms.all_in(text) {
            return;
        }
        self.memories += 1;
        for line in terms.matching_lines(text) {
            self.lines += 1;
            if self.hits.len() < limit {
                self.hits.push(Hit {
                    memory_uuid: uuid.to_string(),
                    created_at: created_at.to_string(),
                    chapter,
                    line,
                });
            }
        }
    }

    /// The scope line's mode: `grep "Cubic" · 3 lines in 2 memories`, with
    /// `(first N shown)` when the limit cut it.
    pub fn mode(&self, terms: &Terms) -> String {
        let shown = if self.hits.len() < self.lines {
            format!(" (first {} shown)", self.hits.len())
        } else {
            String::new()
        };
        format!(
            "{} · {} {} in {} {}{shown}",
            terms.describe(),
            self.lines,
            if self.lines == 1 { "line" } else { "lines" },
            self.memories,
            if self.memories == 1 { "memory" } else { "memories" },
        )
    }
}

/// `line` cut to [`LINE_MAX`] characters, keeping the match at byte `at` of
/// its lowercase form in view. Lowercasing can shift byte offsets, so the cut
/// is made by characters on the original.
fn clip(line: &str, lower: &str, at: usize) -> String {
    let n = line.chars().count();
    if n <= LINE_MAX {
        return line.to_string();
    }
    let at_char = lower[..at.min(lower.len())].chars().count().min(n);
    let start = at_char.saturating_sub(LINE_MAX / 3);
    let end = (start + LINE_MAX).min(n);
    let start = end.saturating_sub(LINE_MAX);
    let body: String = line.chars().skip(start).take(end - start).collect();
    format!("{}{}{}", if start > 0 { "…" } else { "" }, body, if end < n { "…" } else { "" })
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_term_must_appear_and_any_term_shows_a_line() {
        let t = Terms::parse(&["cubic", "PR"]).unwrap();
        let text = "Opening line\nCubic reviewed it.\nnothing here\nThe PR merged.";
        assert!(t.all_in(text));
        assert_eq!(t.matching_lines(text), ["Cubic reviewed it.", "The PR merged."]);
        assert!(!t.all_in("Cubic only"));
    }

    #[test]
    fn matching_is_case_blind_beyond_ascii() {
        let t = Terms::parse(&["ÄRGER"]).unwrap();
        assert!(t.all_in("kein ärger hier"));
        let (cond, params) = t.sql("x", 1);
        assert_eq!((cond.as_str(), params.len()), ("1", 0), "a non-ASCII term must not narrow in SQL");
    }

    #[test]
    fn the_sql_prefilter_escapes_wildcards() {
        let t = Terms::parse(&["50%", "a_b"]).unwrap();
        let (cond, params) = t.sql("content", 3);
        assert_eq!(cond, "content LIKE ?3 ESCAPE '\\' AND content LIKE ?4 ESCAPE '\\'");
        assert_eq!(params, ["%50\\%%", "%a\\_b%"]);
    }

    #[test]
    fn a_long_line_is_cut_around_its_match() {
        let t = Terms::parse(&["needle"]).unwrap();
        let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
        let got = &t.matching_lines(&line)[0];
        assert!(got.contains("needle") && got.starts_with('…') && got.ends_with('…'), "{got}");
        assert!(got.chars().count() <= LINE_MAX + 2);
    }

    #[test]
    fn the_dig_counts_past_its_limit_and_says_so() {
        let t = Terms::parse(&["x"]).unwrap();
        let mut d = Dig::default();
        d.take(&t, "A", "t", None, "x1\nx2", 3);
        d.take(&t, "B", "t", None, "x3\nx4", 3);
        d.take(&t, "C", "t", None, "nothing", 3);
        assert_eq!((d.hits.len(), d.lines, d.memories), (3, 4, 2));
        assert_eq!(d.mode(&t), "grep \"x\" · 4 lines in 2 memories (first 3 shown)");
        assert!(Terms::parse(&["  "]).is_err());
    }
}
