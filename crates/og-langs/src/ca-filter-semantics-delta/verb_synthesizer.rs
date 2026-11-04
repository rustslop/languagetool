use std::sync::LazyLock;

use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use regex::Regex;

use super::token_scan::{
    has_any_partial_pos_tag, has_pos_tag_starting_with, reading_and_tag_matching,
    reading_with_tag_regex,
};

pub static P_VERB: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.*$").unwrap());
pub static P_INFLECTED_VERB: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[SIM].*$").unwrap());
#[allow(dead_code)]
pub static P_IMPERATIVE_VERB: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.M.*$").unwrap());
pub static P_VERB_IS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[IS].*$").unwrap());
static P_NON_PARTICIPLE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[^P].*$").unwrap());
static P_PARTICIPLE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.P.*$").unwrap());
pub static P_PRONOM_FEBLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^P0.{6}$|^PP3CN000$|^PP3NN000$|^PP3..A00$|^PP[123]CP000$|^PP3CSD00$").unwrap()
});

pub struct VerbSynthesizer<'a> {
    tokens: &'a [&'a AnalyzedTokenReadings],
    i_first_verb: isize,
    i_last_verb: isize,
    num_pronouns_before: isize,
    num_pronouns_after: isize,
    #[allow(dead_code)]
    search_backward: bool,
}

impl<'a> VerbSynthesizer<'a> {
    pub fn new(tokens: &'a [&'a AnalyzedTokenReadings], start_pos: isize) -> Self {
        Self::with_search_backward(tokens, start_pos, false)
    }

    pub fn with_search_backward(
        tokens: &'a [&'a AnalyzedTokenReadings],
        start_pos: isize,
        search_backward: bool,
    ) -> Self {
        let mut synth = Self {
            tokens,
            i_first_verb: -1,
            i_last_verb: -1,
            num_pronouns_before: -1,
            num_pronouns_after: -1,
            search_backward,
        };
        synth.set_indexes(start_pos);
        synth
    }

    fn set_indexes(&mut self, start_pos: isize) {
        let len = self.tokens.len() as isize;
        if start_pos < 0 || start_pos >= len {
            return;
        }
        if reading_with_tag_regex(self.tokens[start_pos as usize], &P_PARTICIPLE).is_some()
            && !self.tokens[start_pos as usize].has_pos_tag("_GV_")
            && !self.tokens[start_pos as usize].has_chunk("GV")
        {
            self.i_first_verb = start_pos;
            self.i_last_verb = start_pos;
            self.num_pronouns_before = 0;
            self.num_pronouns_after = 0;
            return;
        }
        let mut j = start_pos;
        if self.search_backward {
            while j > 0 && !self.is_verb_at(j) {
                j -= 1;
            }
            let mut found_some_verb = false;
            while j > 0 && self.is_verb_at(j) {
                found_some_verb = true;
                j -= 1;
            }
            if found_some_verb {
                j += 1;
            }
        } else {
            while j < len && !self.is_verb_at(j) {
                j += 1;
            }
        }
        if self.is_verb_at(j) {
            self.i_first_verb = j;
            self.i_last_verb = j;
            let mut i = j - 1;
            while self.is_multitoken_verb_at(i) && !self.is_first_verb_is() {
                self.i_first_verb = i;
                i -= 1;
            }
            let mut i = j + 1;
            while self.is_multitoken_verb_at(i)
                && !(self.is_first_verb_is() && self.is_verb_is_at(i))
            {
                self.i_last_verb = i;
                i += 1;
            }
        } else {
            return;
        }

        let mut i = 1isize;
        let mut pronouns_after = 0isize;
        while self.i_last_verb + i < len
            && !self.whitespace_before(self.i_last_verb + i)
            && reading_with_tag_regex(
                self.tokens[(self.i_last_verb + i) as usize],
                &P_PRONOM_FEBLE,
            )
            .is_some()
        {
            pronouns_after += 1;
            i += 1;
        }
        self.num_pronouns_after = pronouns_after;

        let mut i = -1isize;
        let mut pronouns_before_no_space_before = 0isize;
        let mut pronouns_before = 0isize;
        while self.i_first_verb + i > 0
            && reading_with_tag_regex(
                self.tokens[self.idx(self.i_first_verb + i)],
                &P_PRONOM_FEBLE,
            )
            .is_some()
        {
            let pos = self.i_first_verb + i;
            let idx = self.idx(pos);
            if self.whitespace_before(pos)
                || pos == 1
                || has_pos_tag_starting_with(self.tokens[idx - 1], "_QM")
            {
                pronouns_before += pronouns_before_no_space_before + 1;
                pronouns_before_no_space_before = 0;
            } else {
                pronouns_before_no_space_before += 1;
            }
            i -= 1;
        }
        self.num_pronouns_before = pronouns_before;
    }

    fn idx(&self, i: isize) -> usize {
        i.clamp(0, self.tokens.len() as isize - 1) as usize
    }

    fn is_verb_at(&self, i: isize) -> bool {
        let len = self.tokens.len() as isize;
        if i < 0 || i > len - 1 {
            return false;
        }
        let t = self.tokens[i as usize];
        t.has_chunk("GV")
            || reading_with_tag_regex(t, &P_NON_PARTICIPLE).is_some()
            || (reading_with_tag_regex(t, &P_PARTICIPLE).is_some() && t.has_pos_tag("_GV_"))
    }

    fn is_multitoken_verb_at(&self, i: isize) -> bool {
        let len = self.tokens.len() as isize;
        if i < 0 || i > len - 1 {
            return false;
        }
        let t = self.tokens[i as usize];
        t.has_chunk("GV") || t.has_pos_tag("_GV_")
    }

    fn is_verb_is_at(&self, i: isize) -> bool {
        let len = self.tokens.len() as isize;
        if i < 0 || i >= len {
            return false;
        }
        reading_with_tag_regex(self.tokens[i as usize], &P_VERB_IS).is_some()
    }

    pub fn is_first_verb_is(&self) -> bool {
        if self.i_first_verb == -1 {
            return false;
        }
        self.is_verb_is_at(self.i_first_verb)
    }

    fn whitespace_before(&self, i: isize) -> bool {
        if i <= 0 || i >= self.tokens.len() as isize {
            return false;
        }
        let idx = i as usize;
        self.tokens[idx].token().start() > self.tokens[idx - 1].token().end()
    }

    pub fn is_undefined(&self) -> bool {
        self.i_first_verb == -1
            || self.i_last_verb == -1
            || self.num_pronouns_after == -1
            || self.num_pronouns_before == -1
    }

    pub fn first_verb_index(&self) -> isize {
        self.i_first_verb
    }

    pub fn last_verb_index(&self) -> isize {
        self.i_last_verb
    }

    pub fn last_index(&self) -> isize {
        self.i_last_verb + self.num_pronouns_after
    }

    pub fn num_pronouns_after(&self) -> isize {
        self.num_pronouns_after
    }

    pub fn num_pronouns_before(&self) -> isize {
        self.num_pronouns_before
    }

    pub fn get_string_from_to(&self, start: isize, end: isize) -> String {
        let mut out = String::new();
        let mut i = start.max(0);
        while i <= end && i < self.tokens.len() as isize {
            if i > start && self.whitespace_before(i) {
                out.push(' ');
            }
            out.push_str(self.tokens[i as usize].token().token());
            i += 1;
        }
        out
    }

    pub fn get_pronouns_str_before(&self) -> String {
        self.get_string_from_to(
            self.i_first_verb - self.num_pronouns_before,
            self.i_first_verb - 1,
        )
    }

    pub fn get_pronouns_str_after(&self) -> String {
        self.get_string_from_to(
            self.i_last_verb + 1,
            self.i_last_verb + self.num_pronouns_after,
        )
    }

    pub fn get_whole_original_str(&self) -> String {
        self.get_string_from_to(
            self.i_first_verb - self.num_pronouns_before,
            self.i_last_verb + self.num_pronouns_after,
        )
    }

    pub fn get_verb_str(&self) -> String {
        self.get_string_from_to(self.i_first_verb, self.i_last_verb)
    }

    pub fn get_casing_model(&self) -> String {
        self.get_string_from_to(
            self.i_first_verb - self.num_pronouns_before,
            self.i_first_verb,
        )
    }

    fn first_verb_reading(&self) -> Option<&AnalyzedToken> {
        if self.i_first_verb < 0 || self.i_first_verb >= self.tokens.len() as isize {
            return None;
        }
        reading_with_tag_regex(self.tokens[self.i_first_verb as usize], &P_VERB)
    }

    fn last_verb_reading(&self) -> Option<&AnalyzedToken> {
        if self.i_last_verb < 0 || self.i_last_verb >= self.tokens.len() as isize {
            return None;
        }
        reading_with_tag_regex(self.tokens[self.i_last_verb as usize], &P_VERB)
    }

    pub fn last_verb_lemma(&self) -> Option<&str> {
        self.last_verb_reading().and_then(|r| r.lemma())
    }

    pub fn first_verb_postag(&self) -> Option<&str> {
        self.first_verb_reading()
            .and_then(|r| r.pos_tags().first().map(String::as_str))
    }

    pub fn get_first_verb_persona_number(&self) -> String {
        if self.i_first_verb < 0 || self.i_first_verb >= self.tokens.len() as isize {
            return String::new();
        }
        match reading_and_tag_matching(self.tokens[self.i_first_verb as usize], &P_INFLECTED_VERB) {
            Some((_, tag)) => substring(tag, 4, 6),
            None => String::new(),
        }
    }

    pub fn get_first_verb_persona_number_imperative(&self) -> String {
        if self.i_first_verb < 0 || self.i_first_verb >= self.tokens.len() as isize {
            return String::new();
        }
        match reading_and_tag_matching(self.tokens[self.i_first_verb as usize], &P_IMPERATIVE_VERB)
        {
            Some((_, tag)) => substring(tag, 4, 6),
            None => String::new(),
        }
    }

    pub fn get_first_verb_is_postag(&self) -> Option<String> {
        if self.i_first_verb == -1 || self.i_first_verb >= self.tokens.len() as isize {
            return None;
        }
        reading_and_tag_matching(self.tokens[self.i_first_verb as usize], &P_VERB_IS)
            .map(|(_, tag)| tag.to_string())
    }

    pub fn is_passat_perifrastic(&self) -> bool {
        let len = self.tokens.len() as isize;
        if self.i_first_verb < 1 || self.i_first_verb + 1 > len - 1 {
            return false;
        }
        let first = self.tokens[self.i_first_verb as usize];
        let next = self.tokens[(self.i_first_verb + 1) as usize];
        has_pos_tag_starting_with(first, "VA")
            && first.has_lemma("anar")
            && has_any_partial_pos_tag(next, &["VMN", "VSN", "VAN"])
    }

    pub fn is_perfet(&self) -> bool {
        let len = self.tokens.len() as isize;
        if self.i_first_verb < 1 || self.i_first_verb + 1 > len - 1 {
            return false;
        }
        let first = self.tokens[self.i_first_verb as usize];
        let next = self.tokens[(self.i_first_verb + 1) as usize];
        has_pos_tag_starting_with(first, "VA")
            && first.has_lemma("haver")
            && has_any_partial_pos_tag(next, &["VMP", "VSP", "VAP"])
    }

    pub fn synthesize(
        &self,
        new_lemma: &str,
        new_postag: &str,
        synth: &dyn Fn(&str, &str) -> Vec<String>,
        adapt: &dyn Fn(&str) -> String,
    ) -> String {
        let mut result = String::new();
        let first_verb = self.first_verb_reading();
        if self.i_first_verb == self.i_last_verb {
            if let Some(form) = synth(new_lemma, &adjust_postag_to_lemma(new_lemma, new_postag))
                .into_iter()
                .next()
            {
                result.push_str(&form);
            }
        } else {
            let mut i = self.i_first_verb;
            while i <= self.i_last_verb {
                if i == self.i_first_verb {
                    if let Some(fv) = first_verb {
                        let lemma = fv.lemma().unwrap_or("");
                        if let Some(form) = synth(lemma, &adjust_postag_to_lemma(lemma, new_postag))
                            .into_iter()
                            .next()
                        {
                            result.push_str(&form);
                        }
                    }
                } else if i == self.i_last_verb {
                    if self.whitespace_before(i) {
                        result.push(' ');
                    }
                    let last_verb = self.last_verb_reading().or(first_verb);
                    if let Some(lv) = last_verb {
                        let postag = lv.pos_tags().first().map(String::as_str).unwrap_or("");
                        if let Some(form) =
                            synth(new_lemma, &adjust_postag_to_lemma(new_lemma, postag))
                                .into_iter()
                                .next()
                        {
                            result.push_str(&form);
                        }
                    }
                } else if i >= 0 && i < self.tokens.len() as isize {
                    if self.whitespace_before(i) {
                        result.push(' ');
                    }
                    result.push_str(self.tokens[i as usize].token().token());
                }
                i += 1;
            }
        }
        adapt(&result)
    }
}

pub fn adjust_postag_to_lemma(lemma: &str, postag: &str) -> String {
    let mut out = postag.to_string();
    if lemma == "haver" {
        out = format!("VA{}", skip_chars(postag, 2));
    }
    if lemma == "ser" {
        out = format!("VS{}", skip_chars(postag, 2));
    }
    out
}

fn skip_chars(s: &str, n: usize) -> String {
    s.chars().skip(n).collect()
}

fn substring(s: &str, from: usize, to: usize) -> String {
    s.chars().skip(from).take(to.saturating_sub(from)).collect()
}
