use regex_syntax::ast::parse::Parser;
use regex_syntax::ast::print::Printer;
use regex_syntax::ast::{
    Ast, ClassBracketed, ClassPerl, ClassPerlKind, ClassSet, ClassSetItem, ClassSetRange,
    ClassSetUnion, Literal, LiteralKind, Span,
};

pub fn to_ascii_perl_classes(pattern: &str) -> Option<String> {
    // A Perl class is written `\d`, `\D`, `\w`, `\W`, `\s` or `\S`, so a
    // byte scan without such a pair proves there is nothing this rewrite
    // can change; skip the AST parse + print for those patterns (all but a
    // few percent of rule regexps). An escaped backslash (`\\d`) can trip
    // the scan; a false positive only loses the skip, never correctness.
    if !contains_perl_class_syntax(pattern) {
        return None;
    }
    let mut ast = Parser::new().parse(pattern).ok()?;
    rewrite(&mut ast)?;
    let mut out = String::with_capacity(pattern.len() + 8);
    Printer::new().print(&ast, &mut out).ok()?;
    Some(out)
}

fn contains_perl_class_syntax(pattern: &str) -> bool {
    pattern.as_bytes().windows(2).any(|pair| {
        pair[0] == b'\\' && matches!(pair[1], b'd' | b'D' | b'w' | b'W' | b's' | b'S')
    })
}

fn rewrite(ast: &mut Ast) -> Option<()> {
    match ast {
        Ast::ClassPerl(perl) => {
            *ast = perl_to_bracketed(perl);
            Some(())
        }
        Ast::ClassBracketed(cls) => rewrite_class(cls),
        Ast::Repetition(rep) => rewrite(&mut rep.ast),
        Ast::Group(group) => rewrite(&mut group.ast),
        Ast::Alternation(alt) => alt.asts.iter_mut().try_for_each(rewrite),
        Ast::Concat(cat) => cat.asts.iter_mut().try_for_each(rewrite),
        _ => Some(()),
    }
}

fn rewrite_class(cls: &mut ClassBracketed) -> Option<()> {
    let span = cls.span;
    match &mut cls.kind {
        ClassSet::BinaryOp(_) => None,
        ClassSet::Item(item) => rewrite_class_item(item, span),
    }
}

fn rewrite_class_item(item: &mut ClassSetItem, span: Span) -> Option<()> {
    match item {
        ClassSetItem::Union(union) => union
            .items
            .iter_mut()
            .try_for_each(|i| rewrite_class_item(i, span)),
        ClassSetItem::Perl(perl) if !perl.negated => {
            *item = ClassSetItem::Union(ClassSetUnion {
                span,
                items: class_members(&perl.kind, span),
            });
            Some(())
        }
        ClassSetItem::Perl(_) => None,
        ClassSetItem::Bracketed(inner) => rewrite_class(inner),
        _ => Some(()),
    }
}

fn perl_to_bracketed(perl: &ClassPerl) -> Ast {
    let span = perl.span;
    Ast::ClassBracketed(Box::new(ClassBracketed {
        span,
        negated: perl.negated,
        kind: ClassSet::Item(ClassSetItem::Union(ClassSetUnion {
            span,
            items: class_members(&perl.kind, span),
        })),
    }))
}

fn class_members(kind: &ClassPerlKind, span: Span) -> Vec<ClassSetItem> {
    let ranges: &[((char, char), bool)] = match kind {
        ClassPerlKind::Digit => &[(('0', '9'), false)],
        ClassPerlKind::Word => &[
            (('0', '9'), false),
            (('A', 'Z'), false),
            (('_', '_'), true),
            (('a', 'z'), false),
        ],
        ClassPerlKind::Space => &[
            (('\t', '\n'), true),
            (('\u{b}', '\u{c}'), true),
            (('\r', '\r'), true),
            ((' ', ' '), true),
        ],
    };
    ranges
        .iter()
        .map(|&((start, end), single)| {
            if single {
                ClassSetItem::Literal(Literal {
                    span,
                    kind: LiteralKind::Verbatim,
                    c: start,
                })
            } else {
                ClassSetItem::Range(ClassSetRange {
                    span,
                    start: Literal {
                        span,
                        kind: LiteralKind::Verbatim,
                        c: start,
                    },
                    end: Literal {
                        span,
                        kind: LiteralKind::Verbatim,
                        c: end,
                    },
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod string_matcher_spec_tests {
    use super::to_ascii_perl_classes;

    const REGEXPS: &[&str] = &[
        "[a-z]+|,",
        "['-].+",
        "[^'-].+",
        "\\p{L}",
        "\\p{Ll}.+",
        "\\p{Lu}.+",
        "[a-z]+",
        "\\d+",
        "[1234567890]{1,2}",
        "[1234567890][1234567890., ]+[1234567890]",
        "\\p{P}\\p{P}+",
        "[_~°]+",
        "0o[0-7]+",
        "0x[0-9A-Fa-f]+",
        "[A-Z]+\\d+",
        "#.+",
        "@.+",
        ".+e",
        ".+[aei]sses",
        "a.*b",
        "a.+b",
        "aa?",
        "a{2,3}b",
        "[a-c]?[0-9]*x",
        "(?-i)Han",
        "\\.|\\-",
        "[\\d ,\\.]+",
        "−?[\\d .,]+%?",
        "[a-z]+|,|[0-9]",
        "\\p{Lu}.*",
        "\\w+",
        "\\d?",
        "a|",
        "|a",
        "\\p{N}+",
        "[^a-z]+",
        "[abc]*d",
        "x*",
        "x+",
        "x?",
        "[^\\d]+",
    ];

    fn build(pattern: &str, case_sensitive: bool) -> Option<regex::Regex> {
        let translated = to_ascii_perl_classes(pattern).unwrap_or_else(|| pattern.to_string());
        let anchored = format!("^(?:{translated})$");
        if case_sensitive {
            regex::Regex::new(&anchored).ok()
        } else {
            crate::java_case_fold::compile_java_ci(&anchored)
        }
    }

    #[test]
    fn patterns_without_perl_syntax_skip_the_rewrite() {
        // No `\d`-style pair anywhere: nothing this rewrite can change, so
        // the prescan returns None and the caller keeps the pattern as-is.
        for pattern in ["abc", "[a-z]+", "^(?:foo|bar)$", "[[:digit:]]x", "a.*b"] {
            assert_eq!(to_ascii_perl_classes(pattern), None, "{pattern}");
        }
    }

    #[test]
    fn perl_syntax_still_takes_the_rewrite_path() {
        assert!(to_ascii_perl_classes(r"\d+").is_some());
        // An escaped backslash before `d` trips the prescan and keeps the
        // slow path: the parse succeeds and the rewrite is the identity.
        assert!(to_ascii_perl_classes(r"\\d").is_some());
    }

    #[test]
    fn every_string_matcher_regexp_compiles_on_both_case_paths() {
        for pattern in REGEXPS {
            for &case_sensitive in &[true, false] {
                assert!(
                    build(pattern, case_sensitive).is_some(),
                    "StringMatcherTest.java:151-190 regexp '{pattern}' (case_sensitive=\
                     {case_sensitive}) must compile through the token-regex path"
                );
            }
        }
    }

    #[test]
    fn perl_word_class_stays_ascii_like_java() {
        for &case_sensitive in &[true, false] {
            let re = build("\\w+", case_sensitive).unwrap();
            assert!(re.is_match("abc"), "cs={case_sensitive}");
            assert!(re.is_match("a_B1"), "cs={case_sensitive}");
            assert!(
                !re.is_match("à"),
                "cs={case_sensitive}: java.util.regex \\w is [a-zA-Z0-9_] without \
                 UNICODE_CHARACTER_CLASS (StringMatcher.java:99), so accented letters are \
                 outside the class"
            );
            assert!(!re.is_match("ç"), "cs={case_sensitive}");
            assert!(!re.is_match("é"), "cs={case_sensitive}");
            assert!(!re.is_match("·"), "cs={case_sensitive}");
            assert!(!re.is_match(""), "cs={case_sensitive}");
            assert!(!re.is_match("a-b"), "cs={case_sensitive}");
        }
    }

    #[test]
    fn digit_and_space_classes_stay_ascii_like_java() {
        for &case_sensitive in &[true, false] {
            let digits = build("\\d+", case_sensitive).unwrap();
            assert!(digits.is_match("12"));
            assert!(!digits.is_match("a"));
            assert!(!digits.is_match("à"));
            let optional_digit = build("\\d?", case_sensitive).unwrap();
            assert!(optional_digit.is_match(""));
            assert!(optional_digit.is_match("1"));
            assert!(!optional_digit.is_match("a"));
            let class = build("[\\d ,\\.]+", case_sensitive).unwrap();
            assert!(class.is_match("1 ,."));
            assert!(!class.is_match("1a"));
            let negated = build("[^\\d]+", case_sensitive).unwrap();
            assert!(negated.is_match("à"));
            assert!(!negated.is_match("1"));
            assert!(!negated.is_match(""));
            let signed = build("−?[\\d .,]+%?", case_sensitive).unwrap();
            assert!(signed.is_match("−1.2%"));
            assert!(!signed.is_match("x"));
        }
    }

    #[test]
    fn unicode_property_classes_match_letters_and_punctuation() {
        for &case_sensitive in &[true, false] {
            let letter = build("\\p{L}", case_sensitive).unwrap();
            assert!(letter.is_match("a"));
            assert!(letter.is_match("à"));
            assert!(!letter.is_match("1"));
            assert!(!letter.is_match("_"));
            let lower = build("\\p{Ll}.+", case_sensitive).unwrap();
            assert!(lower.is_match("àb"));
            if case_sensitive {
                assert!(!lower.is_match("Àb"));
            } else {
                assert!(
                    lower.is_match("Àb"),
                    "Java CASE_INSENSITIVE|UNICODE_CASE folds À into \\p{{Ll}}"
                );
            }
            let punct = build("\\p{P}\\p{P}+", case_sensitive).unwrap();
            assert!(punct.is_match("--"));
            assert!(punct.is_match("·-"));
            assert!(!punct.is_match("-a"));
            let number = build("\\p{N}+", case_sensitive).unwrap();
            assert!(number.is_match("11"));
            assert!(!number.is_match("a"));
        }
    }

    #[test]
    fn inline_case_sensitive_island_scopes_to_the_rest_of_the_group() {
        let case_sensitive_re = build("(?-i)Han", true).unwrap();
        assert!(case_sensitive_re.is_match("Han"));
        assert!(!case_sensitive_re.is_match("han"));
        let ci_re = build("(?-i)Han", false).unwrap();
        assert!(ci_re.is_match("Han"));
        assert!(
            !ci_re.is_match("han"),
            "(?-i) inside a case-insensitive build must stay case-sensitive in both engines"
        );
        assert!(!ci_re.is_match("HAN"));
        let island = build("hagen-v|(?-i)Hagen|Hamada", false).unwrap();
        assert!(island.is_match("hagen-v"));
        assert!(island.is_match("HAGEN-V"));
        assert!(island.is_match("Hagen"));
        assert!(!island.is_match("hagen"));
        assert!(island.is_match("Hamada"));
        assert!(
            !island.is_match("hamada"),
            "the ca entities.ent alternations put (?-i) islands mid-alternation; from the flag \
             to the end of the enclosing group matching is case-sensitive in Java and here"
        );
    }

    #[test]
    fn empty_alternatives_and_repetition_bounds_behave_like_java() {
        for &case_sensitive in &[true, false] {
            let empty_tail = build("a|", case_sensitive).unwrap();
            assert!(empty_tail.is_match(""));
            assert!(empty_tail.is_match("a"));
            assert!(!empty_tail.is_match("b"));
            let empty_head = build("|a", case_sensitive).unwrap();
            assert!(empty_head.is_match(""));
            assert!(empty_head.is_match("a"));
            assert!(!empty_head.is_match("b"));
            let star = build("x*", case_sensitive).unwrap();
            assert!(star.is_match(""));
            assert!(star.is_match("xx"));
            assert!(!star.is_match("y"));
            let plus = build("x+", case_sensitive).unwrap();
            assert!(!plus.is_match(""));
            assert!(plus.is_match("x"));
            let opt = build("aa?", case_sensitive).unwrap();
            assert!(opt.is_match("a"));
            assert!(opt.is_match("aa"));
            assert!(!opt.is_match("ab"));
            let bounded = build("a{2,3}b", case_sensitive).unwrap();
            assert!(bounded.is_match("aab"));
            assert!(bounded.is_match("aaab"));
            assert!(!bounded.is_match("ab"));
            assert!(!bounded.is_match("aaaab"));
            let mixed = build("[a-c]?[0-9]*x", case_sensitive).unwrap();
            assert!(mixed.is_match("x"));
            assert!(mixed.is_match("b12x"));
            assert!(!mixed.is_match("d1x"));
            let digits = build("[1234567890]{1,2}", case_sensitive).unwrap();
            assert!(digits.is_match("12"));
            assert!(!digits.is_match("123"));
            assert!(!digits.is_match(""));
            let span = build("[1234567890][1234567890., ]+[1234567890]", case_sensitive).unwrap();
            assert!(span.is_match("12 3"));
            assert!(span.is_match("123"));
            assert!(!span.is_match("1a3"));
            let greedy = build(".+[aei]sses", case_sensitive).unwrap();
            assert!(greedy.is_match("xasses"));
            assert!(!greedy.is_match("xosses"));
            let lazy_any = build("a.*b", case_sensitive).unwrap();
            assert!(lazy_any.is_match("axxb"));
            assert!(lazy_any.is_match("ab"));
            assert!(!lazy_any.is_match("ba"));
            let strict_any = build("a.+b", case_sensitive).unwrap();
            assert!(!strict_any.is_match("ab"));
            assert!(strict_any.is_match("axb"));
        }
    }

    #[test]
    fn literal_and_escape_forms_behave_like_java() {
        for &case_sensitive in &[true, false] {
            let punct = build("[_~°]+", case_sensitive).unwrap();
            assert!(punct.is_match("_~°"));
            assert!(!punct.is_match("a"));
            let octal = build("0o[0-7]+", case_sensitive).unwrap();
            assert!(octal.is_match("0o17"));
            assert!(!octal.is_match("0o8"));
            let hex = build("0x[0-9A-Fa-f]+", case_sensitive).unwrap();
            assert!(hex.is_match("0xaF"));
            assert!(!hex.is_match("0xg"));
            let upper_digit = build("[A-Z]+\\d+", case_sensitive).unwrap();
            assert!(upper_digit.is_match("AB1"));
            assert!(!upper_digit.is_match("Aa"));
            let hash = build("#.+", case_sensitive).unwrap();
            assert!(hash.is_match("#a"));
            assert!(!hash.is_match("#"));
            let at = build("@.+", case_sensitive).unwrap();
            assert!(at.is_match("@b"));
            let ends_e = build(".+e", case_sensitive).unwrap();
            assert!(ends_e.is_match("ae"));
            assert!(!ends_e.is_match("ea"));
            let escape_alt = build("\\.|\\-", case_sensitive).unwrap();
            assert!(escape_alt.is_match("."));
            assert!(escape_alt.is_match("-"));
            assert!(!escape_alt.is_match("a"));
            let quote_dash = build("['-].+", case_sensitive).unwrap();
            assert!(quote_dash.is_match("'a"));
            assert!(quote_dash.is_match("-x"));
            assert!(quote_dash.is_match("'-"));
            let neg_quote_dash = build("[^'-].+", case_sensitive).unwrap();
            assert!(neg_quote_dash.is_match("ab"));
            assert!(!neg_quote_dash.is_match("'a"));
            assert!(!neg_quote_dash.is_match("-"));
        }
    }

    #[test]
    fn case_modes_agree_with_java_case_folding() {
        let lower_cs = build("[a-z]+", true).unwrap();
        assert!(lower_cs.is_match("abc"));
        assert!(!lower_cs.is_match("ABC"));
        let lower_ci = build("[a-z]+", false).unwrap();
        assert!(lower_ci.is_match("ABC"));
        assert!(lower_ci.is_match("abc"));
        assert!(
            !lower_ci.is_match("àb"),
            "Java CASE_INSENSITIVE|UNICODE_CASE folds only case pairs, so à stays outside [a-z]"
        );
        let neg_lower_cs = build("[^a-z]+", true).unwrap();
        assert!(neg_lower_cs.is_match("AB"));
        assert!(neg_lower_cs.is_match("à"));
        assert!(!neg_lower_cs.is_match("ab"));
        let neg_lower_ci = build("[^a-z]+", false).unwrap();
        assert!(!neg_lower_ci.is_match("AB"));
        assert!(neg_lower_ci.is_match("à"));
        assert!(!neg_lower_ci.is_match("ab"));
        let alt_cs = build("[a-z]+|,", true).unwrap();
        assert!(!alt_cs.is_match("A"));
        let alt_ci = build("[a-z]+|,", false).unwrap();
        assert!(alt_ci.is_match("A"));
        let upper_cs = build("\\p{Lu}.*", true).unwrap();
        assert!(upper_cs.is_match("Àbc"));
        assert!(!upper_cs.is_match("abc"));
        let upper_ci = build("\\p{Lu}.*", false).unwrap();
        assert!(
            upper_ci.is_match("abc"),
            "Java CASE_INSENSITIVE applies to case-aware classes like \\p{{Lu}}"
        );
    }

    #[test]
    fn pt_grammar_letter_prefix_regexp_matches_like_java() {
        for &case_sensitive in &[true, false] {
            let re = build("\\p{L}..*", case_sensitive).unwrap();
            assert!(re.is_match("àbc"));
            assert!(re.is_match("ax"));
            assert!(!re.is_match("a"));
            assert!(!re.is_match("1a"));
        }
    }
}
