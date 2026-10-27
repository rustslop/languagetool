use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let text = std::env::args().nth(1).unwrap();
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="ru">
    <rulegroup id="NN_N_pril_prich" name="t">
        <rule>
            <pattern>
                <marker>
                    <token postag="ADJ:.*" postag_regexp="yes" regexp="yes">.*[аеёиоуэюя][н](ый|ым|ых|ого|ому|ой|ая|ое|ою|ые|ым|ых|ом)<exception  postag="ADJ:Comp" /><exception  negate_pos="yes" postag="ADJ:.*" postag_regexp="yes"></exception><exception>зеленой</exception></token>
                </marker>
                <token  postag="PREP" skip="1" ></token>
                <token postag="NN:.*:.*:.*:P" postag_regexp="yes"></token>
            </pattern>
            <message>m</message>
            <example>Во дворце состоялось объединённое заседание.</example>
        </rule>
    </rulegroup>
</rules>"#;
    let runner = TestRunner::for_language("ru");
    let rule_set = XmlCompiler::new().compile_file(xml).unwrap();
    let (clean, _marker, matches) = runner.match_rule_on_text(&rule_set.rules[0], &text);
    println!("text: {clean}");
    for m in &matches {
        let s = m.offset();
        let e = s + m.length();
        println!(
            "match {}..{} => {:?}",
            s,
            e,
            &clean[s.min(clean.len())..e.min(clean.len())]
        );
    }
    let sentence = runner.debug_sentence(&clean);
    for (i, t) in sentence.tokens().iter().enumerate() {
        if t.is_whitespace() {
            continue;
        }
        println!(
            "{i} {:?} => {:?}",
            t.token().token(),
            t.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>()
        );
    }
}
