use crate::types::XmlPattern;

/// A disambiguation rule from disambiguation.xml
#[derive(Debug, Clone, Default)]
pub struct DisambigRule {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pattern: XmlPattern,
    pub antipatterns: Vec<XmlPattern>,
    pub disambig: DisambigAction,
}

/// The action to take when a disambiguation rule matches
#[derive(Debug, Clone)]
pub enum DisambigAction {
    /// Replace all readings with the specified readings
    Replace(Vec<DisambigWord>),
    /// Remove readings matching the specified criteria
    Remove(Vec<DisambigWord>),
    /// Add readings to the token
    Add(Vec<DisambigWord>),
    /// Keep only readings matching the postag regex
    Filter { postag: String },
    /// Remove all readings (token becomes unknown)
    FilterAll,
    /// Mark token as "ignore spelling" (no spell check)
    IgnoreSpelling,
    /// Apply unification feature matching
    Unify,
    /// Shorthand: replace all readings with a single POS tag (e.g. <disambig postag="CD"/>)
    SetPos(String),
}

impl Default for DisambigAction {
    fn default() -> Self {
        DisambigAction::FilterAll
    }
}

/// A word specification in a disambiguation action
#[derive(Debug, Clone, Default)]
pub struct DisambigWord {
    pub pos: Option<String>,
    pub lemma: Option<String>,
}

/// A set of compiled disambiguation rules
#[derive(Debug, Clone, Default)]
pub struct DisambigRuleSet {
    pub rules: Vec<DisambigRule>,
}

/// A compiled disambiguation rule (pattern tokens are pre-compiled)
#[derive(Debug, Clone)]
pub struct CompiledDisambigRule {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pattern: crate::compiler::CompiledPattern,
    pub antipatterns: Vec<crate::compiler::CompiledPattern>,
    pub disambig: DisambigAction,
}
