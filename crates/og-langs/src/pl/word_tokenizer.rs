use og_core::Token;
use std::sync::Arc;

use super::tagger::PolishTagger;

const PREFIXES: [&str; 29] = [
    "arcy", "neo", "pre", "anty", "eks", "bez", "beze", "ekstra", "hiper", "infra", "kontr",
    "maksi", "midi", "między", "mini", "nad", "nade", "około", "ponad", "post", "pro", "przeciw",
    "pseudo", "super", "śród", "ultra", "wice", "wokół", "wokoło",
];

pub struct PolishWordTokenizer {
    tagger: Arc<PolishTagger>,
    inner: og_tokenizer::DefaultWordTokenizer,
}

impl PolishWordTokenizer {
    pub fn new(tagger: Arc<PolishTagger>) -> Self {
        Self {
            tagger,
            inner: og_tokenizer::DefaultWordTokenizer::new_generic(),
        }
    }

    pub fn shared() -> Arc<PolishWordTokenizer> {
        static CACHE: og_core::releasable::Releasable<PolishWordTokenizer> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CACHE, || {
            let tagger = PolishTagger::shared_arc().expect("pl resource dir with polish.dict");
            Some(Arc::new(PolishWordTokenizer::new(tagger)))
        })
        .expect("polish word tokenizer")
    }

    fn is_compound_parts(&self, parts: &[&str]) -> bool {
        let tagger = self.tagger.as_ref();
        let analyzed: Vec<Vec<og_core::AnalyzedToken>> = parts
            .iter()
            .map(|p| tagger.analyzed_tokens_at(p, 0, 0))
            .collect();
        let has_pos = |i: usize, tag: &str| {
            analyzed[i]
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t == tag))
        };
        let has_partial = |i: usize, tag: &str| {
            analyzed[i]
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t.contains(tag)))
        };
        match parts.len() {
            2 => {
                (has_pos(0, "adja") && has_partial(1, "adj:"))
                    || (has_partial(0, "subst:") && has_partial(1, "subst:"))
                    || (has_partial(0, "num:") && has_partial(1, "num:"))
            }
            3 => has_pos(0, "adja") && has_pos(1, "adja") && has_partial(2, "adj:"),
            _ => false,
        }
    }

    fn expand_token(&self, text: &str, start: usize, out: &mut Vec<Token>) {
        if text.chars().count() <= 1 || !text.contains('-') {
            out.push(Token::new(text, start, start + text.len()));
            return;
        }
        if let Some(stem) = text.strip_prefix('-') {
            out.push(Token::new("-", start, start + 1));
            self.expand_token(stem, start + 1, out);
            return;
        }
        if let Some(stem) = text.strip_suffix('-') {
            out.push(Token::new(stem, start, start + stem.len()));
            out.push(Token::new("-", start + stem.len(), start + stem.len() + 1));
            return;
        }
        let parts: Vec<&str> = text.split('-').collect();
        if PREFIXES.contains(&parts[0]) {
            out.push(Token::new(text, start, start + text.len()));
            return;
        }
        if last_part_starts_with_digit(&parts) {
            push_dashes_between(&parts, start, out);
            return;
        }
        let joined_tagged = !self.tagger.analyzed_tokens_at(text, 0, 0).is_empty();
        if !joined_tagged && self.is_compound_parts(&parts) {
            push_dashes_between(&parts, start, out);
            return;
        }
        out.push(Token::new(text, start, start + text.len()));
    }
}

fn last_part_starts_with_digit(parts: &[&str]) -> bool {
    parts
        .last()
        .and_then(|p| p.chars().next())
        .is_some_and(|c| c.is_ascii_digit())
}

fn push_dashes_between(parts: &[&str], start: usize, out: &mut Vec<Token>) {
    let mut offset = start;
    for (i, part) in parts.iter().enumerate() {
        out.push(Token::new(*part, offset, offset + part.len()));
        offset += part.len();
        if i != parts.len() - 1 {
            out.push(Token::new("-", offset, offset + 1));
            offset += 1;
        }
    }
}

fn is_java_tokenizing_char(c: char) -> bool {
    matches!(
        c,
        '\u{0020}'
            | '\u{00A0}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200f}'
            | '\u{2028}'..='\u{202f}'
            | '\u{205f}'
            | '\u{2060}'..='\u{206f}'
            | '\u{3000}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '¦'
            | '‖'
            | '∣'
            | '|'
            | ','
            | '.'
            | ';'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '='
            | '*'
            | '#'
            | '∗'
            | '+'
            | '×'
            | '·'
            | '÷'
            | '<'
            | '>'
            | '!'
            | '?'
            | ':'
            | '~'
            | '/'
            | '\\'
            | '"'
            | '\''
            | '«'
            | '»'
            | '„'
            | '”'
            | '“'
            | '‘'
            | '’'
            | '‚'
            | '`'
            | '´'
            | '‛'
            | '′'
            | '›'
            | '‹'
            | '…'
            | '¿'
            | '¡'
            | '‼'
            | '⁇'
            | '⁈'
            | '⁉'
            | '™'
            | '®'
            | '\u{203d}'
            | '\u{00B6}'
            | '\u{FFEB}'
            | '\u{2E2E}'
            | '\u{2012}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2015}'
            | '\u{2500}'
            | '\u{3161}'
            | '\u{2713}'
            | '\u{25CF}'
            | '\u{25CB}'
            | '\u{25C6}'
            | '\u{27A2}'
            | '\u{25A0}'
            | '\u{25A1}'
            | '\u{2605}'
            | '\u{274F}'
            | '\u{2794}'
            | '\u{21B5}'
            | '\u{2756}'
            | '\u{25AA}'
            | '\u{2751}'
            | '\u{2022}'
            | '\u{2B9A}'
            | '\u{2265}'
            | '\u{2192}'
            | '\u{21FE}'
            | '\u{21C9}'
            | '\u{21D2}'
            | '\u{21E8}'
            | '\u{21DB}'
            | '\u{00B9}'
            | '\u{00B2}'
            | '\u{00B3}'
            | '\u{2070}'
            | '\u{2071}'
            | '\u{2074}'..='\u{2079}'
            | '\t'
            | '\n'
            | '\r'
            | '\u{000B}'
    )
}

fn join_java_runs(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let boundary_joins = out.last().is_some_and(|prev| {
            prev.end() == token.start()
                && prev
                    .text()
                    .chars()
                    .last()
                    .is_some_and(|c| !is_java_tokenizing_char(c))
                && token
                    .text()
                    .chars()
                    .next()
                    .is_some_and(|c| !is_java_tokenizing_char(c))
        });
        if boundary_joins {
            let last = out.last_mut().expect("checked non-empty");
            let end = token.end();
            let mut text = String::with_capacity(last.text().len() + token.text().len());
            text.push_str(last.text());
            text.push_str(token.text());
            let start = last.start();
            *last = Token::new(text, start, end);
        } else {
            out.push(token);
        }
    }
    out
}

impl og_tokenizer::WordTokenizer for PolishWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let base = self.inner.tokenize(text);
        let joined = join_java_runs(base);
        let mut out = Vec::with_capacity(joined.len());
        for token in joined {
            self.expand_token(token.text(), token.start(), &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn tokens(text: &str) -> Vec<String> {
        PolishWordTokenizer::shared()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pins_two_noun_compound_is_split() {
        assert_eq!(tokens("dziecko-geniusz"), vec!["dziecko", "-", "geniusz"]);
    }

    #[test]
    fn java_pins_adjectival_compound_is_split() {
        assert_eq!(
            tokens("polsko-indonezyjski"),
            vec!["polsko", "-", "indonezyjski"]
        );
        assert_eq!(
            tokens("polsko-francusko-niemiecki"),
            vec!["polsko", "-", "francusko", "-", "niemiecki"]
        );
    }

    #[test]
    fn java_pins_numeral_compound_is_split() {
        assert_eq!(
            tokens("osiemnaście-dwadzieścia"),
            vec!["osiemnaście", "-", "dwadzieścia"]
        );
    }

    #[test]
    fn java_pins_morphological_ending_is_not_split() {
        assert_eq!(tokens("SMS-y"), vec!["SMS-y"]);
        assert_eq!(tokens("Rabke-Zdroju"), vec!["Rabke-Zdroju"]);
        assert_eq!(tokens("niemiecki-lotniczy"), vec!["niemiecki-lotniczy"]);
    }

    #[test]
    fn java_pins_prefix_words_stay_whole() {
        assert_eq!(tokens("super-nowoczesny"), vec!["super-nowoczesny"]);
        assert_eq!(tokens("arcy-nowoczesny"), vec!["arcy-nowoczesny"]);
        assert_eq!(tokens("Bez-piesny"), vec!["Bez-piesny"]);
    }

    #[test]
    fn java_pins_number_ranges_split_at_dash() {
        assert_eq!(tokens("1-10"), vec!["1", "-", "10"]);
        assert_eq!(tokens("65-67"), vec!["65", "-", "67"]);
        assert_eq!(tokens("1915-1918"), vec!["1915", "-", "1918"]);
    }

    #[test]
    fn java_pins_trailing_and_leading_hyphens_split_off() {
        assert_eq!(tokens("Bytom-"), vec!["Bytom", "-"]);
        assert_eq!(tokens("-Bytom"), vec!["-", "Bytom"]);
        assert_eq!(tokens("--a"), vec!["-", "-", "a"]);
        assert_eq!(tokens("32-u"), vec!["32-u"]);
    }

    #[test]
    fn java_pins_token_offsets_survive_expansion() {
        let expanded = PolishWordTokenizer::shared().tokenize("A: polsko-indonezyjski b.");
        let joined: Vec<&str> = expanded.iter().map(|t| t.text()).collect();
        assert!(
            joined.contains(&"polsko") && joined.contains(&"-") && joined.contains(&"indonezyjski"),
            "{joined:?}"
        );
        let text = "A: polsko-indonezyjski b.";
        for token in &expanded {
            assert_eq!(
                &text[token.start()..token.end()],
                token.text(),
                "offsets must address the original text"
            );
        }
    }

    #[test]
    fn java_pins_apostrophe_and_low_quote_splitting() {
        assert_eq!(tokens("IPN'ie"), vec!["IPN", "'", "ie"]);
    }

    #[test]
    fn java_pins_percent_stays_glued_to_the_number() {
        assert_eq!(tokens("9%"), vec!["9%"]);
        assert_eq!(tokens("40%"), vec!["40%"]);
        assert_eq!(tokens("3%."), vec!["3%", "."]);
        assert_eq!(
            tokens("Mam 1,5% chleb."),
            vec!["Mam", " ", "1", ",", "5%", " ", "chleb", "."]
        );
    }

    #[test]
    fn java_pins_percent_joining_keeps_hyphen_expansion() {
        assert_eq!(tokens("65-67%,"), vec!["65", "-", "67%", ","]);
        assert_eq!(
            tokens("Wzrósł zaledwie o 1, 9%."),
            vec![
                "Wzrósł", " ", "zaledwie", " ", "o", " ", "1", ",", " ", "9%", "."
            ]
        );
    }

    #[test]
    fn java_pins_joined_runs_keep_original_offsets() {
        let text = "Wzrósł zaledwie o 1, 9%.";
        for token in PolishWordTokenizer::shared().tokenize(text) {
            assert_eq!(
                &text[token.start()..token.end()],
                token.text(),
                "offsets must address the original text"
            );
        }
    }

    #[test]
    fn java_pins_delimiters_are_not_joined_across() {
        assert_eq!(tokens("1,9 %."), vec!["1", ",", "9", " ", "%", "."]);
        assert_eq!(tokens("a. b"), vec!["a", ".", " ", "b"]);
        assert_eq!(
            tokens("To ‚rekiny’."),
            vec!["To", " ", "‚", "rekiny", "’", "."]
        );
        assert_eq!(
            tokens("To jest wyraz „krowa“."),
            vec!["To", " ", "jest", " ", "wyraz", " ", "„", "krowa", "“", "."]
        );
    }

    fn decode_java_escapes(literal: &str) -> String {
        let mut out = String::with_capacity(literal.len());
        let mut rest = literal;
        while let Some(pos) = rest.find("\\u{") {
            out.push_str(&rest[..pos]);
            let after = &rest[pos + 3..];
            let end = after.find('}').expect("closed escape");
            let cp = u32::from_str_radix(&after[..end], 16).expect("hex escape");
            out.push(char::from_u32(cp).expect("valid code point"));
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn java_pins_the_tokenizing_character_set_matches_wordtokenizer_java() {
        let pieces = [
            r"\u{0020}\u{00A0}\u{115f}\u{1160}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200A}\u{200B}\u{200C}\u{200D}\u{200E}\u{200F}\u{2028}\u{2029}\u{202A}\u{202B}\u{202C}\u{202D}\u{202E}\u{202F}\u{205F}\u{2060}\u{2061}\u{2062}\u{2063}\u{206A}\u{206B}\u{206C}\u{206D}\u{206E}\u{206F}\u{3000}\u{3164}\u{FEFF}\u{FFA0}\u{FFF9}\u{FFFA}\u{FFFB}",
            "¦‖∣|,.;()[]{}=*#∗+×·÷<>!?:~/\\\\\"'«»„”“‘’`´‛′›‹…¿¡‼⁇⁈⁉™®\u{203D}\u{00B6}\u{FFEB}\u{2E2E}",
            r"\u{2012}\u{2013}\u{2014}\u{2015}",
            r"\u{2500}\u{3161}\u{2713}",
            r"\u{25CF}\u{25CB}\u{25C6}\u{27A2}\u{25A0}\u{25A1}\u{2605}\u{274F}\u{2794}\u{21B5}\u{2756}\u{25AA}\u{2751}\u{2022}",
            r"\u{2B9A}\u{2265}\u{2192}\u{21FE}\u{21C9}\u{21D2}\u{21E8}\u{21DB}",
            r"\u{00B9}\u{00B2}\u{00B3}\u{2070}\u{2071}\u{2074}\u{2075}\u{2076}\u{2077}\u{2078}\u{2079}",
            "\t\n\r\u{000B}",
            "–‚",
        ];
        let mut covered = std::collections::HashSet::new();
        for piece in pieces {
            for c in decode_java_escapes(piece).chars() {
                assert!(
                    is_java_tokenizing_char(c),
                    "WordTokenizer.java delimiter {c:?} (U+{:04X}) missing from the Rust set",
                    c as u32
                );
                covered.insert(c);
            }
        }
        for not_delim in ['%', '$', '&', '@', '^', '€', '°', '£', '§', 'µ', '-'] {
            assert!(
                !is_java_tokenizing_char(not_delim),
                "{not_delim:?} is not a Java tokenizing character"
            );
        }
        assert!(covered.contains(&'\u{201A}'), "the Polish ‚ addition");
        assert!(covered.len() > 120, "the full Java set is transcribed");
    }
}
