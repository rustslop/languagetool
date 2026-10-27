use crate::types::XmlPattern;

#[derive(Debug, Clone, Default)]
pub struct DisambigRule {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pattern: XmlPattern,
    pub antipatterns: Vec<XmlPattern>,
    pub disambig: DisambigAction,
    pub unify: Option<UnifySpec>,
    pub filter_class: Option<String>,
    pub filter_args: Option<String>,
    pub untouched_examples: Vec<String>,
    pub examples: Vec<DisambigExample>,
}

#[derive(Debug, Clone, Default)]
pub struct DisambigExample {
    pub text: String,
    pub input_form: Option<String>,
    pub output_form: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifySpec {
    pub feature: String,
    pub features: Vec<String>,
    pub negate: bool,
    pub token_start: usize,
    pub token_end: usize,
    pub elem_start: usize,
    pub elem_end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct UnificationFeature {
    pub feature: String,
    pub equivalences: Vec<UnificationEquivalence>,
}

#[derive(Debug, Clone, Default)]
pub struct UnificationEquivalence {
    pub eq_type: String,
    pub postag: String,
    pub exact: bool,
}

#[derive(Debug, Clone, Default)]
pub enum DisambigAction {
    Replace(Vec<DisambigWord>),
    Remove(Vec<DisambigWord>),
    RemovePostag(String),
    Add(Vec<DisambigWord>),
    Filter {
        postag: String,
    },
    Match(DisambigMatch),
    #[default]
    FilterAll,
    IgnoreSpelling,
    Unify,
    Immunize,
    SetPos(String),
}

#[derive(Debug, Clone, Default)]
pub struct DisambigMatch {
    pub postag: Option<String>,
    pub postag_regexp: bool,
    pub postag_replace: Option<String>,
    pub lemma: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DisambigWord {
    pub pos: Option<String>,
    pub lemma: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct DisambigRuleSet {
    pub rules: Vec<DisambigRule>,
    pub unifications: Vec<UnificationFeature>,
}

#[derive(Debug, Clone)]
pub struct CompiledDisambigRule {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pattern: crate::compiler::CompiledPattern,
    pub antipatterns: Vec<crate::compiler::CompiledPattern>,
    pub disambig: DisambigAction,
    pub unify: Option<UnifySpec>,
    pub filter_class: Option<String>,
    pub filter_args: Option<String>,
    pub untouched_examples: Vec<String>,
    pub examples: Vec<DisambigExample>,
}
