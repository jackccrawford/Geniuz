//! Picking a gist from a note's own text.
//!
//! A gist is the one line a reader sees first. An agent writes its own. A
//! person typing in the window is offered a draft instead: the sentence of
//! their note that is closest in meaning to the whole note. Nothing is
//! generated, so the gist is always the writer's own words. It is a draft to
//! accept or replace, and it can be a poor one.

use crate::embedding::cosine_similarity;

/// A note this short is its own gist.
const SHORT: usize = 8;
/// A very long note is judged by its opening; this keeps the work bounded.
const MOST_SENTENCES: usize = 200;

/// The sentences of a text, in order. Split where a sentence ends and at line
/// breaks; scraps too short to say anything ("Hi.", "OK,") are left out.
pub fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev_ends = false;
    for (i, ch) in text.char_indices() {
        let breaks = ch == '\n' || (prev_ends && ch.is_whitespace());
        if breaks {
            push(&mut out, &text[start..i]);
            start = i + ch.len_utf8();
        }
        prev_ends = matches!(ch, '.' | '!' | '?' | '。' | '！' | '？');
    }
    push(&mut out, &text[start..]);
    out
}

fn push<'a>(out: &mut Vec<&'a str>, piece: &'a str) {
    let piece = piece.trim();
    if piece.chars().count() > SHORT {
        out.push(piece);
    }
}

/// The sentence of `text` closest in meaning to the whole of it. A text of one
/// sentence, or none worth the name, comes back whole. `embed` turns a text
/// into its meaning; the first sentence wins a tie.
pub fn pick<E>(text: &str, embed: E) -> Result<String, String>
where
    E: Fn(&str) -> Result<Vec<f32>, String>,
{
    let whole = text.trim();
    let all = sentences(whole);
    if all.len() < 2 {
        return Ok(whole.to_string());
    }
    let target = embed(whole)?;
    let mut best = (f32::MIN, all[0]);
    for s in all.into_iter().take(MOST_SENTENCES) {
        let score = cosine_similarity(&target, &embed(s)?);
        if score > best.0 {
            best = (score, s);
        }
    }
    Ok(best.1.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Meaning, for the tests: how often each of a few words appears.
    fn counts(text: &str) -> Result<Vec<f32>, String> {
        let lower = text.to_lowercase();
        Ok(["library", "moved", "links", "hello", "lunch"].iter().map(|w| lower.matches(w).count() as f32 + 0.01).collect())
    }

    #[test]
    fn sentences_split_at_ends_and_line_breaks_and_drop_scraps() {
        let got = sentences("Hi. The library moved last night!  Old links won't open?\nCornice has the new ones");
        assert_eq!(got, vec!["The library moved last night!", "Old links won't open?", "Cornice has the new ones"]);
        assert_eq!(sentences("Version 5.0.9 shipped today."), vec!["Version 5.0.9 shipped today."], "a dot inside a number ends nothing");
        assert!(sentences("  \n ").is_empty());
    }

    #[test]
    fn the_pick_is_the_sentence_that_carries_the_note_not_the_greeting() {
        let note = "Hello there, hello again. The library moved, so the old library links moved too. I had lunch.";
        assert_eq!(pick(note, counts).unwrap(), "The library moved, so the old library links moved too.");
    }

    #[test]
    fn a_short_note_is_its_own_gist() {
        assert_eq!(pick("  The library moved.  ", counts).unwrap(), "The library moved.");
        assert_eq!(pick("ok", counts).unwrap(), "ok");
        assert_eq!(pick("", counts).unwrap(), "");
    }

    #[test]
    fn a_model_that_fails_is_reported_not_hidden() {
        let err = pick("One sentence here. Another sentence here.", |_| Err("no model".to_string())).unwrap_err();
        assert_eq!(err, "no model");
    }
}
