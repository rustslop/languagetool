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
