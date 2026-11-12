//! Making sense of words said (or typed) to Buddy, in English and Persian,
//! without needing any language model.
//!
//! The vocabulary is data (`data/lexicon.json`): each meaning lists phrases in
//! each language. Text is normalised the same way on both sides of every
//! comparison, so spelling variants of Persian letters, the half-space and
//! punctuation do not matter. Anything this cannot place is left for a model,
//! if one is available, or for his honest confusion if not.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const LEXICON: &str = include_str!("../data/lexicon.json");

/// What an utterance meant.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Heard {
    /// A cue for something he can do: "sit", "spin", or a word the user has taught him.
    Cue(String),
    Praise,
    Treat,
    /// "That's right": what he just did was what was wanted.
    Approval,
    /// "No, that's wrong": what he just did was not.
    Correction,
    /// "Stop that": whatever he is doing, he should not be.
    Stop,
    Quiet,
    /// Something to keep: the text after "remember that …".
    Remember(String),
    /// Explicit teaching: "when I say CUE, do TRICK".
    Teach {
        cue: String,
        trick: String,
    },
    /// Said to him, but nothing he is asked to do.
    Chat,
    Unknown,
}

/// Lower-cases, unifies Persian letter variants, drops the half-space, diacritics
/// and punctuation, and collapses spaces.
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let mapped = match ch {
            'ي' | 'ى' => 'ی',
            'ك' => 'ک',
            'ة' | 'ۀ' => 'ه',
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ؤ' => 'و',
            // Half-space, joiners, tatweel and short-vowel marks carry no meaning for matching.
            '\u{200c}' | '\u{200d}' | '\u{0640}' | '\u{064b}'..='\u{0652}' => continue,
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.extend(mapped.to_lowercase());
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "fa" if the text contains any Persian or Arabic letter, else "en".
pub fn language(text: &str) -> &'static str {
    if text.chars().any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)) { "fa" } else { "en" }
}

fn lexicon() -> &'static Vec<(String, Vec<String>)> {
    static PARSED: OnceLock<Vec<(String, Vec<String>)>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let raw: BTreeMap<String, BTreeMap<String, Vec<String>>> = serde_json::from_str(LEXICON).expect("data/lexicon.json is valid");
        let mut phrases = Vec::new();
        for (meaning, by_language) in raw {
            for phrase in by_language.into_values().flatten() {
                phrases.push((meaning.clone(), normalise(&phrase).split(' ').map(str::to_string).collect::<Vec<_>>()));
            }
        }
        // Longest phrases first, so "lie down" wins over "down" and "stop that" over "stop".
        phrases.sort_by_key(|(_, words)| std::cmp::Reverse(words.len()));
        phrases
    })
}

/// The meanings that are things he can do, with the name of each.
pub const TRICKS: [&str; 16] =
    ["come", "sit", "down", "stand", "stay", "sleep", "wake", "speak", "paw", "spin", "roll_over", "bow", "shake", "stretch", "play", "look"];

/// Where a phrase (as words) occurs in an utterance (as words), if it does.
fn find(words: &[&str], phrase: &[String]) -> Option<usize> {
    words.windows(phrase.len()).position(|window| window.iter().zip(phrase).all(|(a, b)| a == b))
}

/// Recognises "when I say CUE, (do) TRICK" and its Persian forms.
fn teaching(words: &[&str]) -> Option<Heard> {
    const OPENERS: [&str; 6] = ["when i say", "whenever i say", "وقتی میگم", "وقتی گفتم", "هر وقت میگم", "هر وقت گفتم"];
    const FILLERS: [&str; 12] = ["do", "means", "mean", "it", "you", "should", "then", "to", "یعنی", "بکن", "باید", "تو"];
    let after = OPENERS.iter().find_map(|opener| {
        let opener: Vec<String> = opener.split(' ').map(str::to_string).collect();
        find(words, &opener).map(|at| &words[at + opener.len()..])
    })?;
    // The trick is the first thing in the rest that he knows how to do; the new word is what comes before it.
    let (trick, at) = (1..after.len()).find_map(|at| {
        lexicon()
            .iter()
            .find(|(meaning, phrase)| {
                TRICKS.contains(&meaning.as_str()) && after[at..].starts_with(&phrase.iter().map(String::as_str).collect::<Vec<_>>())
            })
            .map(|(meaning, _)| (meaning.clone(), at))
    })?;
    let cue: Vec<&str> = after[..at].iter().copied().filter(|word| !FILLERS.contains(word)).collect();
    (!cue.is_empty()).then(|| Heard::Teach { cue: cue.join(" "), trick })
}

/// Understands an utterance from the built-in vocabulary and what he has been taught.
/// `names` are what he is called; `taught` are the cue phrases the user has taught him.
pub fn understand(text: &str, names: &[&str], taught: &[String]) -> Heard {
    let normalised = normalise(text);
    let called: Vec<String> = names.iter().map(|n| normalise(n)).collect();
    let words: Vec<&str> = normalised.split(' ').filter(|w| !w.is_empty() && !called.iter().any(|n| n == w)).collect();
    if words.is_empty() {
        // Just his name: he is being called.
        return if normalised.is_empty() { Heard::Unknown } else { Heard::Chat };
    }
    // "When I say X, do Y" is teaching, not a request for Y.
    if let Some(teaching) = teaching(&words) {
        return teaching;
    }
    // What the user taught takes precedence over the built-in vocabulary: it is theirs.
    let mut taught_cues: Vec<Vec<String>> = taught.iter().map(|c| c.split(' ').map(str::to_string).collect()).collect();
    taught_cues.sort_by_key(|c| std::cmp::Reverse(c.len()));
    if let Some(cue) = taught_cues.iter().find(|cue| find(&words, cue).is_some()) {
        return Heard::Cue(cue.join(" "));
    }
    let Some((meaning, at, len)) = lexicon().iter().find_map(|(meaning, phrase)| find(&words, phrase).map(|at| (meaning.as_str(), at, phrase.len())))
    else {
        return Heard::Unknown;
    };
    match meaning {
        "praise" => Heard::Praise,
        "treat" => Heard::Treat,
        "approval" => Heard::Approval,
        "correction" => Heard::Correction,
        "stop" => Heard::Stop,
        "quiet" => Heard::Quiet,
        "remember" => {
            // Keep what was said as it was said, not the normalised form: the last
            // words of the original, as many as followed the phrase.
            let following = words.len() - at - len;
            let said: Vec<&str> = text.split_whitespace().collect();
            let rest = said[said.len().saturating_sub(following)..].join(" ");
            if following == 0 { Heard::Unknown } else { Heard::Remember(rest) }
        }
        trick => Heard::Cue(trick.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hear(text: &str) -> Heard {
        understand(text, &["Buddy", "بادی"], &[])
    }

    #[test]
    fn persian_spelling_variants_are_the_same_word() {
        assert_eq!(normalise("می‌گم"), normalise("میگم"));
        assert_eq!(normalise("آفرين"), normalise("افرین"));
        assert_eq!(normalise("  Good BOY!!  "), "good boy");
        assert_eq!(language("بشین"), "fa");
        assert_eq!(language("sit"), "en");
    }

    #[test]
    fn everyday_commands_are_understood_in_both_languages() {
        for (said, cue) in [
            ("Buddy, come here.", "come"),
            ("بادی بیا اینجا", "come"),
            ("Sit!", "sit"),
            ("بشین", "sit"),
            ("Lie down, boy.", "down"),
            ("دراز بکش", "down"),
            ("Go to sleep.", "sleep"),
            ("برو بخواب", "sleep"),
            ("Let's play!", "play"),
            ("بیا بازی کنیم", "play"),
            ("Look at this", "look"),
            ("اینو ببین", "look"),
            ("give me your paw", "paw"),
            ("دست بده", "paw"),
            ("بچرخ", "spin"),
        ] {
            assert_eq!(hear(said), Heard::Cue(cue.into()), "{said}");
        }
    }

    #[test]
    fn feedback_is_told_apart() {
        assert_eq!(hear("Good boy!"), Heard::Praise);
        assert_eq!(hear("آفرین پسر خوب"), Heard::Praise);
        assert_eq!(hear("No, Buddy, that's wrong."), Heard::Correction);
        assert_eq!(hear("نه، اشتباهه"), Heard::Correction);
        assert_eq!(hear("That's correct."), Heard::Approval);
        assert_eq!(hear("درسته"), Heard::Approval);
        assert_eq!(hear("Stop doing that."), Heard::Stop);
        assert_eq!(hear("بسه"), Heard::Stop);
        assert_eq!(hear("Here's a treat"), Heard::Treat);
    }

    #[test]
    fn a_short_word_does_not_match_inside_a_longer_one() {
        // "no" must not be heard in "know", nor "نه" in "خونه".
        assert_eq!(hear("you know what I mean"), Heard::Unknown);
        assert_eq!(hear("خونه خیلی سرده"), Heard::Unknown);
    }

    #[test]
    fn things_to_remember_keep_their_wording() {
        assert_eq!(hear("Remember that I prefer working late."), Heard::Remember("I prefer working late.".into()));
        assert_eq!(hear("یادت باشه من شب‌ها کار می‌کنم"), Heard::Remember("من شب‌ها کار می‌کنم".into()));
    }

    #[test]
    fn saying_what_a_word_should_mean_is_teaching() {
        assert_eq!(hear("When I say bang, roll over"), Heard::Teach { cue: "bang".into(), trick: "roll_over".into() });
        assert_eq!(hear("Buddy, whenever I say night night it means go to sleep"), Heard::Teach { cue: "night night".into(), trick: "sleep".into() });
        assert_eq!(hear("وقتی می‌گم بپر، یعنی بچرخ"), Heard::Teach { cue: "بپر".into(), trick: "spin".into() });
        assert_eq!(hear("when I say sit"), Heard::Cue("sit".into()), "with nothing to pair it to, it is just the word");
    }

    #[test]
    fn taught_words_come_first_and_his_name_alone_is_a_call() {
        let taught = vec![normalise("بپر")];
        assert_eq!(understand("بادی بپر!", &["Buddy", "بادی"], &taught), Heard::Cue("بپر".into()));
        assert_eq!(hear("Buddy!"), Heard::Chat);
        assert_eq!(hear("what a lovely day"), Heard::Unknown);
    }
}
