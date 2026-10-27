# pt style.xml example suite — per-family disposition (round 26, lang-pt round 2)

Generated 2026-10-06 on the tree of this round's pt module changes (committed as the round-26
pt module commit; promoted from `crates/og-langs/src/pt/style-suite-dispositions-r26.md`).
Companion of `docs/attributions/pt-grammar-suite-r25.md` (grammar.xml residue), which it does
not supersede. Every number below was produced this round by the commands in *Reproduction*.

## Headline

| configuration | suite result | failures |
|---|---|---|
| style.xml, main examples (baseline, commit 34f9ec9 tree) | 2032/2162 (94.0%) | 130 (no_match 103 / fp 20 / marker 7) |
| + antipattern examples evaluated at Java harness parity (this round) | 3371/3639 (92.6%) | 268 = 130 main + 138 antipattern |
| + `regexp="no"`/`postag_regexp="no"` shared-xml bug simulated fixed (in-module clone, see E1) | main 2081/2162 (96.3%); antipatterns 1389/1477 | 81 main + 87 antipattern |

Suite command: `cargo test -p og-langs --release --lib
pt::example_suite::portuguese_style_examples_with_pt_pipeline -- --ignored --nocapture`
(run this round: `pt module pipeline [style.xml + antipatterns]: 3371/3639 passed (92.6%) -
antipattern examples: 1477 evaluated, 138 failed, 0 target-missed, 0 skipped [22.1s]`).

### Harness parity gained this round

`pt/example_suite.rs` now evaluates the `<example>` elements inside `<antipattern>` elements,
mirroring Java's `prepareRule` merge (PatternRuleHandler.java:858-866, asserted zero-match by
LanguageSpecificTest.java:384-390) — the same machinery nl gained in commit d60eb21. pt
differs from nl in two ways the port handles: style.xml quotes attributes with `'` as well as
`"`, and 424 of 770 rulegroup sub-rules carry their own ids, so keys resolve as
`(own_id, Some(pos))` / `(group_id, Some(pos))` matching the compiler's `group_sub_id`
assignment (og-xml parser.rs:167, 899-901). Extraction is pinned always-on
(`style_antipattern_extraction_counts_match_an_independent_parse`): 910 rule-level + 273
group-level examples = 1183 raw, expanding to 1477 evaluations because group-level examples
attach to every sub-rule key exactly like Java clears its lists only at `</rulegroup>`
(PatternRuleHandler.java:694). Independent counts via python `re` over comment-stripped XML.

## Root causes (evidence first-hand this round)

### E1. Shared og-xml bug: `regexp="no"` / `postag_regexp="no"` compile the literal string "no" as a regex

The parser stores the attribute verbatim (`exc.regexp = Some("no")`, og-xml parser.rs:294/340),
and the compiler then uses it as the regex source: `postag_source = … else {
token.postag_regexp.clone() }` (og-xml compiler.rs:347-354) and
`exception_regex_source = … else { exc.regexp.as_deref() }` (compiler.rs:435-439), producing
`^(?:no)$`. Two visible effects:

- **tokens**: `reading_pos_matches` prefers the compiled regex over the exact postag
  (og-xml unifier.rs:82-85), so every element written `postag='X' postag_regexp='no'` is
  unmatchable by any real tag — while its readings satisfy the written postag. Trace
  evidence: `QUANDO_TER_DE… stall elem#3 tok="lidar": postag="VMN0000" got=[lidar:VMN0000]`
  on `<token postag='VMN0000' postag_regexp='no'/>` (style.xml:16333).
- **exceptions**: `exception_text_matches` prefers `compiled_regexp` over `text`
  (og-rules pattern_rule.rs:2137-2140), so `regexp='no'` exceptions never veto. Trace
  evidence: USAR_UTILIZAR matches 'USA' despite `<exception case_sensitive='yes'
  regexp='no'>USA</exception>`; SER_CONSISTIR_RESIDIR matches 'é' whose previous token is
  'como' despite `<exception scope='previous' regexp='no'>como</exception>`.

In Java these attributes are booleans: `regexp="no"`/`postag_regexp="no"` mean plain string
(StringMatcher.java, PatternToken.java:796-812 — `PosToken(posTag, regExp, …)` only compiles a
regex when regExp is true). Token-level `regexp='no'` is masked in our engine because
`element_surface_text_matches` checks `pattern.text` first, which is why the bug surfaces
through postag checks and exceptions.

**Measured impact** (`trace_round2_no_attr_fix_impact_on_style_suite`, #[ignore]d probe that
clones the compiled rules, nulls every `^(?:no)$` regex and re-evaluates the suite): style.xml
carries 148 such tokens; the simulated fix moves main examples 2032→2081 (+49, zero
regressions) and antipatterns 1339→1389 (+50). Fixed main families (from the artifact diff):
SIMPLIFICAR_QUE_PRON_V_ANDAR_A_INFINITIVO(10), SENDO_PARTPASS_PARTPASS(4), WORDINESS(3),
SIMPLIFICAR_POIS_VERBO_AO_VERBOINF(3), PERDER_DEMORAR_LEVAR_ESTAR(3), ERRO_TRADUÇÃO(2),
QUE_É_PARTPASS_PARTPASS(2), ESTAR_A_VERB-INFINITIVE(2), CIENTÍFICO_TER_NOME_DESIGNAÇÃO(2),
REMOVER_EU_NÓS(2), QUANDO_TER_DE_QUE_VINF_AO_VINF(2), PASSAR_A_VERBO(2),
PHD_TESE_PODE_SER_PART_PASS(2), PESO_MOLECULAR, ATURAR_SUPORTAR_TOLERAR, COMO_É_CONFORME,
ATRAVES_DE_POR_VIA, SER_CONSISTIR_RESIDIR, MAGOADO_RESSENTIDO,
CIENTÍFICO_TERMOS_TÉCNICOS_TERMINOLOGIA, USAR_UTILIZAR, SIMPLIFICAR_IR_E_PARA(1 each),
VIR_A_VERBO_VERBO(marker, 1). **Fix site (shared owner): og-xml compiler.rs — treat
Some("no") as None for both `regexp` and `postag_regexp` (token + exception paths).**
grammar.xml carries 114 more occurrences (pt), so the shared fix is expected to move the
grammar suite too — re-measure both suites after it lands (the r26 numbers above then apply).

### D1. Dict: contractions carry plus-joined truncated chained tags/lemmas (37 main failures)

Java pins the modern dict forms: `das/[de:o]SPS00:DA0FP0`, `ao/[a:o]SPS00:DA0MS0`
(PortugueseTaggerTest.java:87-89) — colon-joined complete chains, and the chained **lemma**
notation `de:o` that style.xml regexes match against (`…|de:o|em:o` in
QUE_SER_ESTAR_PARTPASSADO_V2's antipattern, style.xml:~14700). The pinned 2016-era dict
gives `das → de+as/SPS00+*`, `deles → de+eles/SPS00+PP3MP00`, `nesta → em+esta/SPS00+*`,
`disso → de+isso/SPS00+PD0NN00` (pinned this round in
`style_residue_contraction_tags_are_plus_joined_truncated_chains`). Postag matching is
fullmatch in both engines (PatternToken.java:354-361 → StringMatcher.matches();
our anchored `reading_pos_matches`), so `SPS00:DA.+`-style elements and exceptions can never
fire on the truncated readings. Families (count, stall/exception evidence):

| family | n | evidence |
|---|---|---|
| CIENTÍFICO_TERMOS_TÉCNICOS_TERMINOLOGIA | 10 | elem#0 'dos' `DA.+|SPS00:DA.+` got=[de+os:SPS00+*]; elem#3-4 'no'/'do' `SPS00:D[ADI].+` |
| DIFERENTES | 4 | elem#5 'dos' `SPS00|SPS00:DA.+` got=[de+os:SPS00+*] |
| VERBO_PRON_PESS_REM_PRON_PESS | 3 | elem#3 'da'/'das' `SPS00:DA.+` |
| QUE_VEM_PROXIMA | 2 | elem#0 'da'/'do' `SPS00:DA.+` |
| PROXIMO_ZERO_PRATICAMENTE_NULO | 2 | elem#1 'dela' `(SPS00:)?[DP].+` got=[de+ela:SPS00+PP3FS00]; 'da' SPS00+* |
| QUE_VERBOPASSADO_VERBOPARTPASSADO | 2 | elem#1 'na' `[DP][ADIPR].+` got=[em+a:SPS00+DA] |
| INIMIGO_ADVERSÁRIO_ALIADO_OPONENTE (fp) | 2 | scope=next exception `SPS00:PP3.+` on 'deles' (SPS00+PP3MP00) never fires |
| QUE_VERBO_SER_A_AS_O_OS (fp) | 2 | elem#0 scope=previous exception `DD.+|SPS00:DD.+` on 'desse' never fires |
| SIMPLIFICAR_QUE_E_TEM_TÊM (fp) | 1 | elem#4 exception `SPS00:DA.+` on 'na' never fires |
| BÁSICO_ELEMENTAR (fp) | 1 | exception regexp `a:o` matches Java's chained lemma of 'ao'; ours is 'a+o' |
| MAIORES_PRINCIPAIS | 1 | elem#0 'dos' `(SPS00:)?D[AI].+` |
| CAUSAR_DIFICULDADE_DIFICULTAR | 1 | elem#2 'na' |
| LENTO_MOROSO | 1 | elem#1 'na' |
| QUE_SE_ENCONTRAR_NA_NO_NESTA_NESTE | 1 | elem#4 'nesta' `SPS00:D[AI].+` |
| ERRO_TRADUÇÃO_COBRIR_ABRANGER | 1 | elem#4 'pelo' `SPS00:DA.+` got=[por+o:SPS00+DA] |
| SIMPLIFICAR_IR_E_PARA | 1 | elem#1 'ao' `SPS00:[DP][ADIP].+` got=[a+o:SPS00+*] |
| ATRAVES_DE_POR_VIA | 1 | elem#0 'disso' `(SPS00:)?P[DP].+` got=[de+isso:SPS00+PD0NN00] |
| QUE_VERBOPRESENTE_CONTINUAMENTE… | 1 | 'no' SPS00:D[ADI] class (same walk as CIENTÍFICO_TERMOS) |

### D2. Dict: enclitic/hyphen compounds untagged → tokenizer splits them (17 main failures)

`PortugueseWordTokenizer.java:161-171` keeps a hyphen word whole iff the tagger tags it; the
2016 dict lacks the entries Java's v0.14 has (`distribuí-lo-ei/[distribuir]VMIF1S0:PP3MSA00`,
PortugueseTaggerTest.java:118; `jiu-jitsu/[jiu-jitsu]NCMS000`, :92-95). Our tokenizer mirrors
the gate, so these split (pinned in
`style_residue_enclitic_and_hyphen_words_are_untagged_and_split`):

- no_match (pattern sees '-'/'se' pieces): A_VOLTA_DE_EM_TORNO_DE ('centra-se'),
  BASEIA-SE_EM ('baseia-se' + V.+:PP3CNO00), CIENTÍFICO_EMPREGAR_TERMO (2: 'usa-se',
  'utiliza-se' — Java matches because the whole token is the skip=1 hop), PHD_TESE_PARA_AJUDAR
  (2: 'ajudá-la', 'ajudar-nos'), CLICHE_18_LEVANTAR, CLICHE_240_ARMAR-SE, CLICHE_433_SER
  ('troca-tintas'), CLICHE_434_SER, CLICHE_449_TRAIR-SE (+ chained V.+:PP.+),
  DE_PROPOSITO_DELIBERADAMENTE ('fizeram-no'), AVOID_ALL_PASSIVE_VOICE ('fez-se'),
  SIMPLIFICAR_DIA_DE_HOJE ('segunda-feira'), VIR_A_VERBO_VERBO ('conhecê-la').
- marker (match shrinks to the verb stem): PASSAR_A_VERBO (2) and ESTAR_A_VERB-INFINITIVE (2)
  were fixed by E1 (the optional clitic element was postag_regexp='no'); ATURAR_SUPORTAR
  ('aturam-me' → match 13..19 vs marker 13..22) was fixed by E1 the same way.
- fp via scope=previous/next exceptions that need the whole token: WORDINESS 'Reuniu-se grande
  número' (previous = 'Reuniu-se' V+PP chain in Java → `V.+` exception vetoes; ours previous is
  'se' PP → no veto) and 'Fiz um esforço sobre-humano' (scope=next `N.+|AQ.+` on the whole
  'sobre-humano' vs our '-' piece).

### D3. Dict: missing vocabulary / stale readings (19 main failures; probe_dict first-hand)

No entry at all (`got=[:]` stalls, pinned in
`style_residue_missing_vocabulary_and_stale_readings`): CONFORME_IA_IAM (2, 'revendo' needs
VMG0000), PHD_TESE_REFERIREMOS (2, 'refiro' needs VMI[FP]1.+), POR_ANO_MÊS…V2 (4, 'downloads'
needs NC.P.+), SER_CAPAZ ('resolver' VMN.+), E_NÃO_VERB ('resolvem' VMIP3.+), QUE_ESTA_ESTAO_EM
('host'), QUE_SE_V_PRESENTE ('ambientes' has only verb readings, needs NC), ARCHAISMS
('tão-pouco'; also splits — see Differential). Stale/wrong readings: REMOVER_EU_NÓS (2,
'usamos' lacks the VMIS1P0 homograph), PRESO_RETIDO_V2 ('preso' lacks prender/VMP00SM),
MORRER_PERECER_FALECEN (2, 'mortos' participle lemma is *matar*, not *morrer*),
SER_SIGNIFICATIVO (2, 'acrescentar' lacks VMN01S0/VMN03S0 personal-infinitive readings for
`VMN0.S.+`).

### E2. Engine: backreference/unify evaluation (4 main failures)

ELES_ELAS (4): stalls `backref-no=Some(0/1) mismatch` on 'amigos'/'jogadoras' — the rule
unifies the pair via \1 backreferences; our backreference evaluation diverges (fix site:
og-rules backreference path, shared owner). The 2 examples that die at elem#0
(`(?:P.|N).FP.+` never matching despite amigo/NCFP000) suggest the same walk never gets
started on the intended alignment.

### Differential — needs a JVM trace or modern-dict knowledge (4 main failures)

- O_QUE_ESTAR_SER_PARTICIPIOPASSADO ('Para a polícia agir como o que foi definido.'): no
  element walk reproduces Java's intended alignment under static reading (elem#2
  `SPS00|NC.+|AQ.+` cannot match 'como' in either engine).
- O_QUE_TEM_DE_MAL_QUE_MAL_TEM: 'a' carries only a:SPS00 after disambiguation here; the
  DA0FS0 article reading that `D[AI].+` needs is present in other contexts (line-level
  disambiguation divergence or dict).
- SIMPLIFICAR_QUE_E_TEM_TÊM ' e tem' ('Meu marido está na legião, e tem chá pronto.'): no
  Java-side suppression path (antipatterns/exceptions) found by static reading; the sibling
  fp ('que tem lugar na mente') is D1.
- ARCHAISMS ('Não tem férias, nem tão-pouco precisa delas.'): 'tão-pouco' is both dict-missing
  and hyphen-split in our pipeline; whether Java tags it whole (making the `(?:&arcaismos;)s?`
  single-token regexp match) is unknowable without the modern dict.

## Antipattern-example residue (138 baseline failures; 87 after the E1 simulation)

E1 explains 51 (measured, see E1). Of the 87 remaining, the antipattern bodies carry the same
dict-dependent constructs (verified by body scan + spot reads): chained-postag antipatterns
(`SPS00:D[AI].+` etc. — QUE_SER_ESTAR_PARTPASSADO_V2(25), QUE_VERBO_SER(13),
QUE_ESTA_ESTAO_EM(7), SENDO_PARTPASS(6), COLOCACAO_ADVERBIOS_LUGAR(5), VERBO_ESTAR_A_VERBO_INF,
VERB_QUE_É_VERB_SER, SER_NECESSARIO, SIMPLIFICAR_QUE_E_TEM, SIMPLIFICAR_IR_E_PARA),
chained-lemma literals (`de:o|em:o|a:o` regexes), hyphen-literal tokens
(PEDIR_SOLICITAR_V3's `<token>pedi-las</token>`), and POSTAG_NO antipattern elements (covered
by E1 once the shared fix lands). OS_DOIS_AS_DUAS (9) additionally depends on
`postag='PUNCT_INTERROGATION'` — an authoring typo for the `_PUNCT_INTERROGATION` tag the pt
disambiguation itself sets (disambiguation.xml:2496); that element is dead in both engines, so
Java's pass must come from the main pattern's readings — treat as Differential. FAZER_EXERCER
(1) shows none of the constructs — Differential.

## Integrator wiring (outside this round's boundary)

1. **Promote this file** to `docs/attributions/pt-style-suite-r26.md`.
2. **Shared-owner fix E1** (og-xml compiler.rs, treat `="no"` as plain string for
   `regexp`/`postag_regexp` on tokens and exceptions), then re-run both pt suites and refresh
   this doc's headline. 148 dead tokens in style.xml, 114 in grammar.xml (pt).
3. **Refresh `docs/attributions/pt-grammar-suite-r25.md` headline numbers**, stale since the
   13fd76b/34f9ec9 disambiguator wiring: grammar.xml is 5282/5492 (96.2%), no_match 181 /
   fp 23 / marker 6 (210 failures; measured this round by
   `cargo test -p og-langs --release --lib pt::example_suite::portuguese_grammar_examples_with_pt_pipeline -- --ignored --nocapture`),
   artifact `/tmp/test_failures_pt_module.txt` (210 lines). The doc's table still says
   5222/5492 and "270 module failures".
4. Cross-boundary reminder from the gap, unchanged: `ShortenedYearRangeChecker`
   (grammar.xml:36866, DATE_RANGES_2) is unwired in the shared dispatch and falls through the
   pass-through arm at og-rules/pattern_rule.rs:2904 — shared owner.
5. Artifacts this round: `/tmp/test_failures_pt_style.txt` (268 lines: main + antipattern),
   `/tmp/test_failures_pt_module.txt` (210, grammar, unchanged),
   `/tmp/test_failures_pt_style_simfix.txt` (81) and
   `/tmp/test_failures_pt_style_simfix_antipatterns.txt` (87) from the E1 simulation.

## Reproduction

```
cargo test -p og-langs --release --lib pt::example_suite -- --ignored --nocapture   # both suites + counts
cargo test -p og-langs --release --lib antipattern_tests -- --nocapture            # extraction pins
cargo test -p og-langs --release --lib pt::trace::round2_pins                       # dict-era pins (always-on)
cargo test -p og-langs --release --lib pt::trace::round2_tests -- --ignored --nocapture      # per-family stall trace
cargo test -p og-langs --release --lib pt::trace::round2_fix_impact -- --ignored --nocapture # E1 simulated-fix measurement
cargo test -p og-langs --release --lib pt::trace::round2_dict -- --ignored --nocapture       # dict word probes
```
