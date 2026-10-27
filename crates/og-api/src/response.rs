use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct LanguagesResponse {
    pub name: String,
    pub code: String,
    #[serde(rename = "longCode")]
    pub long_code: String,
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub name: String,
    pub version: String,
    #[serde(rename = "apiVersion")]
    pub api_version: i32,
}

#[derive(Debug, Serialize)]
pub struct InfoResponse {
    pub name: String,
    pub version: String,
    #[serde(rename = "apiVersion")]
    pub api_version: i32,
    #[serde(rename = "maxTextLength")]
    pub max_text_length: usize,
    pub premium: bool,
}

#[derive(Debug, Serialize)]
pub struct MaxTextLengthResponse {
    #[serde(rename = "maxTextLength")]
    pub max_text_length: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::MAX_TEXT_LENGTH;

    #[test]
    fn languages_response_renames_long_code() {
        let json = serde_json::to_value(LanguagesResponse {
            name: "English (US)".to_string(),
            code: "en".to_string(),
            long_code: "en-US".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"name": "English (US)", "code": "en", "longCode": "en-US"})
        );
    }

    #[test]
    fn version_response_renames_api_version() {
        let json = serde_json::to_value(VersionResponse {
            name: "OpenGrammar".to_string(),
            version: "0.1.0".to_string(),
            api_version: 2,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"name": "OpenGrammar", "version": "0.1.0", "apiVersion": 2})
        );
    }

    #[test]
    fn info_response_renames_api_version_and_max_text_length() {
        let json = serde_json::to_value(InfoResponse {
            name: "OpenGrammar".to_string(),
            version: "0.1.0".to_string(),
            api_version: 2,
            max_text_length: MAX_TEXT_LENGTH,
            premium: false,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "name": "OpenGrammar",
                "version": "0.1.0",
                "apiVersion": 2,
                "maxTextLength": MAX_TEXT_LENGTH,
                "premium": false,
            })
        );
    }

    #[test]
    fn max_text_length_response_renames_its_field() {
        let json = serde_json::to_value(MaxTextLengthResponse {
            max_text_length: MAX_TEXT_LENGTH,
        })
        .unwrap();
        assert_eq!(json, serde_json::json!({"maxTextLength": MAX_TEXT_LENGTH}));
    }
}
