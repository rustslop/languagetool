use og_rules::pattern_rule::PatternRuleEngine;
use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

const BASE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="de">
<category id="MISC" name="Misc">
<rule id="KOMMA_T" name="t">
<pattern>
<token postag="(VER:[1-3]:.*|VER.*:[1-3]:.*|VER:IMP:.*)" postag_regexp="yes" skip="-1">{EXC1}</token>
<token postag="VER:[1-3]:.*|VER.*:[1-3]:.*|VER:IMP:.*" postag_regexp="yes" regexp="yes" case_sensitive="yes">[a-zäöü]+{EXC2}</token>
</pattern>
<message>m</message>
</rule>
</category>
</rules>"#;

fn run(label: &str, exc1: &str, exc2: &str, runner: &TestRunner, texts: &[&str]) {
    let xml = BASE.replace("{EXC1}", exc1).replace("{EXC2}", exc2);
    let rs = XmlCompiler::new().compile_file(&xml).unwrap();
    let engine = PatternRuleEngine::new();
    for text in texts {
        let s = runner.debug_sentence(text);
        let n = engine.match_rule(&rs.rules[0], &s).len();
        eprintln!("{label} | {text} -> {n}");
    }
}

fn main() {
    let runner = TestRunner::for_language("de");
    let texts = [
        "Ich hoffe ich konnte Ihnen helfen.",
        "Ich denke seine Meinung ist sehr wichtig.",
        "Die Sonne versank hinter dem Horizont die Schatten der Nacht senkten sich über das Land.",
    ];
    run("no-exc", "", "", &runner, &texts);
    run(
        "full-exc",
        r#"
<exception postag="VER:(INF|MOD):.*|VER:AUX:.*:KJ[12]|PA[12]:.*|VER:PA2.*|SUB:.*|EIG:.*|ADJ:.*|ART:.*|PRO:.*|ADV.*|PRP:.*|NEG:.*|ABK:.*|ZUS.*|ZAL.*" postag_regexp="yes"/>
<exception inflected="yes" regexp="yes">bleiben|lassen|versuchen|bemühen|sieben|achten|wetter|feuer|web|hure</exception>
<exception>bitte</exception>
<exception regexp="yes" scope="next">,|\.|:|\?|!|;|-|–|—|─|~|\)|\(|„|“|»|«|"|\*|und|oder|&amp;|…|→|•|=|\/|'|‚|‘</exception>
<exception regexp="yes" scope="next">weil|ob(wohl|gleich)?|da(mit|ss)?|indem|wenn|falls|während|bevor|nachdem|seitdem|so(bald|dass|fern|lange)|was|als|wie|bis|wer|wo|wohin|sondern|aber</exception>
<exception postag="_english_ignore_"/>"#,
        r#"
<exception postag="VER:(INF:|PA2).*|VER:MOD:INF.*|PA[12]:.*|SUB:.*|EIG:.*|ADJ:.*|ART:.*|PRO:.*|ADV.*|PRP:.*|NEG:.*|ABK:.*|ZUS.*|ZAL.*" postag_regexp="yes"/>
<exception inflected="yes" regexp="yes">bleiben|lassen|sieben|achten|wetter|feuer|web|hure</exception>
<exception regexp="yes">bitte|halt|paff|mindere|lichte|sog|rauf|vermittelst</exception>
<exception postag="VER:IMP:SIN:NON" />
<exception regexp="yes" scope="previous">,|\.|:|\?|!|;|-|–|—|\)|\(|„|“|»|«|"|und|oder|da|weil|ob(wohl|gleich)?|damit|sodass|dass|indem|wenn|falls|sofern|während|bevor|nachdem|seitdem|so(bald|lange)|was|als|wie|bis|wer|wo|wohin</exception>
<exception scope="next">,</exception>
<exception scope="next" regexp="yes">['’`´‘ʻ]</exception>
<exception postag="_english_ignore_"/>"#,
        &runner,
        &texts,
    );
}
