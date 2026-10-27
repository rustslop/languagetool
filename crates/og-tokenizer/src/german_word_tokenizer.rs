use crate::WordTokenizer;
use og_core::Token;

const TOKENIZING_CHARACTERS: &str = concat!(
    "\u{0020}\u{00A0}\u{115f}\u{1160}\u{1680}",
    "\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}",
    "\u{2008}\u{2009}\u{200A}\u{200B}\u{200c}\u{200d}\u{200e}\u{200f}",
    "\u{2028}\u{2029}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{202f}",
    "\u{205F}\u{2060}\u{2061}\u{2062}\u{2063}\u{206A}\u{206b}\u{206c}\u{206d}",
    "\u{206E}\u{206F}\u{3000}\u{3164}\u{feff}\u{ffa0}\u{fff9}\u{fffa}\u{fffb}",
    "¦‖∣|,.;()[]{}=*#∗+×·÷<>!?:~/\\\"'«»„“”‘’`´‛′›‹…¿¡‼⁇⁈⁉™®",
    "\u{203d}\u{00B6}\u{FFEB}\u{2E2E}",
    "\u{2012}\u{2013}\u{2014}\u{2015}",
    "\u{2500}\u{3161}\u{2713}",
    "\u{25CF}\u{25CB}\u{25C6}\u{27A2}\u{25A0}\u{25A1}\u{2605}\u{274F}\u{2794}\u{21B5}\u{2756}\u{25AA}\u{2751}\u{2022}",
    "\u{2B9A}\u{2265}\u{2192}\u{21FE}\u{21C9}\u{21D2}\u{21E8}\u{21DB}",
    "\u{00b9}\u{00b2}\u{00b3}\u{2070}\u{2071}\u{2074}\u{2075}\u{2076}\u{2077}\u{2078}\u{2079}",
    "\u{000B}\t\n\r",
    "_\u{201A}",
);

pub struct GermanWordTokenizer;

impl GermanWordTokenizer {
    pub fn new() -> Self {
        Self
    }

    fn is_delimiter(c: char) -> bool {
        TOKENIZING_CHARACTERS.contains(c)
    }

    fn url_or_email_end(text: &str, byte_start: usize) -> Option<usize> {
        let remaining = &text[byte_start..];
        if remaining.starts_with("http://")
            || remaining.starts_with("https://")
            || remaining.starts_with("ftp://")
            || remaining.starts_with("www.")
        {
            let end = remaining
                .find(char::is_whitespace)
                .unwrap_or(remaining.len());
            return Some(byte_start + end);
        }
        if let Some(at_pos) = remaining.find('@')
            && at_pos > 0
            && remaining[..at_pos]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '+')
        {
            let domain = &remaining[at_pos + 1..];
            let domain_end = domain.find(char::is_whitespace).unwrap_or(domain.len());
            let domain_part = &domain[..domain_end];
            let trimmed = domain_part.trim_end_matches(|c: char| !c.is_alphanumeric());
            let top_label = trimmed.rsplit('.').next().unwrap_or_default();
            if trimmed.contains('.')
                && top_label.len() >= 2
                && trimmed
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
            {
                return Some(byte_start + at_pos + 1 + trimmed.len());
            }
        }
        None
    }
}

impl Default for GermanWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WordTokenizer for GermanWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut byte = 0usize;
        while byte < text.len() {
            let c = text[byte..].chars().next().expect("char at byte");
            if c.is_whitespace() {
                let start = byte;
                let mut end = byte;
                let mut rest = text[byte..].chars();
                while let Some(wc) = rest.next()
                    && wc.is_whitespace()
                {
                    end += wc.len_utf8();
                }
                tokens.push(Token::new(&text[start..end], start, end));
                byte = end;
                continue;
            }
            if let Some(end) = Self::url_or_email_end(text, byte) {
                tokens.push(Token::new(&text[byte..end], byte, end));
                byte = end;
                continue;
            }
            if Self::is_delimiter(c) {
                let end = byte + c.len_utf8();
                tokens.push(Token::new(&text[byte..end], byte, end));
                byte = end;
                continue;
            }
            let start = byte;
            let mut end = byte;
            let rest = text[byte..].chars();
            for rc in rest {
                if rc.is_whitespace() || Self::is_delimiter(rc) {
                    break;
                }
                end += rc.len_utf8();
            }
            tokens.push(Token::new(&text[start..end], start, end));
            byte = end;
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(input: &str) -> Vec<String> {
        GermanWordTokenizer::new()
            .tokenize(input)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn hyphen_glued_between_and_at_edges() {
        assert_eq!(
            texts("Es zählt nur das Preis-/Leistungsverhältnis."),
            vec![
                "Es",
                " ",
                "zählt",
                " ",
                "nur",
                " ",
                "das",
                " ",
                "Preis-",
                "/",
                "Leistungsverhältnis",
                "."
            ]
        );
        assert_eq!(
            texts("Ich habe die -Aufgabe bestanden."),
            vec![
                "Ich",
                " ",
                "habe",
                " ",
                "die",
                " ",
                "-Aufgabe",
                " ",
                "bestanden",
                "."
            ]
        );
        assert_eq!(
            texts("ein- oder mehrmals"),
            vec!["ein-", " ", "oder", " ", "mehrmals"]
        );
    }

    #[test]
    fn quotes_and_euro_glued_to_words() {
        assert_eq!(
            texts("Du mit deiner „Geht nicht“-Einstellung!"),
            vec![
                "Du",
                " ",
                "mit",
                " ",
                "deiner",
                " ",
                "„",
                "Geht",
                " ",
                "nicht",
                "“",
                "-Einstellung",
                "!"
            ]
        );
        assert_eq!(
            texts("Ich kaufe mir ein 9€ Ticket."),
            vec![
                "Ich", " ", "kaufe", " ", "mir", " ", "ein", " ", "9€", " ", "Ticket", "."
            ]
        );
    }

    #[test]
    fn underscores_and_apostrophes_split() {
        assert_eq!(
            texts("Porto_und_Spenden"),
            vec!["Porto", "_", "und", "_", "Spenden"]
        );
        assert_eq!(texts("geht's"), vec!["geht", "'", "s"]);
    }

    #[test]
    fn urls_stay_whole() {
        assert_eq!(
            texts("Siehe https://example.org/a-b, ok"),
            vec!["Siehe", " ", "https://example.org/a-b,", " ", "ok"]
        );
    }

    #[test]
    fn wwww_prefix_does_not_start_a_url_mid_word() {
        assert_eq!(
            texts("wwww.example.org"),
            vec!["wwww", ".", "example", ".", "org"]
        );
        assert_eq!(
            texts("9€www.example.org"),
            vec!["9€www", ".", "example", ".", "org"]
        );
    }

    #[test]
    fn emails_stay_whole_and_trailing_punctuation_splits() {
        assert_eq!(
            texts("dev.all@languagetool.org"),
            vec!["dev.all@languagetool.org"]
        );
        assert_eq!(
            texts("dev.all@languagetool.org."),
            vec!["dev.all@languagetool.org", "."]
        );
        assert_eq!(
            texts("dev.all@languagetool.org:"),
            vec!["dev.all@languagetool.org", ":"]
        );
        assert_eq!(
            texts("Schreiben Sie Hr. Meier (meier@mail.com)."),
            vec![
                "Schreiben",
                " ",
                "Sie",
                " ",
                "Hr",
                ".",
                " ",
                "Meier",
                " ",
                "(",
                "meier@mail.com",
                ")",
                "."
            ]
        );
    }

    #[test]
    fn single_label_tld_addresses_are_not_joined() {
        assert_eq!(texts("f@t.t"), vec!["f@t", ".", "t"]);
    }

    #[test]
    fn default_matches_new() {
        let via_default: Vec<String> = GermanWordTokenizer::default()
            .tokenize("ein- oder mehrmals")
            .into_iter()
            .map(|t| t.text().to_string())
            .collect();
        assert_eq!(via_default, vec!["ein-", " ", "oder", " ", "mehrmals"]);
    }

    #[test]
    fn offsets_are_contiguous() {
        let tokens = GermanWordTokenizer::new().tokenize("a- b „c“-d");
        let mut pos = 0;
        for t in &tokens {
            assert_eq!(t.start(), pos);
            assert_eq!(t.end() - t.start(), t.text().len());
            pos = t.end();
        }
        assert_eq!(pos, "a- b „c“-d".len());
    }
}
