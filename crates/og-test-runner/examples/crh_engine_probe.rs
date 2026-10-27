fn main() {
    let engine =
        og_langs::engine::LanguageEngine::for_code("crh").expect("crh engine must be registered");
    println!(
        "crh engine: {} rules, {} text-level",
        engine.rule_count(),
        engine.text_level_rule_ids().len()
    );
    for id in engine.rule_ids() {
        println!("  rule: {id}");
    }
    for id in engine.text_level_rule_ids() {
        println!("  text-level: {id}");
    }
    let result = engine.check("Özü bir çaqrım uzaqta  , demiryol keçidi yanında yaşay.");
    println!(
        "matches on double-space+comma sentence: {}",
        result.matches.len()
    );
}
