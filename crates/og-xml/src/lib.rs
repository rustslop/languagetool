pub mod compiler;
pub mod disambig_parser;
pub mod disambig_types;
pub mod java_case_fold;
pub mod parser;
pub mod types;
pub mod unifier;

pub use compiler::*;
pub use disambig_parser::*;
pub use disambig_types::*;
pub use parser::*;
pub use types::*;

pub(crate) fn decode_attr_value(value: &[u8]) -> String {
    let raw = String::from_utf8_lossy(value);
    match quick_xml::escape::unescape(raw.as_ref()) {
        Ok(decoded) => decoded.into_owned(),
        Err(_) => raw.into_owned(),
    }
}
