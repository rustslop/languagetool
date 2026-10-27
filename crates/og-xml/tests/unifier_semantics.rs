use og_xml::compiler::PosMatcher;
use og_xml::unifier::{Unifier, UnifyElemPlan, UnifyReading, eval_unify_plans};
use std::collections::HashMap;

fn m(p: &str) -> PosMatcher {
    PosMatcher::new_anchored(&format!("^(?:{})$", p)).unwrap()
}

#[test]
fn readings_that_do_not_unify_fail_the_sequence() {
    let number = vec![
        ("sg".to_string(), m(".*:SIN:.*")),
        ("pl".to_string(), m(".*:PLU:.*")),
    ];
    let case = vec![
        ("nom".to_string(), m(".*:NOM:.*")),
        ("akk".to_string(), m(".*:AKK:.*")),
        ("dat".to_string(), m(".*:DAT:.*")),
        ("gen".to_string(), m(".*:GEN:.*")),
    ];
    let gender = vec![
        ("mas".to_string(), m(".*:(MAS|NOG).*")),
        ("fem".to_string(), m(".*:(FEM|NOG).*")),
        ("neu".to_string(), m(".*:(NEU|NOG).*")),
    ];
    let all = [("number", &number), ("case", &case), ("gender", &gender)];
    let mut features: Vec<(String, Vec<&str>)> = Vec::new();
    let mut eq: HashMap<(&str, &str), &PosMatcher> = HashMap::new();
    for (fname, defs) in &all {
        let mut tys = Vec::new();
        for (t, re) in defs.iter() {
            eq.insert((fname, t.as_str()), re);
            tys.push(t.as_str());
        }
        features.push(((*fname).to_string(), tys));
    }
    let der = [
        "ART:DEF:DAT:SIN:FEM",
        "ART:DEF:GEN:PLU:FEM",
        "ART:DEF:GEN:PLU:MAS",
        "ART:DEF:GEN:PLU:NEU",
        "ART:DEF:GEN:SIN:FEM",
        "ART:DEF:NOM:SIN:MAS",
        "PRO:DEM:DAT:SIN:FEM",
        "PRO:DEM:NOM:SIN:MAS",
        "PRO:PER:NOM:SIN:MAS",
    ];
    let harz = ["SUB:AKK:SIN:NEU", "SUB:DAT:SIN:NEU", "SUB:NOM:SIN:NEU"];
    let plans = vec![
        UnifyElemPlan {
            elem_idx: 0,
            neutral_readings: vec![],
            reading_sets: vec![vec![
                der.iter()
                    .enumerate()
                    .map(|(i, t)| UnifyReading {
                        tags: vec![t],
                        id: i as u32,
                    })
                    .collect(),
            ]],
        },
        UnifyElemPlan {
            elem_idx: 1,
            neutral_readings: vec![],
            reading_sets: vec![vec![
                harz.iter()
                    .enumerate()
                    .map(|(i, t)| UnifyReading {
                        tags: vec![t],
                        id: i as u32,
                    })
                    .collect(),
            ]],
        },
    ];
    let mut uni = Unifier::new(features, eq);
    let ok = eval_unify_plans(&plans, 0, 1, false, &mut uni);
    println!("ok={ok} survivors={:?}", uni.unified_readings());
    assert!(!ok, "Der+Harz must not unify");
}
