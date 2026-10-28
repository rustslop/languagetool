use std::sync::LazyLock;

use og_core::AnalyzedToken;
use regex::Regex;

static P_APOSTROPHE_MASC_YES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:h?[aeiou\u{00e0}\u{00e8}\u{00e9}\u{00ed}\u{00f2}\u{00f3}\u{00fa}].*)$")
        .unwrap()
});
static P_APOSTROPHE_MASC_NO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:h?[ui][aeio\u{00e0}\u{00e8}\u{00e9}\u{00f3}\u{00f2}].+)$").unwrap()
});
static P_APOSTROPHE_FEM_YES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:h?[aeo\u{00e0}\u{00e8}\u{00e9}\u{00ed}\u{00f2}\u{00f3}\u{00fa}].*|h?[ui][^aeiou\u{00e0}\u{00e8}\u{00e9}\u{00ed}\u{00f2}\u{00f3}\u{00fa}\u{00fc}\u{00ef}]+[aeiou][ns]?|urbs)$",
    )
    .unwrap()
});
static P_APOSTROPHE_FEM_NO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:host|ira|inxa)$").unwrap());
static P_HAC_ASPIRADA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:Higgs|high|Hildesheim|Hill|hijabs?|Hillary|Himmler|hip-hop|hippies|hippy|hipsters?|Hirado|His|hits?|Hubei|Hudson|Hunter|Husserl|Huygens|husky|Utah|hides?|honey.*|Hartle.*|happy|happi.*|Hulk|Heart.*|Haakon|Halberstadt|Harley|Huck.*|Hanna|haka|hakes|Hama|Hornbostel|Heidi|Hayao|Hansi|Haas|Hindemith|user|users|one|head|history|Human|Hampshire|Hovedstaden|Handmade|Helm|Hahnem.*|hikimor.*|Houdini|Hugging|Heritage|hardcore|hancock|hender.*|h[ei][zs]b[ou]l.*|harira|Hawth.*|Henk.*|Humphry|Hohle|Höhle|Hooke|hajj.*|Hochschule|Hoch.*|Hutt|Hansel|Henley|hook|Handstand|Hull|Hatshepsut|Hatchepsut|Hana|Hamri|Hanley|Halis|Huxley|Hess|Hatteras|Herzberg|Hanlon|Harriet|hawl.*|hard|hip|herderi.*|Hangouts|Hayes|hostings?|Hal|hajj|Hermann|Hannah|Hertzsprung|Hotmail|Homrani|Harris|Harvey|Hunspell|Hassan|Haddock|Haarle[mn].*|Hainan|haendel.*|händel.*|habermas.*|hadits?|Hanuk?kà|hack.*|Harlem|Harper|Hartford|Haifa|haikus?|haima|haimes|Haikou|halal|halar|Halifax|Halmstad|halls?|Halle|Halley|Hallstatt|Hallstein|Halloweens?|Hals|herr|Herut|Hamadan|Hamas|Hamàs|hamilton.*|Hamlet.*|hammams?|Hammond|Hampton|hàmsters?|h[aà]ndicaps?|Hangzhou|Hannover|Hanoi|Hans|Hansa|hanseàti[cq].*|happenings?|Harbin|hardware|Haneke|harolds?|Hatay|Hamleigh |Harrisburg|Harrison|harrods?|harry|Hartley|Hartmann?|Hartree|Haruki|Har[td]?vard|Harz|hash.*|Hastings|Havel|Havilland|hawai.*|hawk.*|Hayek|Haydn.*|Hayworth|Heard|hearst|Heathrow|heav.*|hegel.*|Hebei|Hedmark|Heerenveen|Hedw.*|Heerlen|Hefei|Heidelberg|Heide[gn].*|Heilbronn|Heilongjiang|Heilig.*|hei[nk].*|Heisen.*|Heitz|Helmand|Helmholtz|Helen|Helsingborg|Hèlsinki|Heming.*|Henan|henna|hennes|Henry|Hepburn|herbert.*|Herder|Hereford|Herford|Herning|Hertfordshire|Herzog|Hesse|Hessen.*|Hewlett.*|H[ie]zbol·?l.+|high.*|hilbert.*|Hilda|Hillingdon|hinden.*|Hilton|hinterlands?|Hirsch.*|Hitch.*|hitler.*|Hilversum|Hobart|Hockenheim|Hodeida|Hohhot|Hokkaido|hobbes.*|hobby|Hogw.*|hobbies|Hodgkin|Hohen.*|Hölderlin|h[òo]ldings?|holy.*|hollywood.*|Holmes.*|Holstein|Hong|Hong-Kong|hongk.+|Honolu.+|Honsh[uū]|h[òo]bbits?|hooligan.*|hoover.*|hopkins|Hork.*|Horowitz|horst|H[ou]f.*|Houla|house|Houston|Howard|Hoyerswerda|Hunan|Huddersfield|Hunedoara|huskys?|huskies|hubs?|Hubble|humbold.*|Hume|hunting.*|Hussein|husseinit.+|Unity|university|united.*|European|OneDrive)$").unwrap()
});

pub fn needs_apostrophation_ms(word: &str) -> bool {
    P_APOSTROPHE_MASC_YES.is_match(word)
        && !P_APOSTROPHE_MASC_NO.is_match(word)
        && !P_HAC_ASPIRADA.is_match(word)
}

pub fn all_need_same_apostrophation_ms(first_word: &str, words: &[&AnalyzedToken]) -> bool {
    if words.is_empty() {
        return true;
    }
    let first = needs_apostrophation_ms(first_word);
    words
        .iter()
        .all(|w| needs_apostrophation_ms(w.lemma().unwrap_or("")) == first)
}

fn prep_det(key: &str) -> Option<&'static str> {
    Some(match key {
        "MS" => "el ",
        "FS" => "la ",
        "MP" => "els ",
        "FP" => "les ",
        "MSapos" => "l'",
        "FSapos" => "l'",
        "aMS" => "al ",
        "aFS" => "a la ",
        "aMP" => "als ",
        "aFP" => "a les ",
        "aMSapos" => "a l'",
        "aFSapos" => "a l'",
        "dMS" => "del ",
        "dFS" => "de la ",
        "dMP" => "dels ",
        "dFP" => "de les ",
        "dMSapos" => "de l'",
        "dFSapos" => "de l'",
        "pMS" => "pel ",
        "pFS" => "per la ",
        "pMP" => "pels ",
        "pFP" => "per les ",
        "pMSapos" => "per l'",
        "pFSapos" => "per l'",
        _ => return None,
    })
}

pub fn get_preposition_and_determiner(
    new_form: &str,
    gender_number: &str,
    preposition: &str,
) -> Option<&'static str> {
    let mut gender_number = gender_number.to_string();
    if gender_number.starts_with('C') {
        gender_number = format!("M{}", gender_number.chars().nth(1).unwrap_or_default());
    }
    if gender_number.ends_with('N') {
        gender_number = format!("{}S", gender_number.chars().next().unwrap_or_default());
    }
    let mut preposition = preposition.to_string();
    if !preposition.is_empty() {
        preposition = preposition
            .chars()
            .next()
            .unwrap_or_default()
            .to_lowercase()
            .collect();
    }
    let mut apos = "";
    if !P_HAC_ASPIRADA.is_match(new_form)
        && ((gender_number == "MS"
            && P_APOSTROPHE_MASC_YES.is_match(new_form)
            && !P_APOSTROPHE_MASC_NO.is_match(new_form))
            || (gender_number == "FS"
                && P_APOSTROPHE_FEM_YES.is_match(new_form)
                && !P_APOSTROPHE_FEM_NO.is_match(new_form)))
    {
        apos = "apos";
    }
    prep_det(&format!("{preposition}{gender_number}{apos}"))
}
