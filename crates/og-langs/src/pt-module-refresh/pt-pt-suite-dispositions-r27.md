# pt-PT/grammar.xml example suite — entity-resolution closure and dispositions (round 27, lang-pt round 3)

Generated 2026-10-28 on this round's uncommitted tree. Companion of
`docs/attributions/pt-grammar-suite-r25.md` (grammar.xml) and
`docs/attributions/pt-style-suite-r26.md` (style.xml); family ids D1/D2/D3/E1/E2/Differential
are defined there and reused unchanged. Every number below was produced this round by the
commands in *Reproduction*.

## Headline

| configuration | suite result | failures |
|---|---|---|
| pre-round baseline (nested variant file, entities literal) | 398/482 (82.6%) main examples | 84 (no_match 64 / fp 20) |
| + `RuleEntityResolver` semantics ported in-module (`entity_resolution.rs`), main examples | **464/482 (96.3%)** | 18 (no_match 11 / fp 7) |
| + antipattern examples evaluated at Java harness parity (593 raw → 3069 expanded) | main 464/482 + antipatterns 2860/3069 | **3324/3551 (93.6%) = 18 + 209** |

All 227 failures are pinned accepted-excluded by rule in
`pt_pt_suite.rs` (`ACCEPTED_EXCLUDED_MAIN`/`ACCEPTED_EXCLUDED_ANTIPATTERN`, asserted by the
suite test) and attributed to the families below with first-hand evidence.

Suite command (this round, all numbers above):

```
cargo test -p og-langs --release --lib pt_module_refresh::pt_pt_suite -- --ignored --nocapture
```

## Root cause closed this round: nested variant rule files got no external-entity expansion

Java routes every `*.ent` systemId through `RuleEntityResolver` (PatternRuleHandler.java:43 →
RuleEntityResolver.java:25-40), which strips everything through the last `/resource/` and every
`../`, then loads from the language resource folder — regardless of the rule file's own
directory. The shared Rust loader (`og-xml` parser.rs `include_external_entities`) joins the
systemId onto the base dir instead: from `rules/pt/pt-PT/`, `../../resource/pt/entities/x.ent`
resolves to the non-existent `rules/resource/pt/entities/x.ent` and is silently skipped, and
`engine.rs portuguese_variant` compiles with no base at all (`compile_file`). 60 of 148 main
patterns in pt-PT/grammar.xml reference entities; the resulting no_match families
(CONTRAÇÃO_DO_DE_O_INFINITIVO_PT_PT 11, AO90_COMPOUNDS_GENERIC_PREFIX 15 of 16,
ARTICLES_PRECEDING_LOCATIONS 8, CONTRAÇÃO_EM_UM_NUM_PT_PT 6, AO90_HAVER_DE 4,
PARONYM_PREMIO_PRÉMIO_PT 4, AO90_ACCENTUATED_PREFIXES 3, …) collapse once the entities
resolve: **66 of the 84 baseline failures are this one divergence** (pinned by
`entity_expansion_restores_the_previously_divergent_families`: all 14 fully-recovered
families now behave).

`entity_resolution.rs` ports the Java resolver semantics inside this module (test-only until
the shared loader gains them): normalization `path_from_lt_resource_folder` is asserted
against RuleEntityResolver.java:31-34, all 10 declared `.ent` files of pt-PT/grammar.xml
expand, and the expanded doctype declares no duplicate entity names (Java SAX keeps the first
declaration, the shared expander the last — a divergence that would matter if duplicates
existed; none do).

## Remaining residue (227 failures, all attributed)

### E1 — shared og-xml `regexp="no"`/`postag_regexp="no"` compiled as the regex `no` (191 carry it)

Defined and evidenced in `docs/attributions/pt-style-suite-r26.md` (fix site: og-xml
compiler.rs `postag_source` fallback, treat `="no"` as plain string). pt-PT/grammar.xml
carries 52 `postag_regexp='no'` occurrences; they kill antipattern vetoes wholesale:

- VERBO_HIFENIZADOR_VERBOS_2 antipattern failures 168/168: every failing example's
  antipattern body carries `postag_regexp='no'` (88 pure-E1) and/or chained-postag
  alternatives (D1, 20; both, 60).
- POSSESSIVE_WITHOUT_ARTICLE antipattern failures 32/34 and main fp 6/7 (vocative
  antipatterns [30]/[31]/[37]/[46]/[47] die on `postag='CC'|'I'|'RG' postag_regexp='no'`
  elements; the `aquele seu` fp needs [42] whose first element is
  `postag='SPS00' postag_regexp='no'`).
- VERBO_PRONOME_PESSOAL_VERB 5/5: all three of its own antipatterns open with
  `postag_regexp='no'` elements (`SENT_START`, `VMIP1S0`, `VMIP3S0`); probe on
  'Me sou responsável pelo relatório final.' shows the rule matching 'Me sou' (0..6) with no
  antipattern firing. Pinned structurally by
  `pt_pt_antipatterns_carry_the_shared_postag_no_bug`.

### D1 — dict contractions plus-joined/truncated, colon-chained postags never fire (88 carry it)

Java pins the colon-chained dict forms (`das/[de:o]SPS00:DA0FP0`,
PortugueseTaggerTest.java:87-89); the pinned 2016-era dict truncates to `SPS00+*` /
`SPS00+DA`. Probe evidence (suite `pt_pt_residue_dict_evidence` pins these lookups):

- 'Um dia desses te levarei ao zoológico.' fails subrules /2, /3, /4: the vetoing antipattern
  opens with `postag='NP.+|NC.+|AQ.+|PP.+|RM|PD.+|SPS00:PD.+|CC'`; our 'desses' carries only
  `[VMM02S0, VMSP2S0, SPS00+*, VMSI2S0]` — no reading can satisfy it, while Java's
  `SPS00:PD0MP00` does.
- AO90_COMPOUNDS_GENERIC_PREFIX/15 antipattern failure ('…na bio do @jornalexpresso.'): the
  rule matches 'bio do' with 'do' = `SPS00+*`.
- 'daí' carries only `SPS00+RG` (lemma `de+aí`).

### D2 — dict enclitic/hyphen compounds untagged → tokenizer splits them (6 main failures)

Probe token dumps: 'lava-se' → `lava | - | se` (REFLEXIVO_CONJUNTIVO 2), 'circum-escrito'
split (AO90_COMPOUNDS_GENERIC_PREFIX main 1), 'Segundas-feiras' split
(AO90_WEEKDAYS_CASING_PT_PT 1, AO45_WEEKDAYS_CASING 1), 'Bem-feito' split (AO90_BEM_FEITO 1).
Same family as style.xml D2 (PortugueseWordTokenizer.java:161-171 gate).

### D3 — dict missing vocabulary / stale readings (7)

- BRASILEIRISMO_REVISAR: 'Revisaste' has no dict entry (pinned lookup empty).
- CONTRACAO_INDEVIDA_DAI_DE_AI 2: 'retirar' surfaces only tagless readings through the
  pipeline, stalling the exact `postag='VMN0000'` element (pinned).
- POSSESSIVE_WITHOUT_ARTICLE no_match 2 ('Leve sua irmã no carro.',
  'Leve a cesta de sua irmã.'): the ANTI[8] person-noun veto (`DP.+` +
  `&pronomes_possessivos_excecoes_a_o_pt_pt;` inflected) matches 'sua irmã' through the
  `irmã→irmão` lemma (pinned) and suppresses the would-be rule match; Java's dict lemma must
  differ.
- POSSESSIVE 'Meu pequeno gênio!' antipattern failure: 'gênio' is missing from the dict
  (pinned empty) and the pipeline then strips the AQ reading of 'pequeno' (pinned), killing
  the vocative antipattern's `AQ.+` element.

### Differential — needs a JVM trace or the modern dict (2)

- POSSESSIVE 'Esta mania tão nossa de desdenhar de tudo e de todos.' (fp): no suppressing
  antipattern found by static reading under either engine's tags.
- AO90_WEEKDAYS_CASING_PT_PT/1 'A tríade é simples Ser > Fazer > Ter.' (antipattern fp): the
  rule matches 'Ter' (35..38); its antipattern's second element cannot match the sentence-end
  '.' under either engine by static reading, so Java's pass must come from the rule pattern /
  NP readings of 'Ter'.

### Engine-veto check: none

For every antipattern failure the suite probes whether any of the rule's antipatterns
matches with Java's span-overlap semantics (DisambiguationPatternRuleReplacer.java:84-108);
the count is 0/209 (asserted), so the shared veto machinery is not implicated — everything
routes through E1/D1/D3 dead antipattern bodies or rule-side dict stalls.

## Integrator wiring (outside this round's boundary)

1. **Port `RuleEntityResolver` into the shared loader**: `og-xml`
   `include_external_entities` should normalize `.ent` systemIds to the language resource
   folder (or accept a resolver parameter), and `engine.rs portuguese_variant`
   (`compile_file(&xml)`, no base) should compile with the file's dir as base plus the same
   normalization. `entity_resolution.rs` in this module is the reference implementation;
   once shared, delete it and point the two suites at the shared path. Same geometry affects
   `pt-BR/`, `pt-AO/`, `pt-MZ/` variant dirs and `pt-PT/style.xml` (9 `.ent` decls, 1077
   `<example>` tags — none evaluated this round).
2. **E1 shared fix** (og-xml compiler.rs, `postag_source`/exception paths): pt-PT/grammar.xml
   carries 52 `postag_regexp='no'` occurrences behind 191 of the 227 failures — re-run this
   suite and refresh this doc after it lands.
3. **Promote this file** to `docs/attributions/pt-pt-suite-r27.md`.

## Round 2 verification addendum (2026-10-29, this wave's porter)

All three suites re-run on the refreshed ref (`cargo test -p og-langs --release --lib …`):

- grammar.xml (8,123 tags): `pt::example_suite::portuguese_grammar_examples_with_pt_pipeline
  -- --ignored` → 5297/5506 (96.2%), no_match 180 / fp 23 / marker 6 (209 failures; r26 doc
  baseline was 210 at 8,101 tags).
- pt-PT/grammar.xml (1,088 tags): `pt_module_refresh::pt_pt_suite -- --ignored` → 3324/3551
  (93.6%), failures exactly equal the pinned tables above (18 main + 209 antipattern, 0
  veto-overlap).
- style.xml (3,425 tags): `pt::example_suite::portuguese_style_examples_with_pt_pipeline
  -- --ignored` → 3441/3709 (92.8%), 129 main + 139 antipattern failures (r26 doc: 130 + 138).
- antipattern extraction pins 1477/910/273/29 (`antipattern_tests`): pass.

New pinned evidence (`predelta_diff.rs`, both `--ignored`): staging the pre-delta rule files
from the LT ref git history (grammar `b650fda3881` 8,101 tags, style `1eefa2dc9c0` 3,355) and
evaluating them against the current tree proves the refreshed XML introduces **zero** new
suite failures and resolves exactly one pre-delta stall (grammar
`[PT_MULTITOKEN_SPELLING_TWO] 'Michael Jodan.'`, via the live spelling/multitoken data). The
single style-antipattern movement vs the r26 doc (138→139, the QUE_ESTA_ESTAO_EM example
'…tratamento com Zaarpress HCT…') is **engine-side, not the PT delta**: under pre-delta data
staged from git the rule matches the sentence identically (the only data-side reading change
in it is 'uma' DI0FS0 → DI0FS0+Z0FS0, far outside the match window), and the veto's
`postag='_PUNCT' postag_regexp='no'` element stays the documented E1 fingerprint — the r26
simfix artifact already shows this exact example passing once E1 is fixed.
