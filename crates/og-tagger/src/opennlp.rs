use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
struct FxHasher {
    hash: u64,
}

const FX_SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl std::hash::Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.hash = (self.hash.rotate_left(5) ^ (*b as u64)).wrapping_mul(FX_SEED);
        }
    }
    fn finish(&self) -> u64 {
        self.hash
    }
}

type FxBuild = std::hash::BuildHasherDefault<FxHasher>;
type PredsMap = HashMap<Box<str>, Box<[(u32, f64)]>, FxBuild>;
type ContextRanges = Vec<(usize, usize)>;
type ContextFn<'a> = &'a mut dyn FnMut(usize, &[usize], &mut String, &mut ContextRanges);

pub struct GisModel {
    outcomes: Vec<Box<str>>,
    preds: PredsMap,
    prior: f64,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u16(&mut self) -> Option<u16> {
        let b = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i32(&mut self) -> Option<i32> {
        let b = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> Option<f64> {
        let b = self.data.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(f64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn utf(&mut self) -> Option<String> {
        let len = self.u16()? as usize;
        let bytes = self.data.get(self.pos..self.pos + len)?;
        self.pos += len;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

impl GisModel {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut r = Reader { data, pos: 0 };
        r.utf()?;
        r.i32()?;
        r.f64()?;
        let num_outcomes = r.i32()?;
        let mut outcomes = Vec::with_capacity(num_outcomes.max(0) as usize);
        for _ in 0..num_outcomes {
            outcomes.push(r.utf()?.into_boxed_str());
        }
        let num_patterns = r.i32()?;
        let mut patterns: Vec<(usize, Vec<u32>)> = Vec::with_capacity(num_patterns.max(0) as usize);
        for _ in 0..num_patterns {
            let text = r.utf()?;
            let mut nums = text.split(' ').map(|t| t.parse::<u32>());
            let count = nums.next().and_then(|r| r.ok())? as usize;
            let indices: Vec<u32> = nums.collect::<Result<Vec<u32>, _>>().ok()?;
            patterns.push((count, indices));
        }
        let num_contexts = r.i32()?;
        let mut pred_names = Vec::with_capacity(num_contexts.max(0) as usize);
        for _ in 0..num_contexts {
            pred_names.push(r.utf()?);
        }
        let mut params: Vec<f64> = Vec::with_capacity((data.len() - r.pos) / 8);
        while let Some(v) = r.f64() {
            params.push(v);
        }

        let mut entries: Vec<Vec<(u32, f64)>> = vec![Vec::new(); pred_names.len()];
        let mut ctx = 0;
        let mut param_pos = 0;
        for (count, indices) in patterns {
            for _ in 0..count {
                let entry = entries.get_mut(ctx)?;
                entry.reserve(indices.len());
                for &oid in &indices {
                    entry.push((oid, *params.get(param_pos)?));
                    param_pos += 1;
                }
                ctx += 1;
            }
        }

        let mut preds: PredsMap =
            HashMap::with_capacity_and_hasher(pred_names.len(), FxBuild::default());
        for (name, entry) in pred_names.into_iter().zip(entries) {
            preds.insert(name.into_boxed_str(), entry.into_boxed_slice());
        }
        let prior = crate::fdlibm::log(1.0 / outcomes.len() as f64);
        Some(Self {
            outcomes,
            preds,
            prior,
        })
    }

    pub fn from_file(path: &Path) -> Option<Self> {
        Self::from_bytes(&std::fs::read(path).ok()?)
    }

    pub fn outcome_index(&self, outcome: &str) -> Option<usize> {
        self.outcomes.iter().position(|o| &**o == outcome)
    }

    pub fn num_outcomes(&self) -> usize {
        self.outcomes.len()
    }

    pub fn outcome(&self, index: usize) -> &str {
        &self.outcomes[index]
    }

    fn eval(&self, context: &[(usize, usize)], arena: &str, probs: &mut [f64]) {
        probs.fill(self.prior);
        for (fs, fe) in context {
            if let Some(params) = self.preds.get(&arena[*fs..*fe]) {
                for (oid, weight) in params.iter() {
                    probs[*oid as usize] += weight;
                }
            }
        }
        let mut normal = 0.0;
        for p in probs.iter_mut() {
            *p = crate::fdlibm::exp(*p);
            normal += *p;
        }
        for p in probs.iter_mut() {
            *p /= normal;
        }
    }

    fn best_outcome(&self, probs: &[f64]) -> usize {
        let mut best = 0;
        let mut best_score = f64::NEG_INFINITY;
        for (i, p) in probs.iter().enumerate() {
            if *p > best_score {
                best_score = *p;
                best = i;
            }
        }
        best
    }
}

struct BeamSequence {
    score: f64,
    outcomes: Vec<usize>,
}

impl BeamSequence {
    fn java_cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .partial_cmp(&self.score)
            .unwrap_or(Ordering::Equal)
    }
}

struct JavaPriorityQueue {
    items: Vec<BeamSequence>,
}

impl JavaPriorityQueue {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn add(&mut self, element: BeamSequence) {
        self.items.push(element);
        self.sift_up(self.items.len() - 1);
    }

    fn poll(&mut self) -> Option<BeamSequence> {
        if self.items.is_empty() {
            return None;
        }
        if self.items.len() == 1 {
            return self.items.pop();
        }
        let result = std::mem::replace(
            &mut self.items[0],
            BeamSequence {
                score: 0.0,
                outcomes: Vec::new(),
            },
        );
        let x = self.items.pop().unwrap();
        self.sift_down(0, x);
        Some(result)
    }

    fn sift_up(&mut self, start: usize) {
        let x = std::mem::replace(
            &mut self.items[start],
            BeamSequence {
                score: 0.0,
                outcomes: Vec::new(),
            },
        );
        let mut k = start;
        while k > 0 {
            let parent = (k - 1) / 2;
            if x.java_cmp(&self.items[parent]) != Ordering::Less {
                break;
            }
            let parent_val = std::mem::replace(
                &mut self.items[parent],
                BeamSequence {
                    score: 0.0,
                    outcomes: Vec::new(),
                },
            );
            self.items[k] = parent_val;
            k = parent;
        }
        self.items[k] = x;
    }

    fn sift_down(&mut self, start: usize, x: BeamSequence) {
        let n = self.items.len();
        let mut k = start;
        let half = n / 2;
        while k < half {
            let mut child = 2 * k + 1;
            let right = child + 1;
            if right < n && self.items[child].java_cmp(&self.items[right]) == Ordering::Greater {
                child = right;
            }
            if x.java_cmp(&self.items[child]) != Ordering::Greater {
                break;
            }
            let child_val = std::mem::replace(
                &mut self.items[child],
                BeamSequence {
                    score: 0.0,
                    outcomes: Vec::new(),
                },
            );
            self.items[k] = child_val;
            k = child;
        }
        self.items[k] = x;
    }
}

const ZERO_LOG: f64 = -100000.0;

fn beam_search(
    model: &GisModel,
    sequence_len: usize,
    beam_size: usize,
    contexts_for: ContextFn,
    valid: &dyn Fn(usize, &[usize], usize) -> bool,
) -> Vec<usize> {
    let mut arena = String::with_capacity(4096);
    let mut contexts: Vec<(usize, usize)> = Vec::with_capacity(64);
    let mut memo: std::collections::HashMap<u64, Vec<f64>, FxBuild> =
        std::collections::HashMap::with_hasher(FxBuild::default());
    let mut prev: JavaPriorityQueue = JavaPriorityQueue::new();
    prev.add(BeamSequence {
        score: 0.0,
        outcomes: Vec::new(),
    });
    let num_outcomes = model.num_outcomes();
    let mut probs: Vec<f64> = vec![0.0; num_outcomes];

    for i in 0..sequence_len {
        let sz = beam_size.min(prev.len());
        let mut next: JavaPriorityQueue = JavaPriorityQueue::new();
        for _ in 0..sz {
            let top = prev.poll().unwrap();
            let k0 = *top
                .outcomes
                .len()
                .checked_sub(2)
                .and_then(|k| top.outcomes.get(k))
                .unwrap_or(&usize::MAX);
            let k1 = *top.outcomes.last().unwrap_or(&usize::MAX);
            let enc = |v: usize| -> u64 { if v == usize::MAX { 0 } else { v as u64 + 1 } };
            let memo_key: u64 = ((i as u64) << 44) | (enc(k0) << 22) | enc(k1);
            if let Some(cached) = memo.get(&memo_key) {
                probs.copy_from_slice(cached);
            } else {
                arena.clear();
                contexts.clear();
                contexts_for(i, &top.outcomes, &mut arena, &mut contexts);
                model.eval(&contexts, &arena, &mut probs);
                memo.insert(memo_key, probs.to_vec());
            }
            let kth = probs.len().saturating_sub(beam_size).min(probs.len() - 1);
            let mut sorted = probs.clone();
            sorted.select_nth_unstable_by(kth, |a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            let min = sorted[kth];

            let advance = |next: &mut JavaPriorityQueue, threshold: f64| {
                for (p, probability) in probs.iter().enumerate() {
                    if *probability >= threshold && valid(i, &top.outcomes, p) {
                        let score = top.score + crate::fdlibm::log(*probability);
                        if score > ZERO_LOG {
                            let mut outcomes = top.outcomes.clone();
                            outcomes.push(p);
                            next.add(BeamSequence { score, outcomes });
                        }
                    }
                }
            };

            advance(&mut next, min);
            if next.is_empty() {
                advance(&mut next, f64::NEG_INFINITY);
            }
        }
        prev = next;
    }

    prev.poll().map(|s| s.outcomes).unwrap_or_default()
}

pub struct OpenNlpTokenizer {
    model: GisModel,
}

impl OpenNlpTokenizer {
    pub fn new(model: GisModel) -> Self {
        Self { model }
    }

    pub fn tokenize_spans(&self, text: &str) -> Vec<(usize, usize)> {
        let mut spans = whitespace_spans(text);
        let mut result = Vec::with_capacity(spans.len());
        let mut probs = vec![0.0; self.model.num_outcomes()];
        for (span_start, span_end) in spans.drain(..) {
            let tok = &text[span_start..span_end];
            if tok.chars().count() < 2 || is_alphanumeric(tok) {
                result.push((span_start, span_end));
                continue;
            }
            let mut part_start = span_start;
            for j in (span_start + 1)..span_end {
                if !text.is_char_boundary(j) {
                    continue;
                }
                let position = text[span_start..j].chars().count();
                let mut arena = String::new();
                let mut contexts: Vec<(usize, usize)> = Vec::new();
                tokenizer_contexts(tok, position, &mut arena, &mut contexts);
                self.model.eval(&contexts, &arena, &mut probs);
                if self.model.outcome(self.model.best_outcome(&probs)) == "T" {
                    result.push((part_start, j));
                    part_start = j;
                }
            }
            result.push((part_start, span_end));
        }
        result
    }
}

fn whitespace_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if is_java_whitespace(c) {
            if let Some(s) = start.take() {
                spans.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        spans.push((s, text.len()));
    }
    spans
}

fn is_java_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0B}'
            | '\u{0C}'
            | '\r'
            | '\u{1C}'..='\u{1F}'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{20}'
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

fn is_alphanumeric(tok: &str) -> bool {
    !tok.is_empty() && tok.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn push_feat(arena: &mut String, out: &mut Vec<(usize, usize)>, parts: &[&str]) {
    let start = arena.len();
    for part in parts {
        arena.push_str(part);
    }
    out.push((start, arena.len()));
}

fn push_char_preds(arena: &mut String, out: &mut Vec<(usize, usize)>, key: &str, c: char) {
    push_feat(arena, out, &[key, "=", &c.to_string()]);
    if c.is_alphabetic() {
        push_feat(arena, out, &[key, "_alpha"]);
        if c.is_uppercase() {
            push_feat(arena, out, &[key, "_caps"]);
        }
    } else if c.is_numeric() {
        push_feat(arena, out, &[key, "_num"]);
    } else if is_java_whitespace(c) {
        push_feat(arena, out, &[key, "_ws"]);
    } else if c == '.' || c == '?' || c == '!' {
        push_feat(arena, out, &[key, "_eos"]);
    } else if c == '`' || c == '"' || c == '\'' {
        push_feat(arena, out, &[key, "_quote"]);
    } else if c == '[' || c == '{' || c == '(' {
        push_feat(arena, out, &[key, "_lp"]);
    } else if c == ']' || c == '}' || c == ')' {
        push_feat(arena, out, &[key, "_rp"]);
    }
}

fn tokenizer_contexts(
    token: &str,
    index: usize,
    arena: &mut String,
    preds: &mut Vec<(usize, usize)>,
) {
    let chars: Vec<char> = token.chars().collect();
    let prev: String = chars[..index].iter().collect();
    let suffix: String = chars[index..].iter().collect();
    push_feat(arena, preds, &["p=", &prev]);
    push_feat(arena, preds, &["s=", &suffix]);
    if index > 0 {
        push_char_preds(arena, preds, "p1", chars[index - 1]);
        if index > 1 {
            push_char_preds(arena, preds, "p2", chars[index - 2]);
            push_feat(
                arena,
                preds,
                &[
                    "p21=",
                    &chars[index - 2].to_string(),
                    &chars[index - 1].to_string(),
                ],
            );
        } else {
            push_feat(arena, preds, &["p2=bok"]);
        }
        push_feat(
            arena,
            preds,
            &[
                "p1f1=",
                &chars[index - 1].to_string(),
                &chars[index].to_string(),
            ],
        );
    } else {
        push_feat(arena, preds, &["p1=bok"]);
    }
    push_char_preds(arena, preds, "f1", chars[index]);
    if index + 1 < chars.len() {
        push_char_preds(arena, preds, "f2", chars[index + 1]);
        push_feat(
            arena,
            preds,
            &[
                "f12=",
                &chars[index].to_string(),
                &chars[index + 1].to_string(),
            ],
        );
    } else {
        push_feat(arena, preds, &["f2=bok"]);
    }
    if chars[0] == '&' && chars[chars.len() - 1] == ';' {
        push_feat(arena, preds, &["cc"]);
    }
}

pub struct OpenNlpPosTagger {
    model: GisModel,
    tag_dictionary: HashMap<String, Vec<usize>>,
}

impl OpenNlpPosTagger {
    pub fn new(model: GisModel, tag_dictionary: HashMap<String, Vec<usize>>) -> Self {
        Self {
            model,
            tag_dictionary,
        }
    }

    pub fn outcome(&self, index: usize) -> &str {
        self.model.outcome(index)
    }

    pub fn tag(&self, tokens: &[&str]) -> Vec<usize> {
        let model = &self.model;
        let dict = &self.tag_dictionary;
        beam_search(
            model,
            tokens.len(),
            3,
            &mut |index, outcomes, arena, contexts| {
                pos_contexts(index, tokens, outcomes, model, arena, contexts)
            },
            &|index, _outcomes, outcome| {
                dict.get(tokens[index])
                    .map(|tags| tags.contains(&outcome))
                    .unwrap_or(true)
            },
        )
    }
}

fn pos_contexts(
    index: usize,
    tokens: &[&str],
    outcomes: &[usize],
    model: &GisModel,
    arena: &mut String,
    e: &mut Vec<(usize, usize)>,
) {
    let se = "*SE*";
    let sb = "*SB*";
    let lex = tokens[index];
    let next = if tokens.len() > index + 1 {
        tokens[index + 1]
    } else {
        se
    };
    let nextnext = if tokens.len() > index + 2 {
        Some(tokens[index + 2])
    } else if tokens.len() > index + 1 {
        Some(se)
    } else {
        None
    };

    push_feat(arena, e, &["default"]);
    push_feat(arena, e, &["w=", lex]);
    let char_count = lex.chars().count();
    for li in 0..4 {
        let skip = char_count.saturating_sub(li + 1);
        let start = char_index(lex, skip);
        push_feat(arena, e, &["suf=", &lex[start..]]);
    }
    for li in 0..4 {
        let end = char_index(lex, (li + 1).min(char_count));
        push_feat(arena, e, &["pre=", &lex[..end]]);
    }
    if lex.contains('-') {
        push_feat(arena, e, &["h"]);
    }
    if lex.chars().any(|c| c.is_ascii_uppercase()) {
        push_feat(arena, e, &["c"]);
    }
    if lex.chars().any(|c| c.is_ascii_digit()) {
        push_feat(arena, e, &["d"]);
    }

    if index >= 1 {
        let prev = tokens[index - 1];
        push_feat(arena, e, &["p=", prev]);
        push_feat(arena, e, &["t=", model.outcome(outcomes[index - 1])]);
        if index >= 2 {
            let prevprev = tokens[index - 2];
            push_feat(arena, e, &["pp=", prevprev]);
            push_feat(
                arena,
                e,
                &[
                    "t2=",
                    model.outcome(outcomes[index - 2]),
                    ",",
                    model.outcome(outcomes[index - 1]),
                ],
            );
        } else {
            push_feat(arena, e, &["pp=", sb]);
        }
    } else {
        push_feat(arena, e, &["p=", sb]);
    }

    push_feat(arena, e, &["n=", next]);
    if let Some(nn) = nextnext {
        push_feat(arena, e, &["nn=", nn]);
    }
}

fn char_index(s: &str, char_position: usize) -> usize {
    s.char_indices()
        .nth(char_position)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

pub struct OpenNlpChunker {
    model: GisModel,
}

impl OpenNlpChunker {
    pub fn new(model: GisModel) -> Self {
        Self { model }
    }

    pub fn outcome(&self, index: usize) -> &str {
        self.model.outcome(index)
    }

    pub fn chunk(&self, tokens: &[&str], tags: &[&str]) -> Vec<usize> {
        let model = &self.model;
        beam_search(
            model,
            tokens.len(),
            10,
            &mut |index, outcomes, arena, contexts| {
                chunker_contexts(index, tokens, tags, outcomes, model, arena, contexts)
            },
            &|_index, outcomes, outcome| {
                let name = model.outcome(outcome);
                let prev_name = outcomes.last().map(|o| model.outcome(*o));
                match prev_name {
                    None => !name.starts_with("I-"),
                    Some(prev) => {
                        if name.starts_with("I-") {
                            prev != "O" && name.split_at(2).1 == prev.split_at(2).1
                        } else {
                            true
                        }
                    }
                }
            },
        )
    }
}

fn chunker_contexts(
    index: usize,
    tokens: &[&str],
    tags: &[&str],
    preds: &[usize],
    model: &GisModel,
    arena: &mut String,
    out: &mut Vec<(usize, usize)>,
) {
    let n = tokens.len();
    let (w_2, t_2, p_2) = if index < 2 {
        (vec!["w_2=bos"], vec!["t_2=bos"], vec!["p_2=bos"])
    } else {
        (
            vec!["w_2=", tokens[index - 2]],
            vec!["t_2=", tags[index - 2]],
            vec!["p_2", model.outcome(preds[index - 2])],
        )
    };
    let (w_1, t_1, p_1) = if index < 1 {
        (vec!["w_1=bos"], vec!["t_1=bos"], vec!["p_1=bos"])
    } else {
        (
            vec!["w_1=", tokens[index - 1]],
            vec!["t_1=", tags[index - 1]],
            vec!["p_1=", model.outcome(preds[index - 1])],
        )
    };
    let w0 = vec!["w0=", tokens[index]];
    let t0 = vec!["t0=", tags[index]];
    let (w1, t1) = if index + 1 >= n {
        (vec!["w1=eos"], vec!["t1=eos"])
    } else {
        (vec!["w1=", tokens[index + 1]], vec!["t1=", tags[index + 1]])
    };
    let (w2, t2) = if index + 2 >= n {
        (vec!["w2=eos"], vec!["t2=eos"])
    } else {
        (vec!["w2=", tokens[index + 2]], vec!["t2=", tags[index + 2]])
    };

    macro_rules! push2 {
        ($a:expr, $b:expr) => {
            push_feat(
                arena,
                out,
                &[
                    $a[0],
                    *$a.get(1).unwrap_or(&""),
                    $b[0],
                    *$b.get(1).unwrap_or(&""),
                ],
            );
        };
    }
    macro_rules! push3 {
        ($a:expr, $b:expr, $c:expr) => {
            push_feat(
                arena,
                out,
                &[
                    $a[0],
                    *$a.get(1).unwrap_or(&""),
                    $b[0],
                    *$b.get(1).unwrap_or(&""),
                    $c[0],
                    *$c.get(1).unwrap_or(&""),
                ],
            );
        };
    }
    macro_rules! push4 {
        ($a:expr, $b:expr, $c:expr, $d:expr) => {
            push_feat(
                arena,
                out,
                &[
                    $a[0],
                    *$a.get(1).unwrap_or(&""),
                    $b[0],
                    *$b.get(1).unwrap_or(&""),
                    $c[0],
                    *$c.get(1).unwrap_or(&""),
                    $d[0],
                    *$d.get(1).unwrap_or(&""),
                ],
            );
        };
    }

    push_feat(arena, out, &w_2);
    push_feat(arena, out, &w_1);
    push_feat(arena, out, &w0);
    push_feat(arena, out, &w1);
    push_feat(arena, out, &w2);
    push2!(w_1, w0);
    push2!(w0, w1);
    push_feat(arena, out, &t_2);
    push_feat(arena, out, &t_1);
    push_feat(arena, out, &t0);
    push_feat(arena, out, &t1);
    push_feat(arena, out, &t2);
    push2!(t_2, t_1);
    push2!(t_1, t0);
    push2!(t0, t1);
    push2!(t1, t2);
    push3!(t_2, t_1, t0);
    push3!(t_1, t0, t1);
    push3!(t0, t1, t2);
    push_feat(arena, out, &p_2);
    push_feat(arena, out, &p_1);
    push2!(p_2, p_1);
    push2!(p_1, t_2);
    push2!(p_1, t_1);
    push2!(p_1, t0);
    push2!(p_1, t1);
    push2!(p_1, t2);
    push3!(p_1, t_2, t_1);
    push3!(p_1, t_1, t0);
    push3!(p_1, t0, t1);
    push3!(p_1, t1, t2);
    push4!(p_1, t_2, t_1, t0);
    push4!(p_1, t_1, t0, t1);
    push4!(p_1, t0, t1, t2);
    push2!(p_1, w_2);
    push2!(p_1, w_1);
    push2!(p_1, w0);
    push2!(p_1, w1);
    push2!(p_1, w2);
    push3!(p_1, w_1, w0);
    push3!(p_1, w0, w1);
}
pub struct OpenNlpEnglishChunker {
    pub tokenizer: OpenNlpTokenizer,
    pos: OpenNlpPosTagger,
    chunker: OpenNlpChunker,
}

pub fn parse_tag_dictionary(xml: &str, model: &GisModel) -> HashMap<String, Vec<usize>> {
    let mut dict: HashMap<String, Vec<usize>> = HashMap::new();
    let mut current_tags: Option<Vec<usize>> = None;
    let mut buffer = String::new();
    for event in quick_xml_events(xml) {
        match event {
            XmlEvent::Start(name, attrs) => {
                if name == "entry" {
                    current_tags = attrs.iter().find(|(k, _)| k == "tags").and_then(|(_, v)| {
                        let ids = v
                            .split_whitespace()
                            .filter_map(|t| model.outcome_index(t))
                            .collect::<Vec<usize>>();
                        (!ids.is_empty()).then_some(ids)
                    });
                    if current_tags.is_none() {
                        current_tags = Some(Vec::new());
                    }
                }
            }
            XmlEvent::Text(text) => buffer.push_str(&text),
            XmlEvent::End(name) => {
                if name == "token" {
                    let token = buffer.trim().to_string();
                    if let Some(tags) = current_tags.take()
                        && !token.is_empty()
                    {
                        dict.insert(token, tags);
                    }
                } else if name == "entry" {
                    current_tags = None;
                }
                buffer.clear();
            }
        }
    }
    dict
}

enum XmlEvent {
    Start(String, Vec<(String, String)>),
    End(String),
    Text(String),
}

fn quick_xml_events(xml: &str) -> Vec<XmlEvent> {
    let mut events = Vec::new();
    let mut i = 0;
    while let Some(open) = xml[i..].find('<') {
        let abs = i + open;
        if abs > i {
            events.push(XmlEvent::Text(xml[i..abs].to_string()));
        }
        let close = match xml[abs..].find('>') {
            Some(c) => abs + c,
            None => break,
        };
        let inner = &xml[abs + 1..close];
        i = close + 1;
        if let Some(name) = inner.strip_prefix('/') {
            events.push(XmlEvent::End(name.trim().to_string()));
        } else if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        } else {
            let (name, attrs) = parse_tag(inner);
            if name.is_empty() {
                continue;
            }
            if inner.ends_with('/') {
                events.push(XmlEvent::Start(name.clone(), attrs));
                events.push(XmlEvent::End(name));
            } else {
                events.push(XmlEvent::Start(name, attrs));
            }
        }
    }
    events
}

fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let bytes: Vec<char> = inner.chars().collect();
    let mut p = 0;
    while p < bytes.len() && !bytes[p].is_whitespace() {
        p += 1;
    }
    let name: String = bytes[..p].iter().collect();
    let mut attrs = Vec::new();
    while p < bytes.len() {
        while p < bytes.len() && bytes[p].is_whitespace() {
            p += 1;
        }
        let key_start = p;
        while p < bytes.len() && bytes[p] != '=' && !bytes[p].is_whitespace() {
            p += 1;
        }
        if key_start == p {
            break;
        }
        let key: String = bytes[key_start..p].iter().collect();
        if p < bytes.len() && bytes[p] == '=' {
            p += 1;
            if p < bytes.len() && (bytes[p] == '"' || bytes[p] == '\'') {
                let quote = bytes[p];
                p += 1;
                let value_start = p;
                while p < bytes.len() && bytes[p] != quote {
                    p += 1;
                }
                attrs.push((key, bytes[value_start..p].iter().collect()));
                p += 1;
            } else {
                let value_start = p;
                while p < bytes.len() && !bytes[p].is_whitespace() {
                    p += 1;
                }
                attrs.push((key, bytes[value_start..p].iter().collect()));
            }
        }
    }
    (name, attrs)
}

impl OpenNlpEnglishChunker {
    pub fn from_resource_dir(dir: &Path) -> Option<Self> {
        let tokenizer = GisModel::from_file(&dir.join("en-token.model"))?;
        let pos_model = GisModel::from_file(&dir.join("en-pos-maxent.model"))?;
        let chunk_model = GisModel::from_file(&dir.join("en-chunker.model"))?;
        let tagdict = parse_tag_dictionary(
            &std::fs::read_to_string(dir.join("en-pos-maxent.tagdict")).ok()?,
            &pos_model,
        );
        Some(Self {
            tokenizer: OpenNlpTokenizer::new(tokenizer),
            pos: OpenNlpPosTagger::new(pos_model, tagdict),
            chunker: OpenNlpChunker::new(chunk_model),
        })
    }

    pub fn shared() -> Option<&'static Self> {
        static SHARED: std::sync::OnceLock<Option<OpenNlpEnglishChunker>> =
            std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| {
                let candidates = [
                    "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
                    "languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
                ];
                candidates
                    .iter()
                    .find_map(|p| Self::from_resource_dir(Path::new(p)))
            })
            .as_ref()
    }

    pub fn install(resource_dir: &Path) {
        static SHARED: std::sync::OnceLock<Option<OpenNlpEnglishChunker>> =
            std::sync::OnceLock::new();
        let _ = SHARED.set(Self::from_resource_dir(resource_dir));
    }

    pub fn add_chunk_tags_on(&self, readings: &mut [og_core::AnalyzedTokenReadings]) {
        let spans_and_tags = self.chunk_readings(readings);
        for (idx, chunk_tags) in spans_and_tags {
            readings[idx].replace_chunk_tags_preserving_apostrophe_flag(chunk_tags);
        }
    }

    fn chunk_readings(
        &self,
        readings: &[og_core::AnalyzedTokenReadings],
    ) -> Vec<(usize, Vec<String>)> {
        let mut text = String::new();
        for token in readings {
            if is_sentence_start(token) {
                continue;
            }
            text.push_str(token.token().token());
        }
        let clean = text.replace('\u{2019}', "'");
        let spans = self.tokenizer.tokenize_spans(&clean);
        let token_texts: Vec<&str> = spans.iter().map(|(s, e)| &clean[*s..*e]).collect();
        let tag_indices = self.pos.tag(&token_texts);
        let tag_names: Vec<&str> = tag_indices
            .iter()
            .map(|i| self.pos.model.outcome(*i))
            .collect();
        let chunk_indices = self.chunker.chunk(&token_texts, &tag_names);
        let mapped = map_tokens_to_readings(&token_texts, readings);
        let raw_tags: Vec<Option<&str>> = chunk_indices
            .iter()
            .map(|i| Some(self.chunker.model.outcome(*i)))
            .collect();
        let filtered = english_chunk_filter(&raw_tags, &token_texts, &mapped, readings);
        let mut result = Vec::with_capacity(filtered.len());
        for (idx, chunk_tags) in filtered.iter().enumerate() {
            if chunk_tags.is_empty() {
                continue;
            }
            if let Some(atr_idx) = mapped[idx] {
                result.push((atr_idx, chunk_tags.clone()));
            }
        }
        result
    }

    pub fn add_chunk_tags(&self, sentence: &mut og_core::AnalyzedSentence) {
        let updates = self.chunk_readings(sentence.tokens());
        let tokens = sentence.tokens_mut();
        for (idx, chunk_tags) in updates {
            tokens[idx].replace_chunk_tags_preserving_apostrophe_flag(chunk_tags);
        }
    }
}

fn is_sentence_start(reading: &og_core::AnalyzedTokenReadings) -> bool {
    reading.token().token() == "<S>" && reading.token().has_pos_tag("SENT_START")
}

fn map_tokens_to_readings(
    token_texts: &[&str],
    readings: &[og_core::AnalyzedTokenReadings],
) -> Vec<Option<usize>> {
    let mut spans: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::with_capacity(readings.len());
    let mut reading_pos = 0usize;
    for (idx, reading) in readings.iter().enumerate() {
        let reading_token = reading.token().token();
        let skip = is_sentence_start(reading)
            || reading_token.trim().is_empty()
            || (reading_token.chars().count() == 1 && reading_token.starts_with('\u{a0}'));
        if skip {
            continue;
        }
        let normalized_len = reading_token.replace('\u{2019}', "'").len();
        spans.insert((reading_pos, reading_pos + normalized_len), idx);
        reading_pos += normalized_len;
    }
    let mut opennlp_pos = 0usize;
    token_texts
        .iter()
        .map(|token| {
            let start = opennlp_pos;
            let end = start + token.len();
            opennlp_pos = end;
            spans.get(&(start, end)).copied()
        })
        .collect()
}

fn english_chunk_filter(
    raw_tags: &[Option<&str>],
    token_texts: &[&str],
    mapped: &[Option<usize>],
    readings: &[og_core::AnalyzedTokenReadings],
) -> Vec<Vec<String>> {
    let n = raw_tags.len();
    let is_begin_np = |i: usize| raw_tags.get(i) == Some(&Some("B-NP"));
    let is_cont_np = |i: usize| raw_tags.get(i) == Some(&Some("I-NP"));
    let is_end_np = |i: usize| if i + 2 > n { true } else { !is_cont_np(i + 1) };

    let has_plural_reading = |i: usize| -> bool {
        mapped
            .get(i)
            .and_then(|r| *r)
            .map(|r| readings[r].has_pos_tag("NNS"))
            .unwrap_or(false)
    };

    let chunk_type_plural = |start: usize| -> bool {
        for j in start..n {
            if !is_begin_np(j) && !is_cont_np(j) {
                break;
            }
            if has_plural_reading(j) {
                return true;
            }
        }
        false
    };

    let end_of_np_is_singular = |start: usize| -> bool {
        for j in start..n {
            if is_end_np(j) {
                return !chunk_type_plural(j);
            }
        }
        false
    };

    let mut result: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut new_chunk_tag: Option<String> = None;
    for i in 0..n {
        let mut tags: Vec<String> = Vec::new();
        if is_begin_np(i) {
            let plural = chunk_type_plural(i);
            if !plural || end_of_np_is_singular(i) {
                tags.push("B-NP-singular".to_string());
                new_chunk_tag = Some("NP-singular".to_string());
            } else {
                tags.push("B-NP-plural".to_string());
                new_chunk_tag = Some("NP-plural".to_string());
            }
        }
        if new_chunk_tag.is_some() && is_end_np(i) {
            tags.push(format!("E-{}", new_chunk_tag.clone().unwrap()));
            new_chunk_tag = None;
        }
        if new_chunk_tag.is_some() && is_cont_np(i) {
            tags.push(format!("I-{}", new_chunk_tag.clone().unwrap()));
        }
        if tags.is_empty()
            && let Some(tag) = raw_tags[i]
        {
            tags.push(tag.to_string());
        }
        result[i] = tags;
    }
    let _ = token_texts;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource_dir() -> Option<&'static std::path::Path> {
        static DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
        DIR.get_or_init(|| {
            [
                "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
                "languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
            ]
            .iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.join("en-chunker.model").exists())
        })
        .as_deref()
    }

    fn chunker() -> Option<OpenNlpEnglishChunker> {
        resource_dir().and_then(OpenNlpEnglishChunker::from_resource_dir)
    }

    #[test]
    fn tokenizer_splits_punctuation_from_words() {
        let Some(dir) = resource_dir() else { return };
        let model = GisModel::from_file(&dir.join("en-token.model")).unwrap();
        let tok = OpenNlpTokenizer::new(model);
        let spans = tok.tokenize_spans("Hello world, don't shout!");
        let toks: Vec<&str> = spans
            .iter()
            .map(|(s, e)| &"Hello world, don't shout!"[*s..*e])
            .collect();
        assert_eq!(toks, vec!["Hello", "world", ",", "do", "n't", "shout", "!"]);
    }

    #[test]
    fn tokenizer_treats_no_break_space_as_whitespace() {
        let Some(dir) = resource_dir() else { return };
        let model = GisModel::from_file(&dir.join("en-token.model")).unwrap();
        let tok = OpenNlpTokenizer::new(model);
        let text = "Currently\u{a0}not available";
        let toks: Vec<&str> = tok
            .tokenize_spans(text)
            .iter()
            .map(|(s, e)| &text[*s..*e])
            .collect();
        assert_eq!(toks, vec!["Currently", "not", "available"]);
    }

    #[test]
    fn chunker_keeps_mapping_with_no_break_space_token() {
        let Some(chunker) = chunker() else { return };
        let text = "Currently\u{a0}not available for the Premium version.";
        let mut pos = 0;
        let mut readings = Vec::new();
        let mut last = 0;
        let mut parts: Vec<&str> = Vec::new();
        for (i, c) in text.char_indices() {
            if c == ' ' || c == '\u{a0}' {
                if i > last {
                    parts.push(&text[last..i]);
                }
                parts.push(&text[i..i + c.len_utf8()]);
                last = i + c.len_utf8();
            }
        }
        if last < text.len() {
            parts.push(&text[last..]);
        }
        for part in parts {
            let at = og_core::AnalyzedToken::new(part, pos, pos + part.len());
            readings.push(og_core::AnalyzedTokenReadings::new(at));
            pos += part.len();
        }
        chunker.add_chunk_tags_on(&mut readings);
        let available = readings
            .iter()
            .find(|r| r.token().token() == "available")
            .unwrap();
        assert_eq!(
            available.chunks().first().map(String::as_str),
            Some("B-ADJP"),
            "chunks: {:?}",
            readings
                .iter()
                .map(|r| (r.token().token(), r.chunks()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn chunker_preserves_typographic_apostrophe_marker() {
        let Some(chunker) = chunker() else { return };
        let mut readings = Vec::new();
        let mut pos = 0;
        for part in ["An", " ", "actress", "’s", " ", "role"] {
            let mut atr = og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                part,
                pos,
                pos + part.len(),
            ));
            if part == "’s" {
                atr.mark_typographic_apostrophe();
            }
            readings.push(atr);
            pos += part.len();
        }
        chunker.add_chunk_tags_on(&mut readings);
        let curly = readings.iter().find(|r| r.token().token() == "'s");
        if let Some(curly) = curly {
            assert!(
                curly.has_typographic_apostrophe(),
                "chunks: {:?}",
                curly.chunks()
            );
        }
    }

    #[test]
    fn pos_tagger_labels_a_simple_sentence() {
        let Some(dir) = resource_dir() else { return };
        let model = GisModel::from_file(&dir.join("en-pos-maxent.model")).unwrap();
        let dict = parse_tag_dictionary(
            &std::fs::read_to_string(dir.join("en-pos-maxent.tagdict")).unwrap(),
            &model,
        );
        assert!(!dict.is_empty());
        let tagger = OpenNlpPosTagger::new(model, dict);
        let words = [
            "The", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog", ".",
        ];
        let tags: Vec<String> = tagger
            .tag(&words)
            .iter()
            .map(|i| tagger.model.outcome(*i).to_string())
            .collect();
        println!("TAGS: {tags:?}");
        assert_eq!(tags[0], "DT");
        assert_eq!(tags[3], "NN");
        assert_eq!(tags[7], "JJ");
        assert_eq!(tags[8], "NN");
        assert_eq!(tags[9], ".");
    }

    #[test]
    fn java_english_chunker_test_add_chunk_tags() {
        let Some(chunker) = chunker() else { return };
        let sentence = "A short test of the bicycle is needed";
        let mut readings = Vec::new();
        let mut pos = 0;
        let mut parts: Vec<&str> = Vec::new();
        let mut last = 0;
        for (i, c) in sentence.char_indices() {
            if c == ' ' {
                if i > last {
                    parts.push(&sentence[last..i]);
                }
                parts.push(&sentence[i..i + 1]);
                last = i + 1;
            }
        }
        if last < sentence.len() {
            parts.push(&sentence[last..]);
        }
        for part in parts {
            let at = og_core::AnalyzedToken::new(part, pos, pos + part.len());
            readings.push(og_core::AnalyzedTokenReadings::new(at));
            pos += part.len();
        }
        chunker.add_chunk_tags_on(&mut readings);
        let tags: Vec<String> = readings.iter().map(|r| r.chunks().join("|")).collect();
        assert_eq!(readings.len(), 15);
        assert_eq!(tags[0], "B-NP-singular", "{tags:?}");
        assert_eq!(tags[2], "I-NP-singular", "{tags:?}");
        assert_eq!(tags[4], "E-NP-singular", "{tags:?}");
        assert_eq!(tags[8], "B-NP-singular", "{tags:?}");
        assert_eq!(tags[10], "E-NP-singular", "{tags:?}");
        assert_eq!(tags[12], "B-VP", "{tags:?}");
        assert_eq!(tags[14], "I-VP", "{tags:?}");
    }

    #[test]
    fn chunk_filter_marks_np_singular_when_only_the_end_token_lacks_plural() {
        let words = [
            "performance",
            "management",
            "and",
            "staff",
            "planning",
            "tool",
        ];
        let readings: Vec<og_core::AnalyzedTokenReadings> = words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let tags: &[&str] = if *w == "staff" {
                    &["NNS", "NN:UN"]
                } else {
                    &["NN"]
                };
                let rs = tags
                    .iter()
                    .map(|t| {
                        og_core::AnalyzedToken::new(*w, 0, 0).with_pos_tags(vec![t.to_string()])
                    })
                    .collect();
                let _ = i;
                og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(*w, 0, 0))
                    .with_readings(rs)
            })
            .collect();
        let raw = [
            Some("B-NP"),
            Some("I-NP"),
            Some("I-NP"),
            Some("I-NP"),
            Some("I-NP"),
            Some("I-NP"),
        ];
        let mapped: Vec<Option<usize>> = (0..words.len()).map(Some).collect();
        let filtered = english_chunk_filter(&raw, &words, &mapped, &readings);
        assert_eq!(
            filtered[0],
            vec!["B-NP-singular".to_string()],
            "EnglishChunkFilter.endOfNounPhraseIsSingular: head noun 'tool' has no NNS reading"
        );
        assert_eq!(filtered[5], vec!["E-NP-singular".to_string()]);
        let raw_plural_head = [
            Some("B-NP"),
            Some("I-NP"),
            Some("I-NP"),
            Some("I-NP"),
            Some("I-NP"),
            Some("I-NP"),
        ];
        let plural_readings: Vec<og_core::AnalyzedTokenReadings> = words
            .iter()
            .map(|w| {
                let tags: &[&str] = if *w == "tool" { &["NNS"] } else { &["NN"] };
                let rs = tags
                    .iter()
                    .map(|t| {
                        og_core::AnalyzedToken::new(*w, 0, 0).with_pos_tags(vec![t.to_string()])
                    })
                    .collect();
                og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(*w, 0, 0))
                    .with_readings(rs)
            })
            .collect();
        let filtered2 = english_chunk_filter(&raw_plural_head, &words, &mapped, &plural_readings);
        assert_eq!(filtered2[0], vec!["B-NP-plural".to_string()]);
    }

    #[test]
    fn chunker_labels_noun_phrases() {
        let Some(chunker) = chunker() else { return };
        let words = [
            "The", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog", ".",
        ];
        let tags = ["DT", "JJ", "JJ", "NN", "VBZ", "IN", "DT", "JJ", "NN", "."];
        let chunks: Vec<String> = chunker
            .chunker
            .chunk(&words, &tags)
            .iter()
            .map(|i| chunker.chunker.model.outcome(*i).to_string())
            .collect();
        assert_eq!(chunks[0], "B-NP");
        assert_eq!(chunks[3], "I-NP");
        assert_eq!(chunks[4], "B-VP");
        assert_eq!(chunks[9], "O");
    }

    #[test]
    fn full_pipeline_assignes_np_chunks_to_readings() {
        let Some(chunker) = chunker() else { return };
        let text = "The quick brown fox jumps over the lazy dog.";
        let mut sentence = og_core::AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        let mut last = 0;
        let mut parts: Vec<&str> = Vec::new();
        for (i, c) in text.char_indices() {
            if c == ' ' {
                if i > last {
                    parts.push(&text[last..i]);
                }
                parts.push(&text[i..i + 1]);
                last = i + 1;
            }
        }
        if last < text.len() {
            parts.push(&text[last..]);
        }
        for part in parts {
            let at = og_core::AnalyzedToken::new(part, pos, pos + part.len());
            tokens.push(og_core::AnalyzedTokenReadings::new(at));
            pos += part.len();
        }
        sentence.set_tokens(tokens);
        chunker.add_chunk_tags(&mut sentence);
        let chunks: Vec<String> = sentence
            .tokens()
            .iter()
            .map(|t| t.chunks().join("|"))
            .collect();
        assert!(chunks[0].starts_with("B-NP"), "got {:?}", chunks);
        assert!(chunks[8].contains("E-NP"), "got {:?}", chunks);
        assert!(chunks[10].contains("B-PP"), "got {:?}", chunks);
        assert!(chunks[16].is_empty(), "got {:?}", chunks);
    }
}

#[cfg(test)]
mod opennlp_docs_parity_tests {
    use super::*;

    fn chunker() -> OpenNlpEnglishChunker {
        let resource_dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        );
        OpenNlpEnglishChunker::from_resource_dir(resource_dir).expect("opennlp models")
    }

    #[test]
    fn pos_tagger_matches_official_docs_output() {
        let chunker = chunker();
        let words: Vec<&str> =
            "Pierre Vinken , 61 years old , will join the board as a nonexecutive director Nov. 29 ."
                .split(' ')
                .collect();
        let expected: Vec<&str> = "NNP NNP , CD NNS JJ , MD VB DT NN IN DT JJ NN NNP CD ."
            .split(' ')
            .collect();
        let tag_indices = chunker.pos.tag(&words);
        let got: Vec<&str> = tag_indices
            .iter()
            .map(|i| chunker.pos.model.outcome(*i))
            .collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn chunker_matches_official_docs_output() {
        let chunker = chunker();
        let cases: Vec<(Vec<&str>, Vec<&str>, Vec<&str>)> = vec![
            (
                vec![
                    "Rockwell",
                    "International",
                    "Corp.",
                    "'s",
                    "Tulsa",
                    "unit",
                    "said",
                    "it",
                    "signed",
                    "a",
                    "tentative",
                    "agreement",
                    "extending",
                    "its",
                    "contract",
                    "with",
                    "Boeing",
                    "Co.",
                    "to",
                    "provide",
                    "structural",
                    "parts",
                    "for",
                    "Boeing",
                    "'s",
                    "747",
                    "jetliners",
                    ".",
                ],
                vec![
                    "NNP", "NNP", "NNP", "POS", "NNP", "NN", "VBD", "PRP", "VBD", "DT", "JJ", "NN",
                    "VBG", "PRP$", "NN", "IN", "NNP", "NNP", "TO", "VB", "JJ", "NNS", "IN", "NNP",
                    "POS", "CD", "NNS", ".",
                ],
                vec![
                    "B-NP", "I-NP", "I-NP", "B-NP", "I-NP", "I-NP", "B-VP", "B-NP", "B-VP", "B-NP",
                    "I-NP", "I-NP", "B-VP", "B-NP", "I-NP", "B-PP", "B-NP", "I-NP", "B-VP", "I-VP",
                    "B-NP", "I-NP", "B-PP", "B-NP", "B-NP", "I-NP", "I-NP", "O",
                ],
            ),
            (
                vec![
                    "Rockwell",
                    "said",
                    "the",
                    "agreement",
                    "calls",
                    "for",
                    "it",
                    "to",
                    "supply",
                    "200",
                    "additional",
                    "so-called",
                    "shipsets",
                    "for",
                    "the",
                    "planes",
                    ".",
                ],
                vec![
                    "NNP", "VBD", "DT", "NN", "VBZ", "IN", "PRP", "TO", "VB", "CD", "JJ", "JJ",
                    "NNS", "IN", "DT", "NNS", ".",
                ],
                vec![
                    "B-NP", "B-VP", "B-NP", "I-NP", "B-VP", "B-SBAR", "B-NP", "B-VP", "I-VP",
                    "B-NP", "I-NP", "I-NP", "I-NP", "B-PP", "B-NP", "I-NP", "O",
                ],
            ),
        ];
        for (words, tags, expected) in &cases {
            let chunk_indices = chunker.chunker.chunk(words, tags);
            let got: Vec<&str> = chunk_indices
                .iter()
                .map(|i| chunker.chunker.model.outcome(*i))
                .collect();
            assert_eq!(&got, expected, "input: {:?}", words);
        }
    }

    #[test]
    fn mapping_resyncs_after_reading_covering_two_opennlp_tokens() {
        let readings: Vec<og_core::AnalyzedTokenReadings> = ["jumped", "$9", "the", "day"]
            .iter()
            .scan(0usize, |pos, text| {
                let start = *pos;
                *pos += text.len();
                Some(og_core::AnalyzedTokenReadings::new(
                    og_core::AnalyzedToken::new(*text, start, *pos),
                ))
            })
            .collect();
        let token_texts: Vec<&str> = vec!["jumped", "$", "9", "the", "day"];
        let mapped = map_tokens_to_readings(&token_texts, &readings);
        assert_eq!(mapped[0], Some(0));
        assert_eq!(mapped[1], None);
        assert_eq!(mapped[2], None);
        assert_eq!(mapped[3], Some(2));
        assert_eq!(mapped[4], Some(3));
    }

    #[test]
    fn mapping_skips_whitespace_between_readings() {
        let readings: Vec<og_core::AnalyzedTokenReadings> = vec![
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("a", 0, 1)),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(" ", 1, 2)),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("b", 2, 3)),
        ];
        let token_texts: Vec<&str> = vec!["a", "b"];
        let mapped = map_tokens_to_readings(&token_texts, &readings);
        assert_eq!(mapped[0], Some(0));
        assert_eq!(mapped[1], Some(2));
    }
}
