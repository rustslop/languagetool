use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LanguageCode(String);

impl LanguageCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for LanguageCode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    EnglishUs,
    EnglishGb,
    EnglishAu,
    EnglishCa,
    EnglishNz,
    EnglishZa,
    German,
    GermanDe,
    GermanAt,
    GermanCh,
    French,
    Spanish,
    Portuguese,
    PortugueseBr,
    PortuguesePt,
    PortugueseAo,
    PortugueseMz,
    Dutch,
    DutchBe,
    Polish,
    Ukrainian,
    Arabic,
    Russian,
    Italian,
    Catalan,
    Chinese,
    Japanese,
    Danish,
    DutchNl,
    Asturian,
    Belarusian,
    Breton,
    Esperanto,
    Galician,
    Greek,
    Icelandic,
    Khmer,
    Lithuanian,
    Malayalam,
    Romanian,
    Slovak,
    Slovenian,
    Serbian,
    Swedish,
    Tamil,
    Tagalog,
    CrimeanTatar,
    Irish,
    Persian,
    SimpleGerman,
    Other(String),
}

impl Language {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" | "en-US" if code == "en" => Some(Language::English),
            "en-US" => Some(Language::EnglishUs),
            "en-GB" => Some(Language::EnglishGb),
            "en-AU" => Some(Language::EnglishAu),
            "en-CA" => Some(Language::EnglishCa),
            "en-NZ" => Some(Language::EnglishNz),
            "en-ZA" => Some(Language::EnglishZa),
            "de" => Some(Language::German),
            "de-DE" => Some(Language::GermanDe),
            "de-AT" => Some(Language::GermanAt),
            "de-CH" => Some(Language::GermanCh),
            "fr" => Some(Language::French),
            "es" => Some(Language::Spanish),
            "pt" => Some(Language::Portuguese),
            "pt-BR" => Some(Language::PortugueseBr),
            "pt-PT" => Some(Language::PortuguesePt),
            "pt-AO" => Some(Language::PortugueseAo),
            "pt-MZ" => Some(Language::PortugueseMz),
            "nl" => Some(Language::Dutch),
            "nl-BE" => Some(Language::DutchBe),
            "nl-NL" => Some(Language::DutchNl),
            "pl" => Some(Language::Polish),
            "uk" => Some(Language::Ukrainian),
            "ar" => Some(Language::Arabic),
            "ru" => Some(Language::Russian),
            "it" => Some(Language::Italian),
            "ca" => Some(Language::Catalan),
            "zh" => Some(Language::Chinese),
            "ja" => Some(Language::Japanese),
            "da" => Some(Language::Danish),
            "ast" => Some(Language::Asturian),
            "be" => Some(Language::Belarusian),
            "br" => Some(Language::Breton),
            "eo" => Some(Language::Esperanto),
            "gl" => Some(Language::Galician),
            "el" => Some(Language::Greek),
            "is" => Some(Language::Icelandic),
            "km" => Some(Language::Khmer),
            "lt" => Some(Language::Lithuanian),
            "ml" => Some(Language::Malayalam),
            "ro" => Some(Language::Romanian),
            "sk" => Some(Language::Slovak),
            "sl" => Some(Language::Slovenian),
            "sr" => Some(Language::Serbian),
            "sv" => Some(Language::Swedish),
            "ta" => Some(Language::Tamil),
            "tl" => Some(Language::Tagalog),
            "crh" => Some(Language::CrimeanTatar),
            "ga" => Some(Language::Irish),
            "fa" => Some(Language::Persian),
            "de-DE-x-simple-language" => Some(Language::SimpleGerman),
            _ => None,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Language::English | Language::EnglishUs => "en-US",
            Language::EnglishGb => "en-GB",
            Language::EnglishAu => "en-AU",
            Language::EnglishCa => "en-CA",
            Language::EnglishNz => "en-NZ",
            Language::EnglishZa => "en-ZA",
            Language::German | Language::GermanDe => "de-DE",
            Language::GermanAt => "de-AT",
            Language::GermanCh => "de-CH",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::Portuguese => "pt",
            Language::PortugueseBr => "pt-BR",
            Language::PortuguesePt => "pt-PT",
            Language::PortugueseAo => "pt-AO",
            Language::PortugueseMz => "pt-MZ",
            Language::Dutch | Language::DutchNl => "nl",
            Language::DutchBe => "nl-BE",
            Language::Polish => "pl",
            Language::Ukrainian => "uk",
            Language::Arabic => "ar",
            Language::Russian => "ru",
            Language::Italian => "it",
            Language::Catalan => "ca",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Danish => "da",
            Language::Asturian => "ast",
            Language::Belarusian => "be",
            Language::Breton => "br",
            Language::Esperanto => "eo",
            Language::Galician => "gl",
            Language::Greek => "el",
            Language::Icelandic => "is",
            Language::Khmer => "km",
            Language::Lithuanian => "lt",
            Language::Malayalam => "ml",
            Language::Romanian => "ro",
            Language::Slovak => "sk",
            Language::Slovenian => "sl",
            Language::Serbian => "sr",
            Language::Swedish => "sv",
            Language::Tamil => "ta",
            Language::Tagalog => "tl",
            Language::CrimeanTatar => "crh",
            Language::Irish => "ga",
            Language::Persian => "fa",
            Language::SimpleGerman => "de-DE-x-simple-language",
            Language::Other(s) => s.as_str(),
        }
    }

    pub fn short_code(&self) -> &str {
        match self {
            Language::English
            | Language::EnglishUs
            | Language::EnglishGb
            | Language::EnglishAu
            | Language::EnglishCa
            | Language::EnglishNz
            | Language::EnglishZa => "en",
            Language::German | Language::GermanDe | Language::GermanAt | Language::GermanCh => "de",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::Portuguese
            | Language::PortugueseBr
            | Language::PortuguesePt
            | Language::PortugueseAo
            | Language::PortugueseMz => "pt",
            Language::Dutch | Language::DutchBe | Language::DutchNl => "nl",
            Language::Polish => "pl",
            Language::Ukrainian => "uk",
            Language::Arabic => "ar",
            Language::Russian => "ru",
            Language::Italian => "it",
            Language::Catalan => "ca",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Danish => "da",
            Language::Asturian => "ast",
            Language::Belarusian => "be",
            Language::Breton => "br",
            Language::Esperanto => "eo",
            Language::Galician => "gl",
            Language::Greek => "el",
            Language::Icelandic => "is",
            Language::Khmer => "km",
            Language::Lithuanian => "lt",
            Language::Malayalam => "ml",
            Language::Romanian => "ro",
            Language::Slovak => "sk",
            Language::Slovenian => "sl",
            Language::Serbian => "sr",
            Language::Swedish => "sv",
            Language::Tamil => "ta",
            Language::Tagalog => "tl",
            Language::CrimeanTatar => "crh",
            Language::Irish => "ga",
            Language::Persian => "fa",
            Language::SimpleGerman => "de",
            Language::Other(s) => s.split('-').next().unwrap_or(s.as_str()),
        }
    }

    pub fn base_language(&self) -> Language {
        match self {
            Language::English
            | Language::EnglishUs
            | Language::EnglishGb
            | Language::EnglishAu
            | Language::EnglishCa
            | Language::EnglishNz
            | Language::EnglishZa => Language::English,
            Language::German | Language::GermanDe | Language::GermanAt | Language::GermanCh => {
                Language::German
            }
            Language::Portuguese
            | Language::PortugueseBr
            | Language::PortuguesePt
            | Language::PortugueseAo
            | Language::PortugueseMz => Language::Portuguese,
            Language::Dutch | Language::DutchBe | Language::DutchNl => Language::Dutch,
            other => other.clone(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Language::English | Language::EnglishUs => "English (US)",
            Language::EnglishGb => "English (GB)",
            Language::EnglishAu => "English (Australian)",
            Language::EnglishCa => "English (Canadian)",
            Language::EnglishNz => "English (New Zealand)",
            Language::EnglishZa => "English (South African)",
            Language::German | Language::GermanDe => "German",
            Language::GermanAt => "German (Austria)",
            Language::GermanCh => "German (Swiss)",
            Language::French => "French",
            Language::Spanish => "Spanish",
            Language::Portuguese => "Portuguese",
            Language::PortugueseBr => "Portuguese (Brazil)",
            Language::PortuguesePt => "Portuguese (Portugal)",
            Language::PortugueseAo => "Portuguese (Angola preAO)",
            Language::PortugueseMz => "Portuguese (Moçambique preAO)",
            Language::Dutch | Language::DutchNl => "Dutch",
            Language::DutchBe => "Dutch (Belgian)",
            Language::Polish => "Polish",
            Language::Ukrainian => "Ukrainian",
            Language::Arabic => "Arabic",
            Language::Russian => "Russian",
            Language::Italian => "Italian",
            Language::Catalan => "Catalan",
            Language::Chinese => "Chinese",
            Language::Japanese => "Japanese",
            Language::Danish => "Danish",
            Language::Asturian => "Asturian",
            Language::Belarusian => "Belarusian",
            Language::Breton => "Breton",
            Language::Esperanto => "Esperanto",
            Language::Galician => "Galician",
            Language::Greek => "Greek",
            Language::Icelandic => "Icelandic",
            Language::Khmer => "Khmer",
            Language::Lithuanian => "Lithuanian",
            Language::Malayalam => "Malayalam",
            Language::Romanian => "Romanian",
            Language::Slovak => "Slovak",
            Language::Slovenian => "Slovenian",
            Language::Serbian => "Serbian",
            Language::Swedish => "Swedish",
            Language::Tamil => "Tamil",
            Language::Tagalog => "Tagalog",
            Language::CrimeanTatar => "Crimean Tatar",
            Language::Irish => "Irish",
            Language::Persian => "Persian",
            Language::SimpleGerman => "Simple German",
            Language::Other(s) => s.as_str(),
        }
    }

    pub fn all_languages() -> Vec<Language> {
        vec![
            Language::English,
            Language::EnglishUs,
            Language::EnglishGb,
            Language::EnglishAu,
            Language::EnglishCa,
            Language::EnglishNz,
            Language::EnglishZa,
            Language::German,
            Language::GermanDe,
            Language::GermanAt,
            Language::GermanCh,
            Language::French,
            Language::Spanish,
            Language::Portuguese,
            Language::PortugueseBr,
            Language::PortuguesePt,
            Language::PortugueseAo,
            Language::PortugueseMz,
            Language::Dutch,
            Language::DutchBe,
            Language::DutchNl,
            Language::Polish,
            Language::Ukrainian,
            Language::Arabic,
            Language::Russian,
            Language::Italian,
            Language::Catalan,
            Language::Chinese,
            Language::Japanese,
            Language::Danish,
            Language::Asturian,
            Language::Belarusian,
            Language::Breton,
            Language::Esperanto,
            Language::Galician,
            Language::Greek,
            Language::Icelandic,
            Language::Khmer,
            Language::Lithuanian,
            Language::Malayalam,
            Language::Romanian,
            Language::Slovak,
            Language::Slovenian,
            Language::Serbian,
            Language::Swedish,
            Language::Tamil,
            Language::Tagalog,
            Language::CrimeanTatar,
            Language::Irish,
            Language::Persian,
            Language::SimpleGerman,
        ]
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Language::from_code(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown language: {code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn from_code_maps_every_supported_code() {
        let cases = [
            ("en", Language::English),
            ("en-US", Language::EnglishUs),
            ("en-GB", Language::EnglishGb),
            ("en-AU", Language::EnglishAu),
            ("en-CA", Language::EnglishCa),
            ("en-NZ", Language::EnglishNz),
            ("en-ZA", Language::EnglishZa),
            ("de", Language::German),
            ("de-DE", Language::GermanDe),
            ("de-AT", Language::GermanAt),
            ("de-CH", Language::GermanCh),
            ("fr", Language::French),
            ("es", Language::Spanish),
            ("pt", Language::Portuguese),
            ("pt-BR", Language::PortugueseBr),
            ("pt-PT", Language::PortuguesePt),
            ("pt-AO", Language::PortugueseAo),
            ("pt-MZ", Language::PortugueseMz),
            ("nl", Language::Dutch),
            ("nl-BE", Language::DutchBe),
            ("nl-NL", Language::DutchNl),
            ("pl", Language::Polish),
            ("uk", Language::Ukrainian),
            ("ar", Language::Arabic),
            ("ru", Language::Russian),
            ("it", Language::Italian),
            ("ca", Language::Catalan),
            ("zh", Language::Chinese),
            ("ja", Language::Japanese),
            ("da", Language::Danish),
            ("ast", Language::Asturian),
            ("be", Language::Belarusian),
            ("br", Language::Breton),
            ("eo", Language::Esperanto),
            ("gl", Language::Galician),
            ("el", Language::Greek),
            ("is", Language::Icelandic),
            ("km", Language::Khmer),
            ("lt", Language::Lithuanian),
            ("ml", Language::Malayalam),
            ("ro", Language::Romanian),
            ("sk", Language::Slovak),
            ("sl", Language::Slovenian),
            ("sr", Language::Serbian),
            ("sv", Language::Swedish),
            ("ta", Language::Tamil),
            ("tl", Language::Tagalog),
            ("crh", Language::CrimeanTatar),
            ("ga", Language::Irish),
            ("fa", Language::Persian),
            ("de-DE-x-simple-language", Language::SimpleGerman),
        ];
        for (code, expected) in cases {
            assert_eq!(Language::from_code(code), Some(expected), "code {code}");
        }
    }

    #[test]
    fn from_code_rejects_unknown_and_case_mismatched_codes() {
        assert_eq!(Language::from_code("klingon"), None);
        assert_eq!(Language::from_code(""), None);
        assert_eq!(Language::from_code("EN"), None);
        assert_eq!(Language::from_code("en-us"), None);
        assert_eq!(Language::from_code("de-DE-x-simple"), None);
    }

    #[test]
    fn generic_variants_default_to_a_concrete_locale() {
        assert_eq!(Language::English.code(), "en-US");
        assert_eq!(Language::German.code(), "de-DE");
        assert_eq!(Language::Dutch.code(), "nl");
        assert_eq!(Language::Portuguese.code(), "pt");
        assert_eq!(Language::Other("xx-YY".to_string()).code(), "xx-YY");
    }

    #[test]
    fn short_code_collapses_regional_variants() {
        let cases = [
            (Language::English, "en"),
            (Language::EnglishGb, "en"),
            (Language::EnglishZa, "en"),
            (Language::German, "de"),
            (Language::GermanCh, "de"),
            (Language::PortugueseAo, "pt"),
            (Language::PortugueseMz, "pt"),
            (Language::DutchBe, "nl"),
            (Language::SimpleGerman, "de"),
            (Language::Other("xx-YY".to_string()), "xx"),
            (Language::Other("xx".to_string()), "xx"),
        ];
        for (language, expected) in cases {
            assert_eq!(language.short_code(), expected);
        }
    }

    #[test]
    fn base_language_collapses_only_multi_variant_families() {
        assert_eq!(Language::EnglishNz.base_language(), Language::English);
        assert_eq!(Language::EnglishUs.base_language(), Language::English);
        assert_eq!(Language::GermanCh.base_language(), Language::German);
        assert_eq!(Language::PortugueseMz.base_language(), Language::Portuguese);
        assert_eq!(Language::DutchBe.base_language(), Language::Dutch);
        assert_eq!(Language::French.base_language(), Language::French);
        assert_eq!(
            Language::SimpleGerman.base_language(),
            Language::SimpleGerman
        );
        assert_eq!(
            Language::Other("xx-YY".to_string()).base_language(),
            Language::Other("xx-YY".to_string())
        );
    }

    #[test]
    fn names_follow_java_language_display_names() {
        let cases = [
            (Language::English, "English (US)"),
            (Language::EnglishUs, "English (US)"),
            (Language::EnglishAu, "English (Australian)"),
            (Language::German, "German"),
            (Language::GermanAt, "German (Austria)"),
            (Language::GermanCh, "German (Swiss)"),
            (Language::PortugueseAo, "Portuguese (Angola preAO)"),
            (Language::PortugueseMz, "Portuguese (Moçambique preAO)"),
            (Language::DutchBe, "Dutch (Belgian)"),
            (Language::SimpleGerman, "Simple German"),
            (Language::CrimeanTatar, "Crimean Tatar"),
            (Language::Other("xx-YY".to_string()), "xx-YY"),
        ];
        for (language, expected) in cases {
            assert_eq!(language.name(), expected);
        }
    }

    #[test]
    fn every_known_language_resolves_back_from_its_code() {
        let aliases = [
            (Language::English, Language::EnglishUs),
            (Language::German, Language::GermanDe),
            (Language::DutchNl, Language::Dutch),
        ];
        for language in Language::all_languages() {
            let resolved = Language::from_code(language.code())
                .unwrap_or_else(|| panic!("code {} must resolve", language.code()));
            let expected = aliases
                .iter()
                .find(|(from, _)| *from == language)
                .map(|(_, to)| to.clone())
                .unwrap_or(language.clone());
            assert_eq!(resolved, expected, "code {}", language.code());
            assert!(!language.name().is_empty());
            assert!(!language.short_code().is_empty());
            assert!(language.code().starts_with(language.short_code()));
            assert_eq!(language.base_language().short_code(), language.short_code());
        }
    }

    #[test]
    fn all_languages_lists_the_core_and_variant_families() {
        let all = Language::all_languages();
        assert!(all.contains(&Language::English));
        assert!(all.contains(&Language::EnglishUs));
        assert!(all.contains(&Language::SimpleGerman));
        assert!(!all.contains(&Language::Other("xx".to_string())));
    }

    #[test]
    fn display_uses_the_full_code() {
        assert_eq!(Language::EnglishGb.to_string(), "en-GB");
        assert_eq!(
            Language::SimpleGerman.to_string(),
            "de-DE-x-simple-language"
        );
        assert_eq!(Language::Other("xx-YY".to_string()).to_string(), "xx-YY");
    }

    #[test]
    fn serializes_as_the_language_code() {
        assert_eq!(
            serde_json::to_string(&Language::English).unwrap(),
            r#""en-US""#
        );
        let back: Language = serde_json::from_str(r#""en-GB""#).unwrap();
        assert_eq!(back, Language::EnglishGb);
        let back: Language = serde_json::from_str(r#""de-DE-x-simple-language""#).unwrap();
        assert_eq!(back, Language::SimpleGerman);
    }

    #[test]
    fn deserializing_an_unknown_language_fails_with_the_code_in_the_error() {
        let err = serde_json::from_str::<Language>(r#""klingon""#).unwrap_err();
        assert!(err.to_string().contains("unknown language: klingon"));
    }

    #[test]
    fn language_code_new_display_and_from_str() {
        let code = LanguageCode::new("en-GB");
        assert_eq!(code.as_str(), "en-GB");
        assert_eq!(code.to_string(), "en-GB");
        assert_eq!(
            LanguageCode::from_str("fr").unwrap(),
            LanguageCode::new("fr")
        );
    }
}
