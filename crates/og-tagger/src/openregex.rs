use og_core::AnalyzedSentence;
use og_core::AnalyzedTokenReadings;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhraseTag {
    Bi {
        start: &'static str,
        inner: &'static str,
    },
    Flat(&'static str),
}

impl PhraseTag {
    fn tag(self, at_start: bool) -> &'static str {
        match self {
            PhraseTag::Bi { start, inner } => {
                if at_start {
                    start
                } else {
                    inner
                }
            }
            PhraseTag::Flat(tag) => tag,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Leaf {
    CiStr(String),
    CiRegex(regex::Regex),
    CsRegex(regex::Regex),
    Chunk(regex::Regex),
    Pos(String),
    PosRegex(regex::Regex),
    Not(Box<Leaf>),
    And(Box<Leaf>, Box<Leaf>),
    Or(Box<Leaf>, Box<Leaf>),
}

fn anchored_cs(value: &str) -> regex::Regex {
    regex::Regex::new(&format!("^(?:{value})$"))
        .unwrap_or_else(|e| panic!("bad regex {value}: {e}"))
}

fn anchored_ci(value: &str) -> regex::Regex {
    regex::Regex::new(&format!("(?i)^(?:{value})$"))
        .unwrap_or_else(|e| panic!("bad regex {value}: {e}"))
}

fn unquote(s: &str) -> &str {
    if s.len() > 1 && s.starts_with('\'') && s.ends_with('\'') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn leaf_from_arg(arg: &str) -> Leaf {
    let parts: Vec<&str> = arg.split('=').collect();
    match parts.len() {
        1 => Leaf::CiStr(unquote(parts[0]).to_lowercase()),
        2 => {
            let value = unquote(parts[1]);
            match parts[0] {
                "string" => Leaf::CiStr(value.to_lowercase()),
                "regex" => Leaf::CiRegex(anchored_ci(value)),
                "regexCS" => Leaf::CsRegex(anchored_cs(value)),
                "chunk" => Leaf::Chunk(anchored_cs(value)),
                "pos" => Leaf::Pos(value.to_string()),
                "posre" | "posregex" => Leaf::PosRegex(anchored_cs(value)),
                other => panic!("Expression type not supported: '{other}'"),
            }
        }
        _ => panic!("Could not parse expression: {arg}"),
    }
}

#[derive(Clone, Debug)]
enum LogicTok {
    LParen,
    RParen,
    Not,
    And,
    Or,
    Arg(String),
}

fn logic_tokens(s: &str) -> Vec<LogicTok> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' => i += 1,
            '(' => {
                out.push(LogicTok::LParen);
                i += 1;
            }
            ')' => {
                out.push(LogicTok::RParen);
                i += 1;
            }
            '!' => {
                out.push(LogicTok::Not);
                i += 1;
            }
            '&' => {
                out.push(LogicTok::And);
                i += 1;
            }
            '|' => {
                out.push(LogicTok::Or);
                i += 1;
            }
            _ => {
                let mut depth = 0i32;
                let mut j = i;
                while j < chars.len() {
                    let c = chars[j];
                    if c == '\'' {
                        j += 1;
                        while j < chars.len() && chars[j] != '\'' {
                            j += 1;
                        }
                        j += 1;
                        continue;
                    }
                    if c == '(' {
                        depth += 1;
                    } else if c == ')' {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    } else if c == '&' || c == '|' {
                        break;
                    }
                    j += 1;
                }
                let arg: String = chars[i..j].iter().collect();
                let arg = arg.trim();
                if arg.is_empty() {
                    panic!("zero-length token found in logic expression: {s}");
                }
                out.push(LogicTok::Arg(arg.to_string()));
                i = j;
            }
        }
    }
    out
}

fn logic_precedence(tok: &LogicTok) -> Option<u8> {
    match tok {
        LogicTok::Not => Some(3),
        LogicTok::And => Some(2),
        LogicTok::Or => Some(1),
        _ => None,
    }
}

fn parse_leaf(body: &str) -> Leaf {
    let toks = logic_tokens(body);
    let mut rpn: Vec<LogicTok> = Vec::new();
    let mut stack: Vec<LogicTok> = Vec::new();
    for tok in toks {
        match tok {
            LogicTok::LParen => stack.push(tok),
            LogicTok::RParen => loop {
                let top = stack.pop();
                match top {
                    Some(LogicTok::LParen) | None => break,
                    Some(other) => rpn.push(other),
                }
            },
            ref op @ (LogicTok::Not | LogicTok::And | LogicTok::Or) => {
                let prec = logic_precedence(op).unwrap_or(0);
                while let Some(top) = stack.last() {
                    match logic_precedence(top) {
                        Some(tp) if tp >= prec => {
                            rpn.push(stack.pop().unwrap());
                        }
                        _ => break,
                    }
                }
                stack.push(op.clone());
            }
            arg => rpn.push(arg),
        }
    }
    while let Some(top) = stack.pop() {
        if matches!(top, LogicTok::LParen) {
            panic!("Unbalanced parentheses in {body}");
        }
        rpn.push(top);
    }
    let mut ast: Vec<Leaf> = Vec::new();
    for tok in rpn {
        match tok {
            LogicTok::Arg(a) => ast.push(leaf_from_arg(&a)),
            LogicTok::Not => {
                let sub = ast
                    .pop()
                    .unwrap_or_else(|| panic!("no argument for ! in {body}"));
                ast.push(Leaf::Not(Box::new(sub)));
            }
            LogicTok::And => {
                let r = ast
                    .pop()
                    .unwrap_or_else(|| panic!("no argument for & in {body}"));
                let l = ast
                    .pop()
                    .unwrap_or_else(|| panic!("no argument for & in {body}"));
                ast.push(Leaf::And(Box::new(l), Box::new(r)));
            }
            LogicTok::Or => {
                let r = ast
                    .pop()
                    .unwrap_or_else(|| panic!("no argument for | in {body}"));
                let l = ast
                    .pop()
                    .unwrap_or_else(|| panic!("no argument for | in {body}"));
                ast.push(Leaf::Or(Box::new(l), Box::new(r)));
            }
            LogicTok::LParen | LogicTok::RParen => panic!("unbalanced parentheses in {body}"),
        }
    }
    ast.pop()
        .unwrap_or_else(|| panic!("empty logic expression: {body}"))
}

#[derive(Debug)]
enum Expr {
    Leaf(Leaf),
    Seq(Vec<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Rep {
        inner: Box<Expr>,
        min: u32,
        max: Option<u32>,
    },
}

fn index_of_close(chars: &[char], start: usize, open: char, close: char) -> usize {
    let mut count = 0i32;
    let mut i = start;
    loop {
        if i >= chars.len() {
            panic!("non-matching brackets {open}{close} at {start}");
        }
        let c = chars[i];
        if c == open {
            count += 1;
        } else if c == close {
            count -= 1;
            if count == 0 {
                return i;
            }
        }
        i += 1;
    }
}

fn take_last(exprs: &mut Vec<Expr>) -> Expr {
    exprs
        .pop()
        .unwrap_or_else(|| panic!("operator without operand"))
}

fn parse_regex_expr(s: &str) -> Expr {
    let chars: Vec<char> = s.chars().collect();
    let mut exprs: Vec<Expr> = Vec::new();
    let mut or_pending = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                let close = index_of_close(&chars, i, '(', ')');
                let mut content: String = chars[i + 1..close].iter().collect();
                if let Some(rest) = content.strip_prefix("?:") {
                    content = rest.to_string();
                } else if content.starts_with('<')
                    && let Some(pos) = content.find(">:")
                {
                    let name = &content[1..pos];
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        content = content[pos + 2..].to_string();
                    }
                }
                let group = parse_regex_expr(&content);
                i = close + 1;
                let group = wrap_quantifier_at(&chars, &mut i, group);
                exprs.push(group);
                resolve_or(&mut exprs, &mut or_pending);
            }
            '<' | '[' => {
                let close_char = if c == '<' { '>' } else { ']' };
                let close = index_of_close(&chars, i, c, close_char);
                let body: String = chars[i + 1..close].iter().collect();
                let leaf = parse_leaf(&body);
                i = close + 1;
                let expr = wrap_quantifier_at(&chars, &mut i, Expr::Leaf(leaf));
                exprs.push(expr);
                resolve_or(&mut exprs, &mut or_pending);
            }
            '*' | '?' | '+' => {
                let last = take_last(&mut exprs);
                let rep = match c {
                    '?' => (0u32, Some(1u32)),
                    '*' => (0, None),
                    _ => (1, None),
                };
                exprs.push(Expr::Rep {
                    inner: Box::new(last),
                    min: rep.0,
                    max: rep.1,
                });
                i += 1;
            }
            '{' => {
                let mut j = i + 1;
                let mut min_s = String::new();
                while j < chars.len() && chars[j].is_ascii_digit() {
                    min_s.push(chars[j]);
                    j += 1;
                }
                if min_s.is_empty() || j >= chars.len() || chars[j] != ',' {
                    panic!("bad minmax quantifier at {i} in {s}");
                }
                j += 1;
                let mut max_s = String::new();
                while j < chars.len() && chars[j].is_ascii_digit() {
                    max_s.push(chars[j]);
                    j += 1;
                }
                if max_s.is_empty() || j >= chars.len() || chars[j] != '}' {
                    panic!("bad minmax quantifier at {i} in {s}");
                }
                j += 1;
                let last = take_last(&mut exprs);
                exprs.push(Expr::Rep {
                    inner: Box::new(last),
                    min: min_s.parse().unwrap_or(0),
                    max: Some(max_s.parse().unwrap_or(0)),
                });
                i = j;
            }
            '|' => {
                or_pending = true;
                i += 1;
            }
            '^' | '$' => {
                panic!("assertions not supported in chunker expressions: {s}");
            }
            other => panic!("unknown symbol {other} in {s}"),
        }
    }
    if or_pending {
        panic!("OR remains on the stack: {s}");
    }
    Expr::Seq(exprs)
}

fn wrap_quantifier_at(chars: &[char], i: &mut usize, mut expr: Expr) -> Expr {
    let (min, max) = match chars.get(*i) {
        Some('?') => (0u32, Some(1u32)),
        Some('*') => (0, None),
        Some('+') => (1, None),
        Some('{') => {
            let mut j = *i + 1;
            let mut min_s = String::new();
            while j < chars.len() && chars[j].is_ascii_digit() {
                min_s.push(chars[j]);
                j += 1;
            }
            let mut max_s = String::new();
            if j < chars.len() && chars[j] == ',' {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    max_s.push(chars[j]);
                    j += 1;
                }
            }
            if max_s.is_empty() || j >= chars.len() || chars[j] != '}' {
                panic!("bad minmax quantifier at {i}");
            }
            *i = j + 1;
            return Expr::Rep {
                inner: Box::new(expr),
                min: min_s.parse().unwrap_or(0),
                max: Some(max_s.parse().unwrap_or(0)),
            };
        }
        _ => return expr,
    };
    *i += 1;
    expr = Expr::Rep {
        inner: Box::new(expr),
        min,
        max,
    };
    expr
}

fn resolve_or(exprs: &mut Vec<Expr>, or_pending: &mut bool) {
    if !*or_pending {
        return;
    }
    *or_pending = false;
    let a = take_last(exprs);
    let b = take_last(exprs);
    exprs.push(Expr::Or(Box::new(a), Box::new(b)));
}

struct NfaBuilder {
    edges: Vec<Vec<(u32, Leaf)>>,
    eps: Vec<Vec<u32>>,
}

impl NfaBuilder {
    fn new() -> Self {
        Self {
            edges: Vec::new(),
            eps: Vec::new(),
        }
    }

    fn state(&mut self) -> u32 {
        self.edges.push(Vec::new());
        self.eps.push(Vec::new());
        (self.edges.len() - 1) as u32
    }

    fn edge(&mut self, a: u32, b: u32, leaf: Leaf) {
        self.edges[a as usize].push((b, leaf));
    }

    fn eps_edge(&mut self, a: u32, b: u32) {
        self.eps[a as usize].push(b);
    }

    fn compile(&mut self, expr: &Expr) -> (u32, u32) {
        match expr {
            Expr::Leaf(leaf) => {
                let s = self.state();
                let t = self.state();
                self.edge(s, t, leaf.clone());
                (s, t)
            }
            Expr::Seq(items) => {
                let s = self.state();
                let mut cur = s;
                for item in items {
                    let (a, b) = self.compile(item);
                    self.eps_edge(cur, a);
                    cur = b;
                }
                let t = self.state();
                self.eps_edge(cur, t);
                (s, t)
            }
            Expr::Or(a, b) => {
                let s = self.state();
                let t = self.state();
                let (as_, ae) = self.compile(a);
                let (bs, be) = self.compile(b);
                self.eps_edge(s, as_);
                self.eps_edge(s, bs);
                self.eps_edge(ae, t);
                self.eps_edge(be, t);
                (s, t)
            }
            Expr::Rep { inner, min, max } => {
                let s = self.state();
                let mut cur = s;
                for _ in 0..*min {
                    let (a, b) = self.compile(inner);
                    self.eps_edge(cur, a);
                    cur = b;
                }
                let t = self.state();
                match max {
                    None => {
                        let (a, b) = self.compile(inner);
                        self.eps_edge(cur, a);
                        self.eps_edge(b, t);
                        self.eps_edge(b, a);
                        self.eps_edge(cur, t);
                    }
                    Some(mx) => {
                        let mut stops = vec![cur];
                        for _ in *min..*mx {
                            let (a, b) = self.compile(inner);
                            self.eps_edge(cur, a);
                            self.eps_edge(cur, b);
                            cur = b;
                            stops.push(b);
                        }
                        for stop in stops {
                            self.eps_edge(stop, t);
                        }
                    }
                }
                (s, t)
            }
        }
    }
}

struct Nfa {
    edges: Vec<Vec<(u32, Leaf)>>,
    eps: Vec<Vec<u32>>,
    start: u32,
    end: u32,
}

struct SimCtx {
    active: Vec<bool>,
    next: Vec<bool>,
    stack: Vec<usize>,
}

impl Nfa {
    fn closure(&self) -> Vec<Vec<u32>> {
        let n = self.edges.len();
        let mut closure = vec![Vec::new(); n];
        for s in 0..n {
            let mut seen = vec![false; n];
            let mut stack = vec![s];
            seen[s] = true;
            while let Some(cur) = stack.pop() {
                for &e in &self.eps[cur] {
                    let e = e as usize;
                    if !seen[e] {
                        seen[e] = true;
                        stack.push(e);
                    }
                }
            }
            closure[s] = (0..n).filter(|&x| seen[x]).map(|x| x as u32).collect();
        }
        closure
    }

    fn longest_match(
        &self,
        tokens: &[Token],
        start: usize,
        closure: &[Vec<u32>],
        ctx: &mut SimCtx,
    ) -> Option<usize> {
        let n = self.edges.len();
        ctx.active.iter_mut().for_each(|b| *b = false);
        for &c in &closure[self.start as usize] {
            ctx.active[c as usize] = true;
        }
        let mut best = None;
        let mut pos = start;
        loop {
            if pos > start && ctx.active[self.end as usize] {
                best = Some(pos);
            }
            if pos >= tokens.len() {
                break;
            }
            let tok = &tokens[pos];
            ctx.next.iter_mut().for_each(|b| *b = false);
            let mut any = false;
            for s in 0..n {
                if !ctx.active[s] {
                    continue;
                }
                for (to, leaf) in &self.edges[s] {
                    let to = *to as usize;
                    if !ctx.next[to] && leaf.matches(tok) {
                        ctx.next[to] = true;
                        any = true;
                    }
                }
            }
            if !any {
                break;
            }
            ctx.stack.clear();
            ctx.stack.extend((0..n).filter(|&s| ctx.next[s]));
            while let Some(s) = ctx.stack.pop() {
                for &e in &self.eps[s] {
                    let e = e as usize;
                    if !ctx.next[e] {
                        ctx.next[e] = true;
                        ctx.stack.push(e);
                    }
                }
            }
            std::mem::swap(&mut ctx.active, &mut ctx.next);
            pos += 1;
        }
        best
    }
}

pub(crate) struct Rule {
    nfa: Nfa,
    closure: Vec<Vec<u32>>,
    phrase: PhraseTag,
    overwrite: bool,
    filter_tags: &'static [&'static str],
    form_hints: Vec<Vec<String>>,
}

pub(crate) struct Token<'a> {
    reading: &'a AnalyzedTokenReadings,
    pub(crate) chunks: Vec<String>,
    text: &'a str,
    lower: String,
}

impl<'a> Token<'a> {
    pub(crate) fn new(reading: &'a AnalyzedTokenReadings, chunks: Vec<String>) -> Self {
        let text = reading.token().token();
        Self {
            reading,
            chunks,
            text,
            lower: text.to_lowercase(),
        }
    }
}

impl Leaf {
    fn matches(&self, token: &Token) -> bool {
        match self {
            Leaf::CiStr(v) => *v == token.lower,
            Leaf::CiRegex(re) => re.is_match(token.text),
            Leaf::CsRegex(re) => re.is_match(token.text),
            Leaf::Chunk(re) => token.chunks.iter().any(|c| re.is_match(c)),
            Leaf::Pos(v) => token
                .reading
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| t.contains(v.as_str()))),
            Leaf::PosRegex(re) => token
                .reading
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| re.is_match(t))),
            Leaf::Not(a) => !a.matches(token),
            Leaf::And(a, b) => a.matches(token) && b.matches(token),
            Leaf::Or(a, b) => a.matches(token) || b.matches(token),
        }
    }
}

pub(crate) fn build_expanded(
    expr: &str,
    phrase: PhraseTag,
    overwrite: bool,
    filter_tags: &'static [&'static str],
    form_hints: Vec<Vec<String>>,
) -> Rule {
    let tree = parse_regex_expr(expr);
    let mut nb = NfaBuilder::new();
    let (start, end) = nb.compile(&tree);
    let nfa = Nfa {
        edges: nb.edges,
        eps: nb.eps,
        start,
        end,
    };
    let closure = nfa.closure();
    Rule {
        nfa,
        closure,
        phrase,
        overwrite,
        filter_tags,
        form_hints,
    }
}

pub(crate) fn apply(rule: &Rule, tokens: &mut [Token], forms: &HashSet<String>) {
    if rule
        .form_hints
        .iter()
        .any(|hints| !hints.iter().any(|h| forms.contains(h)))
    {
        return;
    }
    let mut ctx = SimCtx {
        active: vec![false; rule.nfa.edges.len()],
        next: vec![false; rule.nfa.edges.len()],
        stack: Vec::new(),
    };
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match rule.nfa.longest_match(tokens, i, &rule.closure, &mut ctx) {
            Some(end) => {
                spans.push((i, end));
                i = end;
            }
            None => i += 1,
        }
    }
    for &(s, e) in &spans {
        for (offset, token) in tokens.iter_mut().enumerate().take(e).skip(s) {
            let mut new_chunks = std::mem::take(&mut token.chunks);
            if rule.overwrite {
                new_chunks.retain(|c| !rule.filter_tags.contains(&c.as_str()));
            }
            let new_tag = rule.phrase.tag(offset == s);
            if !new_chunks.iter().any(|c| c == new_tag) {
                new_chunks.retain(|c| c != "O");
                new_chunks.push(new_tag.to_string());
            }
            token.chunks = new_chunks;
        }
    }
}

pub(crate) fn sentence_forms(sentence: &AnalyzedSentence) -> HashSet<String> {
    sentence
        .tokens()
        .iter()
        .map(|t| t.token().token().to_lowercase())
        .collect()
}

pub(crate) mod lru_cache {
    const MAX_ENTRIES: usize = 1024;

    pub struct BoundedMap {
        entries: std::collections::HashMap<String, Vec<Vec<String>>>,
        order: std::collections::VecDeque<String>,
    }

    impl BoundedMap {
        pub fn new() -> Self {
            Self {
                entries: std::collections::HashMap::new(),
                order: std::collections::VecDeque::new(),
            }
        }

        pub fn get(&mut self, key: &str) -> Option<&Vec<Vec<String>>> {
            if let Some(value) = self.entries.get(key)
                && let Some(pos) = self.order.iter().position(|k| k == key)
            {
                self.order.remove(pos);
                self.order.push_back(key.to_string());
                return Some(value);
            }
            None
        }

        pub fn put(&mut self, key: String, value: Vec<Vec<String>>) {
            if self.entries.len() >= MAX_ENTRIES
                && !self.entries.contains_key(&key)
                && let Some(evict) = self.order.pop_front()
            {
                self.entries.remove(&evict);
            }
            if !self.entries.contains_key(&key) {
                self.order.push_back(key.clone());
            }
            self.entries.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lru_cache::BoundedMap;
    use super::*;
    use og_core::AnalyzedToken;
    use og_core::AnalyzedTokenReadings;
    use std::collections::HashSet;

    fn reading(text: &str, tags: &[&str]) -> AnalyzedTokenReadings {
        let readings: Vec<AnalyzedToken> = tags
            .iter()
            .map(|t| AnalyzedToken::new(text, 0, 0).with_pos_tags(vec![t.to_string()]))
            .collect();
        let primary = AnalyzedToken::new(text, 0, 0)
            .with_pos_tags(tags.iter().map(|t| t.to_string()).collect());
        AnalyzedTokenReadings::new(primary).with_readings(readings)
    }

    fn chunks_after(expr: &str, parts: &[(&str, &[&str])]) -> Vec<String> {
        let rule = build_expanded(expr, PhraseTag::Flat("X"), false, &[], Vec::new());
        let readings: Vec<AnalyzedTokenReadings> =
            parts.iter().map(|(t, tags)| reading(t, tags)).collect();
        let forms: HashSet<String> = readings
            .iter()
            .map(|r| r.token().token().to_lowercase())
            .collect();
        let mut tokens: Vec<Token> = readings
            .iter()
            .map(|r| Token::new(r, vec!["O".to_string()]))
            .collect();
        apply(&rule, &mut tokens, &forms);
        tokens.iter().map(|t| t.chunks.join("|")).collect()
    }

    fn build(expr: &str) {
        build_expanded(expr, PhraseTag::Flat("X"), false, &[], Vec::new());
    }

    #[test]
    fn optional_atom_matches_with_and_without_the_atom() {
        assert_eq!(
            chunks_after(r"<pos=ART>? <pos=SUB>", &[("Haus", &["SUB"])]),
            vec!["X"]
        );
        assert_eq!(
            chunks_after(
                r"<pos=ART>? <pos=SUB>",
                &[("das", &["ART"]), ("Haus", &["SUB"])]
            ),
            vec!["X", "X"]
        );
        assert_eq!(
            chunks_after(r"<pos=ART>? <pos=SUB>", &[("das", &["ART"])]),
            vec!["O"]
        );
    }

    #[test]
    fn quantifier_after_space_still_applies_to_the_previous_atom() {
        assert_eq!(
            chunks_after(
                r"<pos=ART> ? <pos=SUB>",
                &[("das", &["ART"]), ("Haus", &["SUB"])]
            ),
            vec!["X", "X"]
        );
        assert_eq!(
            chunks_after(r"<pos=ART> ? <pos=SUB>", &[("Haus", &["SUB"])]),
            vec!["X"]
        );
        assert_eq!(
            chunks_after(
                r"<pos=ART> ? <pos=SUB>",
                &[("das", &["ART"]), ("schnell", &["ADV"])]
            ),
            vec!["O", "O"]
        );
    }

    #[test]
    fn minmax_quantifier_bounds_the_repetition() {
        assert_eq!(
            chunks_after(r"<pos=ADJ>{1,2} <pos=SUB>", &[("Haus", &["SUB"])]),
            vec!["O"]
        );
        assert_eq!(
            chunks_after(
                r"<pos=ADJ>{1,2} <pos=SUB>",
                &[("schöne", &["ADJ"]), ("Haus", &["SUB"])]
            ),
            vec!["X", "X"]
        );
        assert_eq!(
            chunks_after(
                r"<pos=ADJ>{1,2} <pos=SUB>",
                &[
                    ("a", &["ADJ"]),
                    ("b", &["ADJ"]),
                    ("c", &["ADJ"]),
                    ("Haus", &["SUB"])
                ]
            ),
            vec!["O", "X", "X", "X"]
        );
    }

    #[test]
    fn spaced_minmax_quantifier_binds_to_the_previous_atom() {
        assert_eq!(
            chunks_after(
                r"<pos=ADJ> {1,2} <pos=SUB>",
                &[
                    ("a", &["ADJ"]),
                    ("b", &["ADJ"]),
                    ("c", &["ADJ"]),
                    ("Haus", &["SUB"])
                ]
            ),
            vec!["O", "X", "X", "X"]
        );
        assert_eq!(
            chunks_after(r"<pos=ADJ> {1,2} <pos=SUB>", &[("Haus", &["SUB"])]),
            vec!["O"]
        );
    }

    #[test]
    #[should_panic(expected = "bad minmax quantifier")]
    fn minmax_quantifier_without_a_maximum_is_rejected() {
        build(r"<pos=ADJ>{2}");
    }

    #[test]
    #[should_panic(expected = "bad minmax quantifier")]
    fn spaced_minmax_quantifier_without_a_maximum_is_rejected() {
        build(r"<pos=ADJ> {2}");
    }

    #[test]
    fn non_capturing_group_prefix_is_accepted() {
        assert_eq!(
            chunks_after(
                r"(?:<pos=ART>|<pos=PRO>) <pos=SUB>",
                &[("das", &["ART"]), ("Haus", &["SUB"])]
            ),
            vec!["X", "X"]
        );
        assert_eq!(
            chunks_after(
                r"(?:<pos=ART>|<pos=PRO>) <pos=SUB>",
                &[("er", &["PRO"]), ("Haus", &["SUB"])]
            ),
            vec!["X", "X"]
        );
        assert_eq!(
            chunks_after(
                r"(?:<pos=ART>|<pos=PRO>) <pos=SUB>",
                &[("schnell", &["ADV"]), ("Haus", &["SUB"])]
            ),
            vec!["O", "O"]
        );
    }

    #[test]
    fn named_group_prefix_is_skipped() {
        assert_eq!(
            chunks_after(
                r"(<grp>:<pos=ART>) <pos=SUB>",
                &[("das", &["ART"]), ("Haus", &["SUB"])]
            ),
            vec!["X", "X"]
        );
        assert_eq!(
            chunks_after(
                r"(<grp>:<pos=ART>) <pos=SUB>",
                &[("schnell", &["ADV"]), ("Haus", &["SUB"])]
            ),
            vec!["O", "O"]
        );
    }

    #[test]
    #[should_panic(expected = "Expression type not supported")]
    fn unknown_expression_type_is_rejected() {
        build(r"<foo=bar>");
    }

    #[test]
    #[should_panic(expected = "Could not parse expression")]
    fn expression_with_two_equals_is_rejected() {
        build(r"<a=b=c>");
    }

    #[test]
    #[should_panic(expected = "empty logic expression")]
    fn empty_logic_expression_is_rejected() {
        build(r"< >");
    }

    #[test]
    #[should_panic(expected = "Unbalanced parentheses")]
    fn unbalanced_parenthesis_in_an_atom_is_rejected() {
        build(r"<(a>");
    }

    #[test]
    #[should_panic(expected = "non-matching brackets")]
    fn unclosed_group_is_rejected() {
        build(r"( <pos=SUB>");
    }

    #[test]
    #[should_panic(expected = "assertions not supported")]
    fn assertions_are_rejected() {
        build(r"^ <pos=SUB>");
    }

    #[test]
    #[should_panic(expected = "unknown symbol #")]
    fn unknown_symbols_are_rejected() {
        build(r"# <pos=SUB>");
    }

    #[test]
    #[should_panic(expected = "OR remains on the stack")]
    fn trailing_alternative_is_rejected() {
        build(r"<pos=SUB> |");
    }

    #[test]
    fn bounded_map_evicts_least_recently_used_entries_at_capacity() {
        let mut map = BoundedMap::new();
        map.put("k".to_string(), vec![vec!["v".to_string()]]);
        assert_eq!(map.get("k").map(|v| v.len()), Some(1));
        assert!(map.get("missing").is_none());
        for i in 0..=1024 {
            map.put(format!("k{i}"), Vec::new());
        }
        assert!(map.get("k").is_none());
        assert!(map.get("k0").is_none());
        assert!(map.get("k1").is_some());
        assert!(map.get("k1024").is_some());
    }
}
