use std::collections::{HashMap, HashSet};

use crate::compiler::{CompiledPatternToken, CompiledUnification, PosMatcher};
use crate::types::XmlUnifyGroup;

pub const UNIFY_IGNORE: &str = "unify-ignore";

pub fn normalize_apostrophes(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains('\u{2019}') || text.contains('\u{2018}') || text.contains('\u{201B}') {
        std::borrow::Cow::Owned(text.replace(['\u{2019}', '\u{2018}', '\u{201B}'], "'"))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    let mut ia = a.chars();
    let mut ib = b.chars();
    loop {
        match (ia.next(), ib.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) => {
                if x != y
                    && java_simple_upper(x) != java_simple_upper(y)
                    && java_simple_lower(java_simple_upper(x))
                        != java_simple_lower(java_simple_upper(y))
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

fn java_simple_upper(c: char) -> char {
    let mut it = c.to_uppercase();
    match (it.next(), it.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

fn java_simple_lower(c: char) -> char {
    // Character.toLowerCase(İ) is 'i' (simple mapping); Rust's full mapping is two chars
    if c == '\u{0130}' {
        return 'i';
    }
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

pub fn surface_text(token: &og_core::AnalyzedTokenReadings) -> &str {
    if token.token().token() == "<S>" && token.is_sentence_start() {
        ""
    } else {
        token.token().token()
    }
}

pub fn has_no_tag_reading(token: &og_core::AnalyzedTokenReadings) -> bool {
    token.readings().iter().all(|r| {
        r.pos_tags()
            .iter()
            .all(|t| t == "SENT_END" || t == "PARAGRAPH_END")
    })
}

pub fn reading_pos_matches(
    postag: &str,
    postag_re: Option<&crate::compiler::SharedRegex>,
    postag_fancy: Option<&crate::compiler::SharedFancyRegex>,
    tags: &[String],
    token_untagged: bool,
) -> bool {
    match postag_re {
        Some(re) => {
            tags.iter().any(|t| re.is_match(t)) || (re.is_match("UNKNOWN") && token_untagged)
        }
        None => match postag_fancy {
            Some(re) => {
                tags.iter().any(|t| re.is_match(t).unwrap_or(false))
                    || (re.is_match("UNKNOWN").unwrap_or(false) && token_untagged)
            }
            None => {
                if postag == "UNKNOWN" {
                    token_untagged
                } else {
                    tags.iter().any(|t| t == postag)
                }
            }
        },
    }
}

pub fn reading_matches_element(
    pattern: &CompiledPatternToken,
    surface: &str,
    reading: &og_core::AnalyzedToken,
    token_untagged: bool,
) -> bool {
    let pos_ok = match pattern.postag.as_deref() {
        Some(postag) => {
            reading_pos_matches(
                postag,
                pattern.compiled_postag_regexp.as_ref(),
                pattern.compiled_postag_regexp_fancy.as_ref(),
                reading.pos_tags(),
                token_untagged,
            ) != pattern.negate_pos
        }
        None => true,
    };
    let has_string = pattern.text.is_some()
        || pattern.compiled_regexp.is_some()
        || pattern.compiled_regexp_fancy.is_some();
    if !has_string {
        return !pattern.negate && pos_ok;
    }
    let test = if pattern.inflected {
        reading.lemma().unwrap_or(surface)
    } else {
        surface
    };
    let text_ok = if let Some(text) = pattern.text.as_deref() {
        let norm = normalize_apostrophes(text);
        let t = normalize_apostrophes(test);
        if pattern.case_sensitive {
            *t == *norm
        } else {
            eq_ignore_case(t.as_ref(), norm.as_ref())
        }
    } else if let Some(re) = pattern.compiled_regexp_fancy.as_ref() {
        re.is_match(test).unwrap_or(false)
    } else if let Some(re) = pattern.compiled_regexp.as_ref() {
        if pattern.case_sensitive {
            re.is_match(test)
        } else if let Some(ci) = pattern.compiled_regexp_ci.as_ref() {
            ci.is_match(test)
        } else {
            re.is_match(test)
        }
    } else {
        true
    };
    (text_ok != pattern.negate) && pos_ok
}

pub struct UnifyReading<'a> {
    pub tags: Vec<&'a str>,
    pub id: u32,
}

pub struct UnifyElemPlan<'a> {
    pub elem_idx: usize,
    pub neutral_readings: Vec<usize>,
    pub reading_sets: Vec<Vec<Vec<UnifyReading<'a>>>>,
}

#[derive(Clone)]
pub struct Unifier<'a> {
    maps: Vec<HashMap<String, HashSet<String>>>,
    slots: Vec<Vec<usize>>,
    slot_ids: Vec<Vec<u32>>,
    equiv_matched: Vec<usize>,
    kept: HashMap<String, HashSet<String>>,
    features: Vec<(String, Vec<&'a str>)>,
    eq_matchers: HashMap<(&'a str, &'a str), &'a PosMatcher>,
    tok_cnt: usize,
    readings_counter: usize,
    all_feats_in: bool,
    in_unification: bool,
    uni_matched: bool,
    uni_all_matched: bool,
}

impl<'a> Unifier<'a> {
    pub fn new(
        features: Vec<(String, Vec<&'a str>)>,
        eq_matchers: HashMap<(&'a str, &'a str), &'a PosMatcher>,
    ) -> Self {
        Self {
            maps: Vec::new(),
            slots: Vec::new(),
            slot_ids: Vec::new(),
            equiv_matched: Vec::new(),
            kept: HashMap::new(),
            features,
            eq_matchers,
            tok_cnt: 0,
            readings_counter: 1,
            all_feats_in: false,
            in_unification: false,
            uni_matched: false,
            uni_all_matched: false,
        }
    }

    pub fn from_unifications(
        unifications: &'a [CompiledUnification],
        group: &'a XmlUnifyGroup,
    ) -> Self {
        let mut eq_matchers: HashMap<(&str, &str), &PosMatcher> =
            HashMap::with_capacity(unifications.len() * 4);
        let mut declared: HashMap<&str, Vec<&str>> = HashMap::with_capacity(unifications.len());
        for uni in unifications {
            for (eq_type, matcher) in &uni.equivalences {
                eq_matchers
                    .entry((uni.feature.as_str(), eq_type.as_str()))
                    .or_insert(matcher);
                declared
                    .entry(uni.feature.as_str())
                    .or_default()
                    .push(eq_type.as_str());
            }
        }
        let features: Vec<(String, Vec<&'a str>)> = if group.features.is_empty() {
            unifications
                .iter()
                .map(|u| {
                    (
                        u.feature.clone(),
                        declared
                            .get(u.feature.as_str())
                            .cloned()
                            .unwrap_or_default(),
                    )
                })
                .collect()
        } else {
            group
                .features
                .iter()
                .map(|(name, types)| {
                    let resolved: Vec<&'a str> = if types.is_empty() {
                        declared.get(name.as_str()).cloned().unwrap_or_default()
                    } else {
                        types.iter().map(String::as_str).collect()
                    };
                    (name.clone(), resolved)
                })
                .collect()
        };
        Self::new(features, eq_matchers)
    }

    pub fn is_unified(&mut self, reading: &UnifyReading<'_>, last_reading: bool) -> bool {
        if self.in_unification {
            self.uni_matched |= self.is_satisfied(reading);
            self.uni_all_matched = self.uni_matched;
            if last_reading {
                self.start_next_token();
                self.uni_matched = false;
            }
            self.uni_all_matched && self.get_final_unification_value()
        } else {
            self.is_satisfied(reading);
            if last_reading {
                self.in_unification = true;
                self.uni_matched = false;
                self.all_feats_in = true;
            }
            true
        }
    }

    fn is_satisfied(&mut self, reading: &UnifyReading<'_>) -> bool {
        let tags = &reading.tags;
        if self.all_feats_in && self.equiv_matched.is_empty() {
            return false;
        }
        if self.all_feats_in {
            return self.check_next(reading);
        }
        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        for (feature, types) in &self.features {
            let mut matched = HashSet::new();
            for eq_type in types {
                let Some(matcher) = self.eq_matchers.get(&(feature.as_str(), *eq_type)) else {
                    return false;
                };
                if tags.iter().any(|t| matcher.is_match(t)) {
                    matched.insert((*eq_type).to_string());
                }
            }
            if matched.is_empty() {
                return false;
            }
            map.insert(feature.clone(), matched);
        }
        let map_idx = self.maps.len();
        self.maps.push(map);
        if self.slots.is_empty() {
            self.slots.push(vec![map_idx]);
            self.slot_ids.push(vec![reading.id]);
        } else {
            self.slots[0].push(map_idx);
            self.slot_ids[0].push(reading.id);
        }
        self.equiv_matched.push(map_idx);
        self.tok_cnt += 1;
        true
    }

    fn check_next(&mut self, reading: &UnifyReading<'_>) -> bool {
        let tags = &reading.tags;
        let mut any_feat_unified = false;
        let mut matched_here: HashMap<String, HashSet<String>> = HashMap::new();
        for i in 0..self.tok_cnt {
            let mut all_feats_unified = true;
            for (feature, types) in &self.features {
                let mut feat_unified = false;
                let branch = &self.maps[self.equiv_matched[i]];
                let set = branch.get(feature);
                for eq_type in types {
                    if set.is_some_and(|s| s.contains(*eq_type))
                        && self
                            .eq_matchers
                            .get(&(feature.as_str(), *eq_type))
                            .is_some_and(|m| tags.iter().any(|t| m.is_match(t)))
                    {
                        feat_unified = true;
                        self.kept
                            .entry(feature.clone())
                            .or_default()
                            .insert((*eq_type).to_string());
                        matched_here
                            .entry(feature.clone())
                            .or_default()
                            .insert((*eq_type).to_string());
                    }
                }
                all_feats_unified &= feat_unified;
            }
            any_feat_unified |= all_feats_unified;
        }
        if any_feat_unified {
            let map_idx = self.maps.len();
            self.maps.push(matched_here);
            if self.slots.len() == self.readings_counter {
                self.slots.push(vec![map_idx]);
                self.slot_ids.push(vec![reading.id]);
            } else if self.readings_counter < self.slots.len() {
                self.slots[self.readings_counter].push(map_idx);
                self.slot_ids[self.readings_counter].push(reading.id);
            } else {
                return false;
            }
        }
        any_feat_unified
    }

    fn start_next_token(&mut self) {
        self.readings_counter += 1;
        let Unifier {
            maps, slots, kept, ..
        } = self;
        for slot in slots.iter_mut() {
            for idx in slot.iter().copied() {
                let map = &mut maps[idx];
                map.retain(|feature, types| {
                    if feature.as_str() == UNIFY_IGNORE {
                        return true;
                    }
                    match kept.get(feature) {
                        Some(kept_types) => {
                            types.retain(|t| kept_types.contains(t));
                            true
                        }
                        None => false,
                    }
                });
            }
        }
        self.kept.clear();
    }

    pub fn add_neutral(&mut self, readings: usize) {
        let map_idx = self.maps.len();
        self.maps
            .push(std::iter::once((UNIFY_IGNORE.to_string(), HashSet::new())).collect());
        self.slots.push(vec![map_idx; readings]);
        self.slot_ids.push((0..readings as u32).collect());
        self.readings_counter += 1;
    }

    pub fn get_final_unification_value(&self) -> bool {
        let n_feats = self.features.len();
        let mut tok_unified = 0usize;
        for (j, slot) in self.slots.iter().enumerate() {
            let mut unified_tokens_found = false;
            for (i, &map_idx) in slot.iter().enumerate() {
                let map = &self.maps[map_idx];
                if map.contains_key(UNIFY_IGNORE) {
                    if i == 0 {
                        tok_unified += 1;
                    }
                    unified_tokens_found = true;
                    continue;
                }
                let mut feat_unified = 0usize;
                for (feature, _) in &self.features {
                    match map.get(feature) {
                        Some(s) if s.is_empty() => feat_unified = 0,
                        _ => feat_unified += 1,
                    }
                    if feat_unified == n_feats && tok_unified <= j {
                        tok_unified += 1;
                        unified_tokens_found = true;
                        break;
                    }
                }
            }
            if !unified_tokens_found {
                return false;
            }
        }
        tok_unified == self.slots.len()
    }

    pub fn unified_readings(&self) -> Option<Vec<Vec<u32>>> {
        let n_feats = self.features.len();
        let mut out: Vec<Vec<u32>> = Vec::with_capacity(self.slots.len());
        for (j, slot) in self.slots.iter().enumerate() {
            let mut unified_tokens_found = false;
            let mut ids: Vec<u32> = Vec::with_capacity(slot.len());
            for (i, &map_idx) in slot.iter().enumerate() {
                let map = &self.maps[map_idx];
                if map.contains_key(UNIFY_IGNORE) {
                    if let Some(&id) = self.slot_ids[j].get(i) {
                        ids.push(id);
                    }
                    unified_tokens_found = true;
                    continue;
                }
                let mut feat_unified = 0usize;
                for (feature, _) in &self.features {
                    match map.get(feature) {
                        Some(s) if s.is_empty() => feat_unified = 0,
                        _ => feat_unified += 1,
                    }
                    if feat_unified == n_feats {
                        if let Some(&id) = self.slot_ids[j].get(i) {
                            ids.push(id);
                        }
                        unified_tokens_found = true;
                    }
                }
            }
            if !unified_tokens_found {
                return None;
            }
            out.push(ids);
        }
        Some(out)
    }
}

pub fn eval_unify_plans(
    plans: &[UnifyElemPlan<'_>],
    plan_idx: usize,
    last_elem: usize,
    negate: bool,
    uni: &mut Unifier<'_>,
) -> bool {
    let Some(plan) = plans.get(plan_idx) else {
        return true;
    };
    if plan.reading_sets.is_empty() {
        for count in &plan.neutral_readings {
            uni.add_neutral(*count);
        }
        return eval_unify_plans(plans, plan_idx + 1, last_elem, negate, uni);
    }
    for variant in &plan.reading_sets {
        let mut branch_uni = if plan.reading_sets.len() > 1 {
            Some(uni.clone())
        } else {
            None
        };
        let active = branch_uni.as_mut().unwrap_or(uni);
        let mut failed = false;
        let total_sets = variant.len();
        for (s, set) in variant.iter().enumerate() {
            let total = set.len();
            let mut any_matched = false;
            for (i, reading) in set.iter().enumerate() {
                any_matched |= active.is_unified(reading, i + 1 == total);
            }
            if plan.elem_idx != last_elem {
                continue;
            }
            if negate && any_matched {
                failed = true;
                break;
            }
            if s + 1 == total_sets && !any_matched && !negate {
                failed = true;
                break;
            }
        }
        if failed {
            continue;
        }
        if eval_unify_plans(plans, plan_idx + 1, last_elem, negate, active) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_ignore_case_matches_java_equalsignorecase_for_ascii() {
        assert!(eq_ignore_case("Hello", "hello"));
        assert!(!eq_ignore_case("Hello", "hell"));
        assert!(eq_ignore_case("", ""));
    }

    #[test]
    fn eq_ignore_case_folds_dotless_and_dotted_i_like_java() {
        assert!(
            eq_ignore_case("i", "ı"),
            "String.equalsIgnoreCase folds i with ı via toUpperCase"
        );
        assert!(eq_ignore_case("İ", "i"));
        assert!(eq_ignore_case("İ", "I"));
        assert!(
            eq_ignore_case("ı", "İ"),
            "StringLatin1.regionMatchesCI compares toLower(toUpper(x)) so ı equals İ"
        );
        assert!(eq_ignore_case("bıg", "BIG"));
        assert!(!eq_ignore_case("bıg", "beg"));
    }

    #[test]
    fn eq_ignore_case_uses_per_char_mapping_not_full_lowercase() {
        assert!(!eq_ignore_case("ß", "SS"));
        assert!(eq_ignore_case("ß", "ß"));
        assert!(eq_ignore_case("ſ", "s"));
        assert!(eq_ignore_case("K", "k"));
        assert!(!eq_ignore_case("ab", "abc"));
    }

    use crate::compiler::CompiledPatternToken;
    use crate::compiler::{CompiledUnification, PosMatcher};
    use crate::types::XmlUnifyGroup;

    fn gender_unifications() -> Vec<CompiledUnification> {
        vec![CompiledUnification {
            feature: "gender".to_string(),
            equivalences: vec![
                (
                    "mas".to_string(),
                    PosMatcher::new_anchored(".*:MAS.*").unwrap(),
                ),
                (
                    "fem".to_string(),
                    PosMatcher::new_anchored(".*:FEM.*").unwrap(),
                ),
            ],
        }]
    }

    fn gender_number_unifications() -> Vec<CompiledUnification> {
        vec![
            CompiledUnification {
                feature: "gender".to_string(),
                equivalences: vec![
                    (
                        "mas".to_string(),
                        PosMatcher::new_anchored(".*:MAS.*").unwrap(),
                    ),
                    (
                        "fem".to_string(),
                        PosMatcher::new_anchored(".*:FEM.*").unwrap(),
                    ),
                ],
            },
            CompiledUnification {
                feature: "number".to_string(),
                equivalences: vec![
                    (
                        "sg".to_string(),
                        PosMatcher::new_anchored(".*:SIN.*").unwrap(),
                    ),
                    (
                        "pl".to_string(),
                        PosMatcher::new_anchored(".*:PLU.*").unwrap(),
                    ),
                ],
            },
        ]
    }

    fn reading(tags: &[&'static str], id: u32) -> UnifyReading<'static> {
        UnifyReading {
            tags: tags.to_vec(),
            id,
        }
    }

    #[test]
    fn tokens_sharing_a_feature_unify() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let mut uni = Unifier::from_unifications(&unifications, &group);

        let det = reading(&["ART:AKK:MAS:SIN"], 0);
        let noun = reading(&["SUB:AKK:MAS:SIN"], 0);
        assert!(uni.is_unified(&det, true));
        assert!(uni.is_unified(&noun, true));
        assert_eq!(uni.unified_readings(), Some(vec![vec![0], vec![0]]));
    }

    #[test]
    fn tokens_with_conflicting_features_do_not_unify() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let mut uni = Unifier::from_unifications(&unifications, &group);

        let det = reading(&["ART:AKK:MAS:SIN"], 0);
        let noun = reading(&["SUB:AKK:FEM:SIN"], 0);
        assert!(uni.is_unified(&det, true));
        assert!(!uni.is_unified(&noun, true));
    }

    #[test]
    fn ambiguous_readings_keep_the_matching_branch() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let mut uni = Unifier::from_unifications(&unifications, &group);

        let det_mas = reading(&["ART:NOM:MAS:SIN"], 0);
        let det_fem = reading(&["ART:NOM:FEM:SIN"], 1);
        let noun_fem = reading(&["SUB:NOM:FEM:SIN"], 0);
        assert!(uni.is_unified(&det_mas, false));
        assert!(uni.is_unified(&det_fem, true));
        assert!(uni.is_unified(&noun_fem, true));
        assert_eq!(uni.unified_readings(), Some(vec![vec![1], vec![0]]));
    }

    #[test]
    fn neutral_elements_keep_the_sequence_unified() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let mut uni = Unifier::from_unifications(&unifications, &group);

        let det = reading(&["ART:AKK:MAS:SIN"], 0);
        let noun = reading(&["SUB:AKK:MAS:SIN"], 1);
        assert!(uni.is_unified(&det, true));
        uni.add_neutral(1);
        assert!(uni.is_unified(&noun, true));
        assert_eq!(
            uni.unified_readings(),
            Some(vec![vec![0], vec![0], vec![1]])
        );
    }

    #[test]
    fn a_reading_missing_the_second_feature_blocks_unification() {
        let unifications = gender_number_unifications();
        let group = XmlUnifyGroup::default();
        let mut uni = Unifier::from_unifications(&unifications, &group);

        let det = reading(&["ART:AKK:MAS:SIN"], 0);
        let same_gender_other_number = reading(&["SUB:AKK:MAS:PLU"], 0);
        assert!(uni.is_unified(&det, true));
        assert!(!uni.is_unified(&same_gender_other_number, true));
    }

    #[test]
    fn explicit_group_features_restrict_the_unified_types() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup {
            features: vec![("gender".to_string(), vec!["mas".to_string()])],
            ..XmlUnifyGroup::default()
        };
        let mut uni = Unifier::from_unifications(&unifications, &group);

        let det = reading(&["ART:AKK:MAS:SIN"], 0);
        let noun_fem = reading(&["SUB:AKK:FEM:SIN"], 0);
        assert!(uni.is_unified(&det, true));
        assert!(!uni.is_unified(&noun_fem, true));
    }

    #[test]
    fn eval_unify_plans_accepts_a_two_element_gender_sequence() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let plans = vec![
            UnifyElemPlan {
                elem_idx: 0,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["ART:AKK:MAS:SIN"], 0)]]],
            },
            UnifyElemPlan {
                elem_idx: 1,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["SUB:AKK:MAS:SIN"], 0)]]],
            },
        ];
        let mut uni = Unifier::from_unifications(&unifications, &group);
        assert!(eval_unify_plans(&plans, 0, 1, false, &mut uni));
    }

    #[test]
    fn eval_unify_plans_rejects_conflicting_genders() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let plans = vec![
            UnifyElemPlan {
                elem_idx: 0,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["ART:AKK:MAS:SIN"], 0)]]],
            },
            UnifyElemPlan {
                elem_idx: 1,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["SUB:AKK:FEM:SIN"], 0)]]],
            },
        ];
        let mut uni = Unifier::from_unifications(&unifications, &group);
        assert!(!eval_unify_plans(&plans, 0, 1, false, &mut uni));
    }

    #[test]
    fn eval_unify_plans_negate_passes_only_when_features_conflict() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup {
            negate: true,
            ..XmlUnifyGroup::default()
        };
        let conflicting = vec![
            UnifyElemPlan {
                elem_idx: 0,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["ART:AKK:MAS:SIN"], 0)]]],
            },
            UnifyElemPlan {
                elem_idx: 1,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["SUB:AKK:FEM:SIN"], 0)]]],
            },
        ];
        let mut uni = Unifier::from_unifications(&unifications, &group);
        assert!(eval_unify_plans(&conflicting, 0, 1, true, &mut uni));

        let agreeing = vec![
            UnifyElemPlan {
                elem_idx: 0,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["ART:AKK:MAS:SIN"], 0)]]],
            },
            UnifyElemPlan {
                elem_idx: 1,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["SUB:AKK:MAS:SIN"], 0)]]],
            },
        ];
        let mut uni = Unifier::from_unifications(&unifications, &group);
        assert!(!eval_unify_plans(&agreeing, 0, 1, true, &mut uni));
    }

    #[test]
    fn eval_unify_plans_walks_neutral_elements_between_unified_tokens() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let plans = vec![
            UnifyElemPlan {
                elem_idx: 0,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["ART:AKK:MAS:SIN"], 0)]]],
            },
            UnifyElemPlan {
                elem_idx: 1,
                neutral_readings: vec![1],
                reading_sets: Vec::new(),
            },
            UnifyElemPlan {
                elem_idx: 2,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["SUB:AKK:MAS:SIN"], 0)]]],
            },
        ];
        let mut uni = Unifier::from_unifications(&unifications, &group);
        assert!(eval_unify_plans(&plans, 0, 2, false, &mut uni));
        assert_eq!(
            uni.unified_readings(),
            Some(vec![vec![0], vec![0], vec![0]])
        );
    }

    #[test]
    fn eval_unify_plans_falls_back_to_the_next_or_group_variant() {
        let unifications = gender_unifications();
        let group = XmlUnifyGroup::default();
        let plans = vec![
            UnifyElemPlan {
                elem_idx: 0,
                neutral_readings: Vec::new(),
                reading_sets: vec![
                    vec![vec![reading(&["ART:AKK:FEM:SIN"], 0)]],
                    vec![vec![reading(&["ART:AKK:MAS:SIN"], 0)]],
                ],
            },
            UnifyElemPlan {
                elem_idx: 1,
                neutral_readings: Vec::new(),
                reading_sets: vec![vec![vec![reading(&["SUB:AKK:MAS:SIN"], 0)]]],
            },
        ];
        let mut uni = Unifier::from_unifications(&unifications, &group);
        assert!(eval_unify_plans(&plans, 0, 1, false, &mut uni));
    }

    #[test]
    fn reading_matches_element_combines_regexp_text_and_postag() {
        let re = std::sync::Arc::new(regex::Regex::new("h.t").unwrap());
        let ci = std::sync::Arc::new(regex::Regex::new("(?i)h.t").unwrap());
        let mut pattern = CompiledPatternToken {
            unify_neutral: false,
            is_regexp: true,
            text: None,
            compiled_regexp: Some(re),
            compiled_regexp_ci: Some(ci),
            compiled_regexp_fancy: None,
            postag: Some("VVFIN".to_string()),
            compiled_postag_regexp: None,
            compiled_postag_regexp_fancy: None,
            negate: false,
            negate_pos: false,
            case_sensitive: false,
            inflected: false,
            min: None,
            max: None,
            skip: 0,
            exceptions: Vec::new(),
            space_before: None,
            chunk: None,
            chunk_re: None,
            match_no: None,
            match_no_case_conversion: None,
            match_no_postag: None,
            match_no_postag_regexp: false,
            match_no_postag_replace: None,
            match_no_setpos: false,
            match_no_regexp_replace: None,
            compiled_match_no_regex: None,
        };
        let token =
            og_core::AnalyzedToken::new("hat", 0, 3).with_pos_tags(vec!["VVFIN".to_string()]);
        assert!(reading_matches_element(&pattern, "hat", &token, false));
        assert!(reading_matches_element(&pattern, "HAT", &token, false));

        pattern.case_sensitive = true;
        assert!(!reading_matches_element(&pattern, "HAT", &token, false));
        assert!(reading_matches_element(&pattern, "hat", &token, false));

        pattern.postag = Some("NN".to_string());
        assert!(!reading_matches_element(&pattern, "hat", &token, false));
        pattern.negate_pos = true;
        assert!(reading_matches_element(&pattern, "hat", &token, false));

        pattern.postag = Some("UNKNOWN".to_string());
        pattern.negate_pos = false;
        assert!(!reading_matches_element(&pattern, "hat", &token, false));
        assert!(reading_matches_element(&pattern, "hat", &token, true));
    }

    fn base_pattern() -> CompiledPatternToken {
        CompiledPatternToken {
            unify_neutral: false,
            is_regexp: false,
            text: None,
            compiled_regexp: None,
            compiled_regexp_ci: None,
            compiled_regexp_fancy: None,
            postag: None,
            compiled_postag_regexp: None,
            compiled_postag_regexp_fancy: None,
            negate: false,
            negate_pos: false,
            case_sensitive: false,
            inflected: false,
            min: None,
            max: None,
            skip: 0,
            exceptions: Vec::new(),
            space_before: None,
            chunk: None,
            chunk_re: None,
            match_no: None,
            match_no_case_conversion: None,
            match_no_postag: None,
            match_no_postag_regexp: false,
            match_no_postag_replace: None,
            match_no_setpos: false,
            match_no_regexp_replace: None,
            compiled_match_no_regex: None,
        }
    }

    #[test]
    fn normalize_apostrophes_and_non_ascii_case_helpers_follow_java() {
        assert_eq!(normalize_apostrophes("l’été"), "l'été");
        assert_eq!(normalize_apostrophes("‘‛’"), "'''");
        assert_eq!(normalize_apostrophes("plain").as_ref(), "plain");
        assert!(!eq_ignore_case("aı", "aıx"));
        assert!(!eq_ignore_case("aıx", "aı"));
        assert!(!eq_ignore_case("ſ", "f"));
        assert!(!eq_ignore_case("ß", "ss"));
    }

    #[test]
    fn surface_and_tag_helpers_classify_sentence_tokens() {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let start = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        assert_eq!(surface_text(&start), "");
        let word = AnalyzedTokenReadings::new(AnalyzedToken::new("Wort", 0, 4));
        assert_eq!(surface_text(&word), "Wort");

        let untagged = AnalyzedTokenReadings::new(AnalyzedToken::new("x", 0, 1));
        assert!(has_no_tag_reading(&untagged));
        let mut ended = AnalyzedTokenReadings::new(AnalyzedToken::new("x", 0, 1));
        ended.add_sent_end();
        assert!(has_no_tag_reading(&ended));
        let tagged = AnalyzedTokenReadings::new(
            AnalyzedToken::new("x", 0, 1).with_pos_tags(vec!["NN".to_string()]),
        );
        assert!(!has_no_tag_reading(&tagged));
    }

    #[test]
    fn reading_pos_matches_supports_regex_fancy_and_unknown_forms() {
        let tags = vec!["ART:DEF:NOM:SIN:MAS".to_string()];
        let plain = std::sync::Arc::new(regex::Regex::new(".*:NOM.*").unwrap());
        assert!(reading_pos_matches("", Some(&plain), None, &tags, false));
        let unknown_re = std::sync::Arc::new(regex::Regex::new("UNKNOWN").unwrap());
        assert!(reading_pos_matches("", Some(&unknown_re), None, &[], true));
        assert!(!reading_pos_matches(
            "",
            Some(&unknown_re),
            None,
            &[],
            false
        ));

        let fancy = std::sync::Arc::new(fancy_regex::Regex::new(".*:(NOM|AKK).*").unwrap());
        assert!(reading_pos_matches("", None, Some(&fancy), &tags, false));
        let fancy_unknown = std::sync::Arc::new(fancy_regex::Regex::new("^UNKNOWN$").unwrap());
        assert!(reading_pos_matches(
            "",
            None,
            Some(&fancy_unknown),
            &[],
            true
        ));
        assert!(!reading_pos_matches(
            "",
            None,
            Some(&fancy_unknown),
            &[],
            false
        ));
        assert!(reading_pos_matches(
            "ART:DEF:NOM:SIN:MAS",
            None,
            None,
            &tags,
            false
        ));
    }

    #[test]
    fn reading_matches_element_covers_text_fancy_and_lemma_paths() {
        let mut pattern = base_pattern();
        pattern.text = Some("l’été".to_string());
        let reading = og_core::AnalyzedToken::new("l'été", 0, 5);
        assert!(reading_matches_element(&pattern, "l'été", &reading, false));
        assert!(!reading_matches_element(
            &pattern, "l'hiver", &reading, false
        ));

        let mut cs = base_pattern();
        cs.text = Some("Straße".to_string());
        cs.case_sensitive = true;
        assert!(reading_matches_element(&cs, "Straße", &reading, false));
        assert!(!reading_matches_element(&cs, "straße", &reading, false));

        let mut inflected = base_pattern();
        inflected.text = Some("Haus".to_string());
        inflected.inflected = true;
        let lemma_reading = og_core::AnalyzedToken::new("Häuser", 0, 6).with_lemma("Haus");
        assert!(reading_matches_element(
            &inflected,
            "Häuser",
            &lemma_reading,
            false
        ));

        let mut fancy_pattern = base_pattern();
        fancy_pattern.compiled_regexp_fancy = Some(std::sync::Arc::new(
            fancy_regex::Regex::new("colou?r").unwrap(),
        ));
        assert!(reading_matches_element(
            &fancy_pattern,
            "color",
            &reading,
            false
        ));
        assert!(!reading_matches_element(
            &fancy_pattern,
            "couleur",
            &reading,
            false
        ));

        let mut re_only = base_pattern();
        re_only.compiled_regexp = Some(std::sync::Arc::new(regex::Regex::new("h.t").unwrap()));
        assert!(reading_matches_element(&re_only, "hat", &reading, false));

        let mut postag_only = base_pattern();
        postag_only.postag = Some("NN".to_string());
        let nn_reading =
            og_core::AnalyzedToken::new("hat", 0, 3).with_pos_tags(vec!["NN".to_string()]);
        assert!(reading_matches_element(
            &postag_only,
            "hat",
            &nn_reading,
            false
        ));
        postag_only.negate_pos = true;
        assert!(!reading_matches_element(
            &postag_only,
            "hat",
            &nn_reading,
            false
        ));

        let mut negated = base_pattern();
        negated.text = Some("hat".to_string());
        negated.negate = true;
        assert!(!reading_matches_element(&negated, "hat", &reading, false));
        assert!(reading_matches_element(&negated, "cat", &reading, false));
    }

    #[test]
    fn from_unifications_resolves_explicit_group_feature_types() {
        let unifications = gender_number_unifications();
        let group = XmlUnifyGroup {
            negate: false,
            features: vec![
                ("gender".to_string(), vec!["mas".to_string()]),
                ("number".to_string(), Vec::new()),
            ],
            token_start: 0,
            token_end: 2,
            elem_start: 0,
            elem_end: 2,
        };
        let mut uni = Unifier::from_unifications(&unifications, &group);
        let det = reading(&["ART:AKK:MAS:SIN"], 0);
        let noun = reading(&["SUB:AKK:MAS:SIN"], 1);
        assert!(uni.is_unified(&det, true));
        assert!(uni.is_unified(&noun, true));
        assert_eq!(uni.unified_readings(), Some(vec![vec![0], vec![1]]));

        let fem_group = XmlUnifyGroup {
            negate: false,
            features: vec![("gender".to_string(), vec!["fem".to_string()])],
            token_start: 0,
            token_end: 2,
            elem_start: 0,
            elem_end: 2,
        };
        let mut fem = Unifier::from_unifications(&unifications, &fem_group);
        let die = reading(&["ART:AKK:FEM:SIN"], 0);
        let tuer = reading(&["SUB:AKK:FEM:SIN"], 1);
        assert!(fem.is_unified(&die, true));
        assert!(fem.is_unified(&tuer, true));
    }
}
