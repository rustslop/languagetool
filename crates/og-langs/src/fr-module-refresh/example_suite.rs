use std::collections::HashMap;

use og_xml::types::XmlExampleType;

use super::support::compiled_rules;
use super::support::grammar_available;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
}

const PINNED_EVALUATED: usize = 15293;
const PINNED_PASSED: usize = 15260;
const PINNED_RESIDUES: &[(&str, &str)] = &[
    ("IL_DEVIENT_QUOI", "Il devient de plus en plus."),
    ("ALORS", "alors il faut partir"),
    ("DANS_CE_CAS", "dans ce cas il est certain"),
    (
        "ADJ_ADJ_SENT_END",
        "Ses grandes fleurs doubles sont roses à nuance lilas et très parfumées.",
    ),
    (
        "PEUT_PEUX_PEU",
        "Quelqu\"un peut -il m'éclairer et si possible en justifiant.",
    ),
    (
        "OU",
        "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
    ),
    ("CE_SE", "Il faut tout ce dire."),
    ("CE_SE", "C'est lui qui ce cache sous la table."),
    ("CE_SE", "Cela ce passe sous la table."),
    ("CE_SE", "Qu'attent-il pour ce lancer ?"),
    ("CE_SE", "Il ne le fait pas pour ce repentir."),
    ("CE_SE", "Il a décidé de ce lever tôt."),
    ("VOIX_VOIE", "Le candidat a obtenu 51% des voies."),
    ("DU_DU", "C'est trop, tu n'aurais pas du savoir !"),
    ("MOTS_COLLES_OU_SEPARES", "Il le fait poursoi."),
    ("ETRE_VPPA_OU_ADJ", "De leur rencontre est donc né ce lieu."),
    ("ACCORD_SUJET_VERBE", "De quels sites parles -tu?"),
    ("PREFIXES_NON_SEPARES", "Pour revenir au post précédent."),
    (
        "PREFIXES_NON_SEPARES",
        "Miguel Diaz-Canel est porté sur les fonts néo-post-castristes.",
    ),
    (
        "PREFIXES_NON_SEPARES",
        "Elle poursuit les séries et aligne sur la post saison 14,3 points",
    ),
    ("ESPACE_POURCENT", "Une croissance de 8%."),
    ("ESPACE_POURCENT", "Une croissance de 8,5%."),
    ("PAS_DE_TRAIT_UNION", "Le néo-impressionnisme"),
    (
        "TRAIT_UNION_ESPACE",
        "Vas -tu les regarder à la télévision\u{202f}?",
    ),
    ("TIRET", "-il est arrivé à l'hôtel hier."),
    ("DATE", "Il est né le 32.1.1985."),
    ("DATE", "Il est né le 300.1.1985."),
    ("DATE", "Il est né le 3000.1.1985."),
    ("DATE", "Elle est née le 31.4.1989."),
    ("DATE", "Elle est née le 30.2.1900."),
    ("DATE", "Elle est née le 31.2.1900."),
    ("DATE", "Elle est née le 29.2.1900."),
    ("DATE", "Le 29.2.2100."),
];

fn strip_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(start), Some(end)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((start, end - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}

fn evaluate_all() -> Vec<(String, String, Option<Outcome>)> {
    use rayon::prelude::*;
    let rule_set = compiled_rules();
    let engine = super::support::pattern_engine();
    let pipeline = crate::fr::test_support::FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut groups: Vec<Vec<&og_xml::compiler::CompiledRule>> = Vec::new();
    for rule in &rule_set.rules {
        let key = (rule.id.clone(), rule.sub_id.clone());
        let next = groups.len();
        let idx = *group_index.entry(key).or_insert(next);
        if idx == next {
            groups.push(vec![rule]);
        } else {
            groups[idx].push(rule);
        }
    }

    groups
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect::<Vec<_>>()
        .par_iter()
        .map(|(group, example)| {
            let (clean_text, marker_range) = strip_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group.iter() {
                let fr_filter = aux.filter.as_ref().filter(|f| {
                    f.class
                        == crate::fr::suppress_misspelled::FRENCH_SUPPRESS_MISSPELLED_FILTER_CLASS
                });
                for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                    for m in engine.match_rule(aux, &sentence) {
                        let Some(mut m) = (match fr_filter {
                            Some(filter) => crate::fr::suppress_misspelled::accept_rule_match(
                                m,
                                &filter.args,
                                &[],
                            ),
                            None => Some(m),
                        }) else {
                            continue;
                        };
                        m.set_offset_length(m.offset() + sentence_start, m.length());
                        matches.push(m);
                    }
                }
            }
            let failure = match example.example_type {
                XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some(Outcome::NoMatch)
                    } else if let Some((m_start, m_end)) = marker_range {
                        let first = &matches[0];
                        let match_start = first.offset();
                        let match_end = match_start + first.length();
                        if match_start <= m_start && match_end >= m_end {
                            None
                        } else {
                            Some(Outcome::Marker)
                        }
                    } else {
                        None
                    }
                }
                XmlExampleType::Correct => {
                    if matches.is_empty() {
                        None
                    } else {
                        Some(Outcome::FalsePositive)
                    }
                }
                XmlExampleType::TriggersError => None,
            };
            (group[0].id.clone(), clean_text, failure)
        })
        .collect()
}

#[test]
#[ignore]
fn french_refresh_example_suite_is_green_over_accepted_residues() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _release_guard = crate::release_guard();
    let started = std::time::Instant::now();
    let per_example = evaluate_all();

    let evaluated = per_example.len();
    let passed = per_example
        .iter()
        .filter(|(_, _, failure)| failure.is_none())
        .count();
    let mut failures: Vec<(&str, &str)> = per_example
        .iter()
        .filter_map(|(rule, text, failure)| failure.map(|_| (rule.as_str(), text.as_str())))
        .collect();
    failures.sort_unstable();
    let mut pinned: Vec<(&str, &str)> = PINNED_RESIDUES.to_vec();
    pinned.sort_unstable();

    assert_eq!(
        evaluated, PINNED_EVALUATED,
        "the 4cff7008 grammar.xml must evaluate its 15293 live first-rule examples (16792 raw \
         <example> tags, 170 inside comments, none inside antipatterns)"
    );
    assert_eq!(
        passed, PINNED_PASSED,
        "passed count pinned at the 4cff7008 refresh (upstream ec2ddb171fa + b650fda3881)"
    );
    assert_eq!(
        failures, pinned,
        "every failing example must be an explicitly accepted-excluded residue of the \
         crate::fr::residue_map inventory (these 33 of its 41 entries still fail; the other 8 \
         already pass), so the refreshed PARENTHESES[5] and NEGATION_QUE[4] data must not \
         change any example outcome"
    );
    eprintln!(
        "fr refresh suite: {passed}/{evaluated} passed in {:.1}s, {} accepted residues",
        started.elapsed().as_secs_f64(),
        pinned.len()
    );
}
