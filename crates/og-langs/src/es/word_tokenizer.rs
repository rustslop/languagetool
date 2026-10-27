use og_core::Token;
use og_tokenizer::WordTokenizer;
use regex::Regex;
use std::sync::{Arc, OnceLock};

const DECIMAL_POINT_PLACEHOLDER: &str = "xxES_DECIMAL_POINTxx";
const DECIMAL_COMMA_PLACEHOLDER: &str = "xxES_DECIMAL_COMMAxx";
const ORDINAL_POINT_PLACEHOLDER: &str = "xxES_ORDINAL_POINTxx";

const CAMEL_CASE_HYPHEN_WORDS: [&str; 6] = [
    "mers-cov",
    "mcgraw-hill",
    "sars-cov-2",
    "sars-cov",
    "ph-metre",
    "ph-metres",
];

pub struct SpanishWordTokenizer {
    tagger: Arc<SpanishTaggerHandle>,
}

struct SpanishTaggerHandle {
    inner: Option<std::sync::Arc<crate::es::tagger::SpanishTagger>>,
}

impl SpanishTaggerHandle {
    fn is_word_tagged(&self, word: &str) -> bool {
        self.inner.as_ref().is_some_and(|t| t.is_word_tagged(word))
    }
}

const WORD_CHARS: &str = "\u{A7}\u{A9}@\u{20AC}\u{A3}$\\p{L}0-9\u{B7}\u{300}-\u{36F}\u{A8}\u{2070}-\u{209F}\u{B0}%\u{2030}\u{2031}&\u{FFFD}\u{AD}\u{AC}_-";

fn tokenizer_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!("[{WORD_CHARS}]+|[^{WORD_CHARS}]")).unwrap())
}

fn decimal_point_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("([0-9])\\.([0-9])").unwrap())
}

fn decimal_comma_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("([0-9]),([0-9])").unwrap())
}

fn ordinal_point_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b([0-9]+)\.(º|ª|o|a|er|os|as)\b").unwrap())
}

fn e_mail_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@((\[[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}])|(([a-zA-Z\-0-9]+\.)+[a-zA-Z]{2,}))").unwrap()
    })
}

fn domain_chars_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9-]+$").unwrap())
}

impl SpanishWordTokenizer {
    pub fn new() -> Self {
        Self {
            tagger: Arc::new(SpanishTaggerHandle {
                inner: crate::es::tagger::SpanishTagger::shared(),
            }),
        }
    }

    pub fn tokenize_strings(&self, text: &str) -> Vec<String> {
        let aux_text = text.replace(['\u{2010}', '\u{2011}'], "-");
        let aux_text = decimal_point_re()
            .replace_all(
                &aux_text,
                format!("${{1}}{DECIMAL_POINT_PLACEHOLDER}${{2}}"),
            )
            .into_owned();
        let aux_text = decimal_comma_re()
            .replace_all(
                &aux_text,
                format!("${{1}}{DECIMAL_COMMA_PLACEHOLDER}${{2}}"),
            )
            .into_owned();
        let aux_text = ordinal_point_re()
            .replace_all(
                &aux_text,
                format!("${{1}}{ORDINAL_POINT_PLACEHOLDER}${{2}}"),
            )
            .into_owned();

        let mut tokens: Vec<String> = Vec::new();
        for mat in tokenizer_pattern().find_iter(&aux_text) {
            let s = mat.as_str();
            if let Some(c) = s.chars().next()
                && s.chars().count() == 1
                && ('\u{FE00}'..='\u{FE0F}').contains(&c)
                && !tokens.is_empty()
            {
                if let Some(last) = tokens.last_mut() {
                    last.push_str(s);
                }
                continue;
            }
            let s = s
                .replace(DECIMAL_POINT_PLACEHOLDER, ".")
                .replace(DECIMAL_COMMA_PLACEHOLDER, ",")
                .replace(ORDINAL_POINT_PLACEHOLDER, ".");
            tokens.extend(self.words_to_add(&s));
        }
        join_emails_and_urls(tokens)
    }

    fn words_to_add(&self, s: &str) -> Vec<String> {
        if s.is_empty() {
            return Vec::new();
        }
        if !s.contains('-') {
            return vec![s.to_string()];
        }
        let lookup_form = s.replace('\u{AD}', "").replace('\u{2019}', "'");
        if self.tagger.is_word_tagged(&lookup_form)
            || CAMEL_CASE_HYPHEN_WORDS.contains(&s.to_lowercase().as_str())
        {
            return vec![s.to_string()];
        }
        let mut out = Vec::new();
        for (i, part) in s.split('-').enumerate() {
            if i > 0 {
                out.push("-".to_string());
            }
            if !part.is_empty() {
                out.push(part.to_string());
            }
        }
        out
    }
}

fn is_protocol(token: &str) -> bool {
    matches!(
        token,
        "http"
            | "https"
            | "ws"
            | "wss"
            | "ftp"
            | "ftps"
            | "sftp"
            | "file"
            | "mailto"
            | "tel"
            | "sms"
            | "git"
            | "ssh"
            | "data"
            | "magnet"
            | "smb"
            | "slack"
            | "spotify"
    )
}

fn join_emails(list: Vec<String>) -> Vec<String> {
    let text = list.concat();
    if !text.contains('@') {
        return list;
    }
    let mut out = Vec::new();
    let mut current_position = 0usize;
    let mut idx = 0usize;
    for mat in e_mail_re().find_iter(&text) {
        let (start, end) = (mat.start(), mat.end());
        while current_position < end {
            if current_position < start {
                out.push(list[idx].clone());
            } else if current_position == start {
                out.push(mat.as_str().to_string());
            }
            current_position += list[idx].len();
            idx += 1;
        }
    }
    if current_position < text.len() {
        out.extend(list[idx..].iter().cloned());
    }
    if out.is_empty() { list } else { out }
}

fn url_starts_at(i: usize, list: &[String]) -> bool {
    let token = list[i].as_str();
    if is_protocol(token)
        && list.len() > i + 3
        && list[i + 1] == ":"
        && list[i + 2] == "/"
        && list[i + 3] == "/"
    {
        return true;
    }
    if list.len() > i + 1 && token == "www" && list[i + 1] == "." {
        return true;
    }
    if list.len() > i + 3
        && list[i + 1] == "."
        && list[i + 3] == "/"
        && domain_chars_re().is_match(token)
        && domain_chars_re().is_match(&list[i + 2])
    {
        return true;
    }
    list.len() > i + 5
        && list[i + 1] == "."
        && list[i + 3] == "."
        && list[i + 5] == "/"
        && domain_chars_re().is_match(token)
        && domain_chars_re().is_match(&list[i + 2])
        && domain_chars_re().is_match(&list[i + 4])
}

fn url_chars_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Java's "[a-zA-ZÄÖÜäöü0-9/%$-_.+!*'(),?#~]" reads $-_. as the range 0x24..='_' (so ':' '=' '&' '@' match)
    RE.get_or_init(|| Regex::new(r"^[a-zA-ZÄÖÜäöü0-9/%$-_.+!*'(),?#~]+$").unwrap())
}

fn is_java_whitespace(token: &str) -> bool {
    token
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '\u{A0}' | '\u{2028}' | '\u{2029}'))
}

fn url_ends_at(i: usize, list: &[String], url_quote: &Option<&str>) -> bool {
    let token = list[i].as_str();
    if is_java_whitespace(token) || token == ")" || token == "]" {
        return true;
    }
    if list.len() > i + 1 {
        let next_token = list[i + 1].as_str();
        let next_boundary = is_java_whitespace(next_token)
            || matches!(
                next_token,
                "\"" | "»" | "«" | "‘" | "’" | "“" | "”" | "'" | "."
            );
        let token_punct = matches!(token, "." | "," | ";" | ":" | "!" | "?");
        if (next_boundary && (token_punct || Some(token) == *url_quote))
            || !url_chars_re().is_match(token)
        {
            return true;
        }
    } else if !url_chars_re().is_match(token) || token == "." || Some(token) == *url_quote {
        return true;
    }
    false
}

fn join_urls(list: Vec<String>) -> Vec<String> {
    let mut new_list = Vec::new();
    let mut in_url = false;
    let mut url = String::new();
    let mut url_quote: Option<String> = None;
    for (i, token) in list.iter().enumerate() {
        if url_starts_at(i, &list) && !in_url {
            in_url = true;
            if i > 0 {
                url_quote = Some(list[i - 1].clone());
            }
            url.push_str(token);
        } else if in_url && url_ends_at(i, &list, &url_quote.as_deref()) {
            in_url = false;
            url_quote = None;
            new_list.push(std::mem::take(&mut url));
            new_list.push(token.clone());
        } else if in_url {
            url.push_str(token);
        } else {
            new_list.push(token.clone());
        }
    }
    if !url.is_empty() {
        new_list.push(url);
    }
    new_list
}

fn join_emails_and_urls(list: Vec<String>) -> Vec<String> {
    join_urls(join_emails(list))
}

impl WordTokenizer for SpanishWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let strings = self.tokenize_strings(text);
        let mut tokens = Vec::with_capacity(strings.len());
        let mut offset = 0usize;
        for s in strings {
            let len = s.len();
            tokens.push(Token::new(s, offset, offset + len));
            offset += len;
        }
        tokens
    }
}

impl Default for SpanishWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenizer() -> SpanishWordTokenizer {
        SpanishWordTokenizer::new()
    }

    fn tokenized(text: &str) -> Vec<String> {
        tokenizer().tokenize_strings(text)
    }

    #[test]
    fn splits_leading_and_trailing_symbols() {
        assert_eq!(tokenized("*test+"), vec!["*", "test", "+"]);
    }

    #[test]
    fn keeps_dict_hyphen_words_and_splits_unknown_ones() {
        assert_eq!(
            tokenized("best-seller pret-a;sars-cov-2"),
            vec!["best-seller", " ", "pret", "-", "a", ";", "sars-cov-2"]
        );
        assert_eq!(tokenized("e-mails"), vec!["e-mails"]);
    }

    #[test]
    fn keeps_currency_and_decimals_whole() {
        assert_eq!(tokenized("$100"), vec!["$100"]);
        assert_eq!(tokenized("$1.000"), vec!["$1.000"]);
        assert_eq!(tokenized("$1,400.50"), vec!["$1,400.50"]);
        assert_eq!(tokenized("1,400.50$"), vec!["1,400.50$"]);
    }

    #[test]
    fn replaces_figure_dash_with_hyphen_minus() {
        assert_eq!(tokenized("Ven ‒dijo."), vec!["Ven", " ", "‒", "dijo", "."]);
    }

    #[test]
    fn keeps_ordinals_whole() {
        assert_eq!(tokenized("1.º").len(), 1);
        assert_eq!(tokenized("Es la 21.ª y el 45.º").len(), 11);
        assert_eq!(tokenized("Es la 21.a y el 45.o").len(), 11);
        assert_eq!(
            tokenized("11.as Jornadas de Estudio"),
            vec!["11.as", " ", "Jornadas", " ", "de", " ", "Estudio"]
        );
    }

    #[test]
    fn byte_offsets_partition_the_original_text() {
        let text = "Es la 21.ª y el 45.o con $1,400.50 en e-mails";
        let tokens = tokenizer().tokenize(text);
        let joined: String = tokens.iter().map(|t| t.text().to_string()).collect();
        assert_eq!(joined, text);
        assert_eq!(tokens.last().unwrap().end(), text.len());
    }

    #[test]
    fn joins_e_mail_addresses_into_one_token() {
        assert_eq!(
            tokenized("dev.all@languagetool.org"),
            vec!["dev.all@languagetool.org"]
        );
        assert_eq!(
            tokenized("dev.all@languagetool.org."),
            vec!["dev.all@languagetool.org", "."]
        );
        assert_eq!(
            tokenized("dev.all@languagetool.org:"),
            vec!["dev.all@languagetool.org", ":"]
        );
        assert_eq!(
            tokenized("Schreiben Sie Hr. Meier (meier@mail.com)."),
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
    fn joins_protocol_urls_and_stops_at_trailing_punctuation() {
        assert_eq!(
            tokenized("This http://foo.org blah"),
            vec!["This", " ", "http://foo.org", " ", "blah"]
        );
        assert_eq!(
            tokenized("This http://foo.org."),
            vec!["This", " ", "http://foo.org", "."]
        );
        assert_eq!(
            tokenized("This http://foo.org..."),
            vec!["This", " ", "http://foo.org", ".", ".", "."]
        );
        assert_eq!(
            tokenized("This http://foo.org and ftp://bla.com blah"),
            vec![
                "This",
                " ",
                "http://foo.org",
                " ",
                "and",
                " ",
                "ftp://bla.com",
                " ",
                "blah"
            ]
        );
    }

    #[test]
    fn keeps_ports_query_and_credentials_inside_the_url() {
        assert_eq!(
            tokenized("foo http://localhost:32000/?ch=1 bar"),
            vec!["foo", " ", "http://localhost:32000/?ch=1", " ", "bar"]
        );
        assert_eq!(
            tokenized("foo http://google.de/?aaa bar"),
            vec!["foo", " ", "http://google.de/?aaa", " ", "bar"]
        );
        assert_eq!(
            tokenized("foo http://www.youtube.com/watch?v=wDN_EYUvUq0 bar"),
            vec![
                "foo",
                " ",
                "http://www.youtube.com/watch?v=wDN_EYUvUq0",
                " ",
                "bar"
            ]
        );
        assert_eq!(
            tokenized(
                "foo https://joe:passwd@example.net:8080/index.html?action=x&session=A54C6FE2#info bar"
            ),
            vec![
                "foo",
                " ",
                "https://joe:passwd@example.net:8080/index.html?action=x&session=A54C6FE2#info",
                " ",
                "bar"
            ]
        );
    }

    #[test]
    fn joins_urls_wrapped_in_quotes_or_brackets() {
        assert_eq!(
            tokenized("This 'http://foo.org' blah"),
            vec!["This", " ", "'", "http://foo.org", "'", " ", "blah"]
        );
        assert_eq!(
            tokenized("This \"http://foo.org\" blah"),
            vec!["This", " ", "\"", "http://foo.org", "\"", " ", "blah"]
        );
        assert_eq!(
            tokenized("This (\"http://foo.org\") blah"),
            vec![
                "This",
                " ",
                "(",
                "\"",
                "http://foo.org",
                "\"",
                ")",
                " ",
                "blah"
            ]
        );
        assert_eq!(
            tokenized("foo (http://ex.net/p?a=x#i) bar"),
            vec!["foo", " ", "(", "http://ex.net/p?a=x#i", ")", " ", "bar"]
        );
        assert_eq!(
            tokenized("foo http://ex.net/p?a=x#i, bar"),
            vec!["foo", " ", "http://ex.net/p?a=x#i", ",", " ", "bar"]
        );
    }

    #[test]
    fn joins_bare_domains_only_when_a_slash_follows() {
        assert_eq!(
            tokenized("Get more at languagetool.org/foo, and via twitter"),
            vec![
                "Get",
                " ",
                "more",
                " ",
                "at",
                " ",
                "languagetool.org/foo",
                ",",
                " ",
                "and",
                " ",
                "via",
                " ",
                "twitter"
            ]
        );
        assert_eq!(
            tokenized("Get more at languagetool.org and via twitter"),
            vec![
                "Get",
                " ",
                "more",
                " ",
                "at",
                " ",
                "languagetool",
                ".",
                "org",
                " ",
                "and",
                " ",
                "via",
                " ",
                "twitter"
            ]
        );
        assert_eq!(
            tokenized("Get more at sub.languagetool.org/foo, and via twitter"),
            vec![
                "Get",
                " ",
                "more",
                " ",
                "at",
                " ",
                "sub.languagetool.org/foo",
                ",",
                " ",
                "and",
                " ",
                "via",
                " ",
                "twitter"
            ]
        );
        assert_eq!(
            tokenized("Visite www.example.org/hoy."),
            vec!["Visite", " ", "www.example.org/hoy", "."]
        );
    }

    #[test]
    fn joins_every_protocol_the_java_tokenizer_knows() {
        for protocol in [
            "ws", "wss", "ftps", "sftp", "file", "git", "ssh", "data", "magnet", "smb", "slack",
            "spotify",
        ] {
            let text = format!("foo {protocol}://example.net/x bar");
            assert_eq!(
                tokenized(&text),
                vec![
                    "foo".to_string(),
                    " ".to_string(),
                    format!("{protocol}://example.net/x"),
                    " ".to_string(),
                    "bar".to_string()
                ],
                "protocol {protocol}"
            );
        }
    }

    #[test]
    fn variation_selectors_attach_to_the_previous_token() {
        assert_eq!(
            tokenized("1\u{FE0F}\u{20E3} de junio"),
            vec!["1\u{FE0F}", "\u{20E3}", " ", "de", " ", "junio"]
        );
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(
            SpanishWordTokenizer::default().tokenize_strings("e-mails"),
            SpanishWordTokenizer::new().tokenize_strings("e-mails")
        );
    }
}
