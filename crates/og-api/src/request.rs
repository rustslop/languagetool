use axum::body::Body;
use axum::extract::FromRequest;
use axum::http::{Method, Request};
use og_core::Language;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct CheckParams {
    pub text: Option<String>,
    pub data: Option<String>,
    pub language: String,
    #[serde(rename = "motherTongue")]
    pub mother_tongue: Option<String>,
    #[serde(rename = "enabledRules")]
    pub enabled_rules: Option<String>,
    #[serde(rename = "disabledRules")]
    pub disabled_rules: Option<String>,
    #[serde(rename = "enabledCategories")]
    pub enabled_categories: Option<String>,
    #[serde(rename = "disabledCategories")]
    pub disabled_categories: Option<String>,
    pub level: Option<String>,
    #[serde(default)]
    pub picky: bool,
}

impl CheckParams {
    pub fn get_text(&self) -> Option<&str> {
        self.text.as_deref().or(self.data.as_deref())
    }

    pub fn get_language(&self) -> Option<Language> {
        Language::from_code(&self.language)
    }

    pub fn get_enabled_rules(&self) -> Vec<String> {
        Self::split_list(self.enabled_rules.as_deref())
    }

    pub fn get_disabled_rules(&self) -> Vec<String> {
        Self::split_list(self.disabled_rules.as_deref())
    }

    pub fn get_enabled_categories(&self) -> Vec<String> {
        Self::split_list(self.enabled_categories.as_deref())
    }

    pub fn get_disabled_categories(&self) -> Vec<String> {
        Self::split_list(self.disabled_categories.as_deref())
    }

    fn split_list(value: Option<&str>) -> Vec<String> {
        value
            .map(|s| {
                s.split(',')
                    .map(|r| r.trim().to_string())
                    .filter(|r| !r.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn from_map(map: &HashMap<String, String>) -> Self {
        let get = |key: &str| map.get(key).cloned();
        Self {
            text: get("text"),
            data: get("data"),
            language: get("language").unwrap_or_default(),
            mother_tongue: get("motherTongue"),
            enabled_rules: get("enabledRules"),
            disabled_rules: get("disabledRules"),
            enabled_categories: get("enabledCategories"),
            disabled_categories: get("disabledCategories"),
            level: get("level"),
            picky: matches!(
                map.get("picky").map(String::as_str),
                Some("true") | Some("yes")
            ),
        }
    }
}

pub fn parse_urlencoded_into(map: &mut HashMap<String, String>, input: &[u8]) {
    for pair in input.split(|&b| b == b'&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.iter().position(|&b| b == b'=') {
            Some(pos) => (&pair[..pos], &pair[pos + 1..]),
            None => (pair, [].as_slice()),
        };
        map.insert(percent_decode(key), percent_decode(value));
    }
}

fn percent_decode(input: &[u8]) -> String {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        match input[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < input.len()
                && input[i + 1].is_ascii_hexdigit()
                && input[i + 2].is_ascii_hexdigit() =>
            {
                let hi = (input[i + 1] as char).to_digit(16).unwrap_or(0);
                let lo = (input[i + 2] as char).to_digit(16).unwrap_or(0);
                out.push((hi * 16 + lo) as u8);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct MergedCheckParams(pub CheckParams);

impl<S> FromRequest<S> for MergedCheckParams
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request(req: Request<Body>, _state: &S) -> Result<Self, Self::Rejection> {
        let (parts, body) = req.into_parts();
        let mut map = HashMap::new();
        if parts.method == Method::POST {
            let bytes = axum::body::to_bytes(body, crate::server::MAX_TEXT_LENGTH * 4 + 8192)
                .await
                .unwrap_or_default();
            parse_urlencoded_into(&mut map, &bytes);
        }
        if let Some(query) = parts.uri.query() {
            parse_urlencoded_into(&mut map, query.as_bytes());
        }
        Ok(MergedCheckParams(CheckParams::from_map(&map)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_plus_and_percent_sequences() {
        let mut map = HashMap::new();
        parse_urlencoded_into(&mut map, b"text=Hello+world&language=en-US&extra=a%20b%2Fc");
        assert_eq!(map.get("text").map(String::as_str), Some("Hello world"));
        assert_eq!(map.get("language").map(String::as_str), Some("en-US"));
        assert_eq!(map.get("extra").map(String::as_str), Some("a b/c"));
    }

    #[test]
    fn later_inserts_override_earlier_ones() {
        let mut map = HashMap::new();
        parse_urlencoded_into(&mut map, b"text=from+body");
        parse_urlencoded_into(&mut map, b"text=from+query");
        assert_eq!(map.get("text").map(String::as_str), Some("from query"));
    }

    #[test]
    fn keeps_lone_percent_and_valueless_keys() {
        let mut map = HashMap::new();
        parse_urlencoded_into(&mut map, b"flag&text=100%&x=%E2%82%AC");
        assert_eq!(map.get("flag").map(String::as_str), Some(""));
        assert_eq!(map.get("text").map(String::as_str), Some("100%"));
        assert_eq!(map.get("x").map(String::as_str), Some("€"));
    }

    #[test]
    fn builds_params_from_map() {
        let mut map = HashMap::new();
        map.insert("text".to_string(), "hi".to_string());
        map.insert("language".to_string(), "en-GB".to_string());
        map.insert("disabledRules".to_string(), "A,B".to_string());
        map.insert("picky".to_string(), "true".to_string());
        let params = CheckParams::from_map(&map);
        assert_eq!(params.get_text(), Some("hi"));
        assert_eq!(
            params.get_language().map(|l| l.code().to_string()),
            Some("en-GB".to_string())
        );
        assert_eq!(
            params.get_disabled_rules(),
            vec!["A".to_string(), "B".to_string()]
        );
        assert!(params.picky);
    }

    #[test]
    fn data_is_used_only_when_text_is_absent() {
        let mut map = HashMap::new();
        map.insert("data".to_string(), "from data".to_string());
        assert_eq!(CheckParams::from_map(&map).get_text(), Some("from data"));
        map.insert("text".to_string(), "from text".to_string());
        assert_eq!(CheckParams::from_map(&map).get_text(), Some("from text"));
    }

    #[test]
    fn missing_language_defaults_to_empty_and_parses_to_none() {
        let params = CheckParams::from_map(&HashMap::new());
        assert_eq!(params.language, "");
        assert!(params.get_language().is_none());
        assert_eq!(params.get_text(), None);
    }

    #[test]
    fn rule_and_category_lists_trim_and_drop_empty_items() {
        let mut map = HashMap::new();
        map.insert("enabledRules".to_string(), " A , B ,, ".to_string());
        map.insert("disabledRules".to_string(), "C".to_string());
        map.insert(
            "enabledCategories".to_string(),
            " GRAMMAR ,TYPOS ".to_string(),
        );
        map.insert("disabledCategories".to_string(), ",".to_string());
        let params = CheckParams::from_map(&map);
        assert_eq!(
            params.get_enabled_rules(),
            vec!["A".to_string(), "B".to_string()]
        );
        assert_eq!(params.get_disabled_rules(), vec!["C".to_string()]);
        assert_eq!(
            params.get_enabled_categories(),
            vec!["GRAMMAR".to_string(), "TYPOS".to_string()]
        );
        assert!(params.get_disabled_categories().is_empty());
    }

    #[test]
    fn absent_lists_stay_empty() {
        let params = CheckParams::from_map(&HashMap::new());
        assert!(params.get_enabled_rules().is_empty());
        assert!(params.get_disabled_rules().is_empty());
        assert!(params.get_enabled_categories().is_empty());
        assert!(params.get_disabled_categories().is_empty());
    }

    #[test]
    fn picky_accepts_only_true_and_yes() {
        let mut map = HashMap::new();
        assert!(!CheckParams::from_map(&map).picky);
        for value in ["1", "TRUE", "on", "yes "] {
            map.insert("picky".to_string(), value.to_string());
            assert!(!CheckParams::from_map(&map).picky, "picky={value}");
        }
        for value in ["true", "yes"] {
            map.insert("picky".to_string(), value.to_string());
            assert!(CheckParams::from_map(&map).picky, "picky={value}");
        }
    }

    #[test]
    fn deserializes_camel_case_fields() {
        let params: CheckParams = serde_json::from_str(
            r#"{"language":"en-US","enabledRules":"A,B","disabledCategories":"MISC",
                "motherTongue":"de","level":"picky","picky":true,"text":"hi"}"#,
        )
        .unwrap();
        assert_eq!(params.language, "en-US");
        assert_eq!(params.get_text(), Some("hi"));
        assert_eq!(params.mother_tongue.as_deref(), Some("de"));
        assert_eq!(params.level.as_deref(), Some("picky"));
        assert_eq!(
            params.get_enabled_rules(),
            vec!["A".to_string(), "B".to_string()]
        );
        assert_eq!(params.get_disabled_categories(), vec!["MISC".to_string()]);
        assert!(params.picky);
        assert!(params.data.is_none());
    }

    #[test]
    fn percent_decode_keeps_malformed_sequences_literally() {
        assert_eq!(percent_decode(b"100%"), "100%");
        assert_eq!(percent_decode(b"a%2b"), "a+");
        assert_eq!(percent_decode(b"%ZZ"), "%ZZ");
        assert_eq!(percent_decode(b"%C3%A4"), "ä");
        assert_eq!(percent_decode(b"+%41+"), " A ");
        assert_eq!(percent_decode(b""), "");
    }

    #[test]
    fn urlencoded_pairs_split_on_first_equals_only() {
        let mut map = HashMap::new();
        parse_urlencoded_into(&mut map, b"a=b=c&empty=&%6Bey=val&skipped&&x=1");
        assert_eq!(map.get("a").map(String::as_str), Some("b=c"));
        assert_eq!(map.get("empty").map(String::as_str), Some(""));
        assert_eq!(map.get("key").map(String::as_str), Some("val"));
        assert_eq!(map.get("x").map(String::as_str), Some("1"));
        assert_eq!(map.get("skipped").map(String::as_str), Some(""));
    }

    #[tokio::test]
    async fn merged_params_read_query_on_get() {
        let req = Request::builder()
            .uri("/v2/check?language=en-US&text=Hello+world")
            .body(Body::empty())
            .unwrap();
        let MergedCheckParams(params) = MergedCheckParams::from_request(req, &()).await.unwrap();
        assert_eq!(params.get_text(), Some("Hello world"));
        assert_eq!(
            params.get_language().map(|l| l.code().to_string()),
            Some("en-US".to_string())
        );
    }

    #[tokio::test]
    async fn merged_params_let_query_override_post_body() {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v2/check?text=from+query&level=picky")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("language=en-GB&text=from+body&picky=true"))
            .unwrap();
        let MergedCheckParams(params) = MergedCheckParams::from_request(req, &()).await.unwrap();
        assert_eq!(params.get_text(), Some("from query"));
        assert_eq!(
            params.get_language().map(|l| l.code().to_string()),
            Some("en-GB".to_string())
        );
        assert_eq!(params.level.as_deref(), Some("picky"));
        assert!(params.picky);
    }

    #[tokio::test]
    async fn oversized_post_body_is_dropped_but_query_survives() {
        let huge = "x".repeat(crate::server::MAX_TEXT_LENGTH * 4 + 8192 + 1);
        let req = Request::builder()
            .method(Method::POST)
            .uri("/v2/check?language=en-US&text=kept")
            .body(Body::from(huge))
            .unwrap();
        let MergedCheckParams(params) = MergedCheckParams::from_request(req, &()).await.unwrap();
        assert_eq!(params.get_text(), Some("kept"));
        assert_eq!(
            params.get_language().map(|l| l.code().to_string()),
            Some("en-US".to_string())
        );
    }
}
