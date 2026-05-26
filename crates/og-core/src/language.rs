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
            Language::Portuguese | Language::PortugueseBr => "pt-BR",
            Language::PortuguesePt => "pt-PT",
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

    pub fn base_language(&self) -> Language {
        match self {
            Language::English
            | Language::EnglishUs
            | Language::EnglishGb
            | Language::EnglishAu
            | Language::EnglishCa
            | Language::EnglishNz
            | Language::EnglishZa => Language::English,
            Language::German
            | Language::GermanDe
            | Language::GermanAt
            | Language::GermanCh => Language::German,
            Language::Portuguese
            | Language::PortugueseBr
            | Language::PortuguesePt => Language::Portuguese,
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
            Language::Portuguese | Language::PortugueseBr => "Portuguese (Brazil)",
            Language::PortuguesePt => "Portuguese (Portugal)",
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
        Language::from_code(&code).ok_or_else(|| serde::de::Error::custom(format!("unknown language: {code}")))
    }
}
