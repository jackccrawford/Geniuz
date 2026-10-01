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

/// Terms as a person types them in one box: words, with "quoted phrases"
/// kept whole. `Cubic "pull request"` is two terms.
pub fn split_terms(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, part) in input.split('"').enumerate() {
        if i % 2 == 1 {
            if !part.trim().is_empty() {
                out.push(part.trim().to_string());
            }
        } else {
            out.extend(part.split_whitespace().map(String::from));
        }
    }
    out
}

/// The proper names in a meaning query (search feature 3): every "quoted
/// term", and every word that starts with a capital letter but is not the
/// query's first word. A deliberately simple rule, so it can be steered:
/// quote a term to make it a name; lowercase a word to make it a meaning.
/// Plain lowercase queries have no names and rank exactly as before.
pub fn names(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = String::new();
    let mut quoted = query.split('"');
    if let Some(first) = quoted.next() {
        rest.push_str(first);
    }
    for (i, part) in quoted.enumerate() {
        if i % 2 == 0 {
            let p = part.trim();
            if !p.is_empty() {
                out.push(p.to_string());
            }
            rest.push(' ');
        } else {
            rest.push_str(part);
        }
    }
    let first_word = query.split_whitespace().next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric());
    for (i, word) in rest.split_whitespace().enumerate() {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric());
        let capital = w.chars().next().is_some_and(|c| c.is_uppercase());
        let is_first = i == 0 && w == first_word && !query.trim_start().starts_with('"');
        if capital && !is_first && !out.iter().any(|n| n == w) {
            out.push(w.to_string());
        }
    }
    out
}

/// A meaning search's mode for its scope line: `semantic "q"`, plus
/// ` · names first: Cubic` when the query held proper names.
pub fn semantic_mode(query: &str) -> String {
    let n = names(query);
    if n.is_empty() {
        format!("semantic \"{query}\"")
    } else {
        format!("semantic \"{query}\" · names first: {}", n.join(", "))
    }
}

/// How many of `names` appear in `text` verbatim (case kept).
pub fn names_in(names: &[String], text: &str) -> usize {
    names.iter().filter(|n| text.contains(n.as_str())).count()
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

/// What a result set points onward to (search feature 4): proper names
/// that recur in the hits but were not asked for, and ids the hits cite.
/// Read from the returned text only, so it costs nothing to keep.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Harvest {
    /// `(name, times seen)`, most seen first.
    pub names: Vec<(String, usize)>,
    /// `XXXXXXXX:YYYYYYYY` signal pointers and bare 8-hex memory ids.
    pub pointers: Vec<String>,
}

impl Harvest {
    /// Harvest `texts` (the hits as returned). A name is a Capitalized word
    /// that does not open a sentence, seen at least twice, not in `query`.
    /// A pointer is two 8-hex groups joined by `:`, or one 8-hex word holding
    /// both a digit and a letter (so dates and words are not ids); the hits'
    /// own ids (`own`) are not pointers onward.
    pub fn of(texts: &[&str], query: &str, own: &[String]) -> Self {
        let asked: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .map(|w| w.to_lowercase())
            .collect();
        let own: Vec<String> = own.iter().map(|u| u.chars().take(8).collect::<String>().to_lowercase()).collect();
        let mut names: Vec<(String, usize)> = Vec::new();
        let mut pointers: Vec<String> = Vec::new();
        for text in texts {
            for line in text.lines() {
                let mut sentence_start = true;
                for raw in line.split_whitespace() {
                    let word = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != ':' && c != '-');
                    let word = word.trim_end_matches(|c: char| c == ':' || c == '-');
                    if let Some(p) = pointer(word) {
                        if !own.contains(&p[..8].to_lowercase()) && !pointers.contains(&p) {
                            pointers.push(p);
                        }
                    } else {
                        let w = word.trim_matches(|c: char| !c.is_alphanumeric());
                        let capital = w.chars().next().is_some_and(|c| c.is_uppercase());
                        let has_lower = w.chars().any(|c| c.is_lowercase());
                        if capital && has_lower && w.chars().count() > 1 && !sentence_start
                            && !asked.contains(&w.to_lowercase()) && !is_calendar_word(w)
                        {
                            match names.iter_mut().find(|(n, _)| n == w) {
                                Some((_, k)) => *k += 1,
                                None => names.push((w.to_string(), 1)),
                            }
                        }
                    }
                    sentence_start = raw.ends_with(['.', '!', '?', ':']);
                }
            }
        }
        names.retain(|(_, k)| *k >= 2);
        names.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        names.truncate(8);
        pointers.truncate(12);
        Harvest { names, pointers }
    }

    /// The closing lines, before the scope: `names seen: Cubic (3), Devin (2)`
    /// and `pointers: 00000000:87C1FADC, 13625F6B`. Empty parts are left out.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.names.is_empty() {
            let n: Vec<String> = self.names.iter().map(|(n, k)| format!("{n} ({k})")).collect();
            out.push(format!("names seen: {}", n.join(", ")));
        }
        if !self.pointers.is_empty() {
            out.push(format!("pointers: {}", self.pointers.join(", ")));
        }
        out
    }
}

/// Month and weekday names, whole or short ("Sep 2026", "on Tuesday"):
/// capitalised by the calendar, not because they name anyone.
fn is_calendar_word(w: &str) -> bool {
    const SHORT: [&str; 20] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "sept", "oct", "nov", "dec",
        "mon", "tue", "wed", "thu", "fri", "sat", "sun",
    ];
    const FULL: [&str; 18] = [
        "january", "february", "march", "april", "june", "july", "august", "september", "october",
        "november", "december", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
    ];
    let l = w.to_lowercase();
    SHORT.contains(&l.as_str()) || FULL.contains(&l.as_str())
}

/// `word` as a pointer: `XXXXXXXX:YYYYYYYY`, or a bare 8-hex id with at
/// least one digit and one letter. Kept in the case it was written.
fn pointer(word: &str) -> Option<String> {
    let hex8 = |s: &str| s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit());
    if let Some((a, b)) = word.split_once(':') {
        return (hex8(a) && hex8(b)).then(|| word.to_string());
    }
    let mixed = word.chars().any(|c| c.is_ascii_digit()) && word.chars().any(|c| c.is_ascii_alphabetic());
    (hex8(word) && mixed).then(|| word.to_string())
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
    fn names_are_quoted_terms_and_capitals_past_the_first_word() {
        assert!(names("third-party reviewers past week").is_empty());
        assert_eq!(names("reviews from Cubic last week"), ["Cubic"]);
        assert!(names("Cubic reviews").is_empty(), "the first word is capitalised by grammar");
        assert_eq!(names("what did \"cubic\" say"), ["cubic"]);
        assert_eq!(names("\"Cubic\" and Devin, on PR-123?"), ["Cubic", "Devin", "PR-123"]);
        assert_eq!(names_in(&names("from Cubic and Devin"), "Cubic said so; cubic is a shape"), 1);
    }

    #[test]
    fn harvest_names_recur_and_pointers_point_onward() {
        let hits = [
            "Cubic reviewed it. Then Devin and Cubic argued.",
            "We asked Devin about 00000000:87C1FADC and 13625F6B.",
            "In March the date 20260930 is not an id; DEADBEEF has no digit; 0123ABCD is ours.",
        ];
        let h = Harvest::of(&hits, "what did reviewers say", &["0123ABCD-0000".to_string()]);
        assert_eq!(h.names, [("Devin".to_string(), 2)], "Cubic opens a sentence once, so it is seen once mid-sentence");
        assert_eq!(h.pointers, ["00000000:87C1FADC", "13625F6B"]);
        assert_eq!(h.lines(), ["names seen: Devin (2)", "pointers: 00000000:87C1FADC, 13625F6B"]);
        let dated = Harvest::of(&["Shipped Sep 2026 and Sep 30.", "Met on Tuesday, then Tuesday again."], "", &[]);
        assert!(dated.names.is_empty(), "calendar words are not names: {:?}", dated.names);
        let asked = Harvest::of(&hits, "and Devin?", &[]);
        assert!(asked.names.is_empty(), "a name the query asked for is not news");
        assert!(Harvest::of(&[], "", &[]).lines().is_empty());
    }

    #[test]
    fn typed_terms_keep_quoted_phrases_whole() {
        assert_eq!(split_terms("Cubic \"pull request\"  PR"), ["Cubic", "pull request", "PR"]);
        assert!(split_terms("  \"\" ").is_empty());
    }

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
