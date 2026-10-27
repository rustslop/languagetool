use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Determiner {
    A,
    An,
    AOrAn,
    Unknown,
}

pub struct ArticleData {
    requires_a: HashSet<String>,
    requires_an: HashSet<String>,
}

fn load_word_set(path: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(data) = std::fs::read_to_string(path) else {
        return set;
    };
    for raw in data.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(word) = line.strip_prefix('*') {
            set.insert(word.to_string());
        } else {
            set.insert(line.to_lowercase());
        }
    }
    set
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
}

fn is_all_uppercase(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn is_not_all_lowercase(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && !c.is_lowercase())
}

pub fn lowercase_first_char_if_capitalized(word: &str) -> String {
    if !is_capitalized_word(word) {
        return word.to_string();
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => word.to_string(),
    }
}

fn cleanup_word(word: &str) -> String {
    word.chars()
        .filter(|c| {
            c.is_ascii_alphanumeric()
                || *c == '.'
                || *c == ';'
                || *c == ','
                || *c == ':'
                || *c == '\''
                || *c == 'α'
        })
        .collect()
}

fn an_prefix_match(word: &str) -> bool {
    for prefix in ["unidentif", "unim", "unin"] {
        if let Some(rest) = word.strip_prefix(prefix) {
            return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphabetic());
        }
    }
    false
}

fn an_exception_prefix_match(word: &str) -> bool {
    for prefix in ["eu", "one", "uni"] {
        if let Some(rest) = word.strip_prefix(prefix) {
            return rest.chars().all(|c| c.is_ascii_alphabetic());
        }
    }
    if let Some(rest) = word.strip_prefix('u') {
        let mut chars = rest.chars();
        if let Some(r) = chars.next()
            && "rst".contains(r)
            && let Some(v) = chars.next()
            && "aeiou".contains(v)
            && chars.all(|c| c.is_ascii_alphabetic())
        {
            return true;
        }
    }
    false
}

impl ArticleData {
    pub fn from_rules_dir(rules_dir: &Path) -> Self {
        Self {
            requires_a: load_word_set(&rules_dir.join("det_a.txt")),
            requires_an: load_word_set(&rules_dir.join("det_an.txt")),
        }
    }

    pub fn empty() -> Self {
        Self {
            requires_a: HashSet::new(),
            requires_an: HashSet::new(),
        }
    }

    pub fn correct_determiner(&self, orig_word: &str) -> Determiner {
        let first_part = orig_word.split(['-', '\'']).next().unwrap_or(orig_word);
        let candidate = if first_part.eq_ignore_ascii_case("a") {
            orig_word
        } else {
            first_part
        };
        let cleaned = cleanup_word(candidate);
        if cleaned.is_empty() {
            return Determiner::Unknown;
        }
        let lower = cleaned.to_lowercase();
        let in_a = self.requires_a.contains(&lower) || self.requires_a.contains(&cleaned);
        let in_an = self.requires_an.contains(&lower) || self.requires_an.contains(&cleaned);
        if in_a && in_an {
            return Determiner::AOrAn;
        }
        if in_a {
            return Determiner::A;
        }
        if in_an {
            return Determiner::An;
        }
        if is_all_uppercase(&cleaned)
            || (is_not_all_lowercase(&cleaned) && !is_capitalized_word(&cleaned))
        {
            return Determiner::Unknown;
        }
        if an_prefix_match(&lower) {
            return Determiner::An;
        }
        let first_char = cleaned.chars().next().unwrap_or(' ');
        let is_vowel = matches!(first_char.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u');
        if is_vowel && !an_exception_prefix_match(&lower) {
            Determiner::An
        } else {
            Determiner::A
        }
    }

    pub fn suggest_a_or_an(&self, word: &str) -> String {
        match self.correct_determiner(word) {
            Determiner::A | Determiner::AOrAn => {
                format!("a {}", lowercase_first_char_if_capitalized(word))
            }
            Determiner::An => format!("an {}", lowercase_first_char_if_capitalized(word)),
            Determiner::Unknown => word.to_string(),
        }
    }
}

pub fn shared_article_data(rules_dir: &Path) -> Option<&'static ArticleData> {
    static CACHE: OnceLock<Option<ArticleData>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            rules_dir
                .join("det_a.txt")
                .is_file()
                .then(|| ArticleData::from_rules_dir(rules_dir))
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> ArticleData {
        ArticleData {
            requires_a: ["euphemism", "european", "unicorn", "one-way", "historical"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            requires_an: ["hour", "heir", "honest", "historical"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }

    #[test]
    fn vowel_words_take_an() {
        assert_eq!(data().suggest_a_or_an("apple"), "an apple");
        assert_eq!(data().suggest_a_or_an("Banana"), "a banana");
    }

    #[test]
    fn word_list_exceptions_win() {
        assert_eq!(data().correct_determiner("hour"), Determiner::An);
        assert_eq!(data().correct_determiner("unicorn"), Determiner::A);
        assert_eq!(data().correct_determiner("historical"), Determiner::AOrAn);
        assert_eq!(data().suggest_a_or_an("historical"), "a historical");
    }

    #[test]
    fn prefix_rules_apply() {
        assert_eq!(data().correct_determiner("unidentified"), Determiner::An);
        assert_eq!(data().correct_determiner("university"), Determiner::A);
        assert_eq!(data().correct_determiner("euphoric"), Determiner::A);
        assert_eq!(data().correct_determiner("one-time"), Determiner::A);
    }

    #[test]
    fn abbreviations_stay_unknown() {
        assert_eq!(data().correct_determiner("FBI"), Determiner::Unknown);
        assert_eq!(data().correct_determiner("iPhone"), Determiner::Unknown);
        assert_eq!(data().suggest_a_or_an("FBI"), "FBI");
    }

    #[test]
    fn capitalized_words_get_lowercase_article_word() {
        assert_eq!(data().suggest_a_or_an("Friend"), "a friend");
        assert_eq!(data().suggest_a_or_an("Hour"), "an hour");
        assert_eq!(data().suggest_a_or_an("FBI"), "FBI");
    }
    #[test]
    fn prefix_and_cleanup_helpers_match_java_string_tools() {
        assert!(an_prefix_match("unidentified"));
        assert!(an_prefix_match("unimaginable"));
        assert!(!an_prefix_match("unidentified1"));
        assert!(!an_prefix_match("unim"));
        assert!(!an_prefix_match("house"));
        assert!(an_exception_prefix_match("euphoric"));
        assert!(an_exception_prefix_match("onerous"));
        assert!(an_exception_prefix_match("unicorn"));
        assert!(an_exception_prefix_match("usual"));
        assert!(!an_exception_prefix_match("umbrella"));
        assert_eq!(cleanup_word("a-b+c!"), "abc");
        assert_eq!(cleanup_word("a.b;c,d:e'f"), "a.b;c,d:e'f");
    }

    #[test]
    fn shared_article_data_loads_det_a() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en",
        );
        let Some(data) = shared_article_data(dir) else {
            panic!("det_a.txt must load");
        };
        assert_eq!(data.suggest_a_or_an("hour"), "an hour");
        assert_eq!(data.suggest_a_or_an("unicorn"), "a unicorn");
    }
}
