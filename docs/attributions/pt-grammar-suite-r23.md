# pt grammar.xml example suite — bucketed attribution (round 23)

Generated on the tree of commit `844c946` (2026-10-03). All numbers below were
produced by the commands listed in *Reproduction* run in this round.

## Headline

- Suite: `cargo test -p og-test-runner --release test_real_portuguese_grammar_examples -- --ignored`
  → `pt: 5012/5492 passed (91.3%) — no_match: 408, fp: 62, marker: 10` (480 failing lines in
  `/tmp/test_failures_pt.txt`).
- r22 final was 565 of ~5,626. This round's three fixes moved pt 565 → 480 and changed the
  denominator 5,626 → 5,492: phrase siblings now share the original rule's subId, so the
  sibling-duplicated example runs collapse (Java re-tests each sibling's identical examples, but
  `getPatternRulesByIdAndSubId` unions all siblings, which makes the per-example outcome
  identical — see `PatternRuleTest.java:584`, `AbstractPatternRuleTest.java:65-72`).
- Per-failure-class split of the 480: 408 no-match, 62 fp ("Expected no match"), 10 marker
  (match does not contain marker).

## Phrase-sibling verification (mandated before any pt matcher work)

`cargo run --release -p og-test-runner --example phrase_sibling_probe -- pt`

12 multi-sibling (id, sub_id) groups, all in the two suffix-confusion rulegroups:

| group | siblings | = alternatives of |
|---|---|---|
| FUTURE_PAST_CONFUSION1..6 | 4 each | ALL_PAST_EXPRESSIONS = includephrases union of YESTERDAYS, PAST_EXPRESSIONS, INVERTED_PAST_EXPRESSIONS, PAST_THAT_HAS_PAST_EXPRESSIONS |
| PAST_FUTURE_CONFUSION1..6 | 5 each | ALL_FUTURE_EXPRESSIONS = union of TOMORROW, IN_THE_FUTURE_EXPRESSION, FROM_NOW_TO_FUTURE_EXPRESSION, COMING_FUTURE_EXPRESSION, NEXT_FUTURE_EXPRESSIONS |

Total compiled rules 2,060 for 2,030 `<rule>` elements (30 net sibling additions). Before
commit `844c946` each sibling was mis-assigned its own positional subId, so FUTURE_PAST_CONFUSION1's
four alternatives were tested as sub-rules 1..4 (each against a single alternative, never the
union) and every later sub-rule of the group shifted by the sibling count — the largest single
pt bucket at r22. This is now Java-shaped.

## no-match bucketing

`cargo run --release -p og-test-runner --example nomatch_trace -- pt` (writes nothing; stdout
captured): 408 no-match failures traced, matched-anyway: 30, untraced: 10.

Reading the buckets:

- matched-anyway (30): a *different* sub_id of the same rule id matches the example in
  isolation. The suite groups by (id, sub_id) exactly like Java, so these are mostly legitimate
  (Java would also fail them); verify individually before counting them as engine bugs.
- untraced (10): no trace events — rule id not found in the compiled set or no failure recorded;
  inspect by hand.
- The 62 fp and 10 marker failures are not bucketed by nomatch_trace (it parses no-match lines
  only).

Top rules by failing examples:

| rule | fails | class |
|---|---|---|
| COLOCACAO_PRONOMINAL_COM_ATRATOR_SIMPLES | 40 | no-match |
| PODERIAM-SE | 16 | no-match |
| COLOCACAO_PRONOMINAL_COM_ATRATOR_COMPLEXO | 15 | no-match |
| DIACRITICS | 14 | fp |
| GENERAL_GENDER_AGREEMENT_ERRORS | 11 | no-match |
| TIME_FORMAT | 10 | no-match |
| GENERAL_GENDER_NUMBER_AGREEMENT_ERRORS | 10 | no-match |
| CONCORDANCIA_DETERMINANTES_MASCULINOS | 10 | no-match |
| PT_MULTITOKEN_SPELLING_TWO / MISPELLED_NAMES / HOURS_ABREVIATION / GENERAL_VERB_AGREEMENT_ERRORS | 8 each | no-match |

Aggregate deepest-failure causes (top of 30 distinct):

- 47 `surface!="^(?:ela|elas|ele|eles|eu|nós|tu|você|vocês|vós)$"` — COLOCACAO_PRONOMINAL family
  (clitic-attachment rules; the biggest single bucket).
- 45 `surface!="^(?:Nath?al[iy]e?)$"` + 16×7 more proper-name regexps (Barbara, François, Glenn,
  Jimmie, Leonardo, Sarah, Steeven, Dominique…) — MISPELLED_NAMES-class rules whose first element
  is a case-sensitive name regexp.
- 30 `surface!="^(?:[012]?\d[,\.][012345]\d)$"` — TIME_FORMAT decimal-minute shapes.
- 24 `surface!="por"`, 21 `inflected-lemma!="ser"`, 12 `inflected-lemma!="^(?:dar|bater)$"`.
- 18 `postag="V.+:PP.+" got=[:]` and 16 `postag="Z.+" got=[:]` — pronominal-passive / numeral
  postags never present on the traced token: readings-parity class, consistent with the known
  pt tagger-dict limitation (fetched dict is an older version missing later words; also absent
  from the last-tracked in-repo version — data-blocked, see commit `370c7e6`).
- 17+11 `postag="D..MP.+"/"N.[CM]P.+" got=[cara:NCFS000; caro:AQ0FP0]` — CONCORDANCIA rules see
  feminine readings where Java's disambiguator would have removed them: pt disambiguation.xml
  ground truth is still unwired for pt (only en/fr/ca/pl/uk/eo harnesses exist).

## Standing data limitations

- pt tagger dictionary is an older version than Java's current one (missing later words such as
  `destacadamente`); re-downloading is out of scope per the round rules.
- pt-PT_synth.dict absent (accepted-excluded).

## Reproduction

```
cargo test -p og-test-runner --release test_real_portuguese_grammar_examples -- --ignored --nocapture
cargo run --release -p og-test-runner --example nomatch_trace -- pt
cargo run --release -p og-test-runner --example phrase_sibling_probe -- pt
cargo run --release -p og-test-runner --example phrase_sibling_probe -- pt CONFUSION   # per-sibling pattern heads
```

Artifacts this round: `/tmp/test_failures_pt.txt` (480 lines), `/tmp/nomatch_trace_pt_r23.txt`.
