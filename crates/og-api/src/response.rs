use serde::Serialize;
use og_core::CheckResult;

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
