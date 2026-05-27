# Progress (updated 2026-05-27)

See `rust-audit.md` for the full detailed audit with compatibility matrix and port lists.

| Phase | Status | % | Notes |
|-------|--------|---|-------|
| 0 — Source audit | STARTED | 40% | Compatibility matrix created in rust-audit.md; port lists partially documented |
| 1 — Rust API shell | DONE | 95% | All 3 endpoints, 10 params functional. `language.name`/`longCode`/`type` fixed |
| 2 — Core text model | DONE | 80% | All 17 types. `default_on` honored. Missing whitespace-before, token type enum |
| 3 — Tokenizer | PARTIAL | 70% | 200+ abbreviations. URL/email detection added. No SRX loading |
| 4 — XML parser | DONE | 85% | `description`/`sub_id`/`<url>`/`<short>`/`corrections`/`type` now parsed |
| 5 — Pattern rule engine | FUNCTIONAL | 80% | `sub_id`/`url`/`short_message`/`issue_type` propagated. Most filters still stubs |
| 6 — Spellchecker | ACTIVE | 50% | Morfologik 259K full-form dict. Length-filtered suggestions. No Hunspell .aff |
| 7 — POS/morphology | PARTIAL | 65% | Tagger traits unified. No statistical tagger |
| 8 — Disambiguation | PARTIAL | 65% | English XML disambig works. Unify is stub |
| 9 — Native rule ports | ACTIVE | 55% | 21 rules ported (4 generic + 17 English). Style freq, regional replace added |
| 10 — Language modules | ENGLISH ONLY | 5% | 1 of 37 languages. No de/fr/es/pt/pl/nl modules |
| 11 — Test suite parity | STUB | 12% | 450 unit tests passing. Heavy tests #[ignore]. No compat harness, no CI |
| 12 — Performance | PARTIAL | 35% | Rayon + first-token index + regex cache + length-filtered suggestions |

**Workspace:** 11 crates, ~58 files, ~20.8K lines of Rust.

---

Understood. Pure plan:

```txt
Goal:
Rewrite LanguageTool compute/API engine in Rust.

No AI.
No product extension plan.
No desktop.
No LibreOffice.
No Java runtime.
No Java wrapper.
No custom simplified clone as the final goal.

Success = Rust engine passes LanguageTool-compatible tests and produces equivalent /v2/check output.
```

LanguageTool’s actual API target is mainly `/v2/check`; their public API docs describe that as the proofreading endpoint. ([dev.languagetool.org][1]) Their development docs also confirm the important internal model: XML `grammar.xml` rules plus Java `Rule.match(AnalyzedSentence)` rules for logic that cannot be expressed in XML. ([dev.languagetool.org][2])

# 1. Rewrite scope

You rewrite this compute path:

```txt
/v2/check request
  ↓
language selection
  ↓
text normalization / markup handling
  ↓
sentence splitting
  ↓
tokenization
  ↓
part-of-speech / morphology analysis
  ↓
disambiguation
  ↓
XML pattern rules
  ↓
native rule classes
  ↓
spellchecking
  ↓
suggestions
  ↓
RuleMatch output
  ↓
LanguageTool-compatible JSON
```

You do **not** rewrite:

```txt
LibreOffice integration
desktop GUI
browser extension
website
Java embedding API
old integrations
non-checking product code
```

LanguageTool’s own repo README describes it as proofreading software for English, Spanish, French, German, Portuguese, Polish, Dutch, and 20+ other languages, so “all languages” means you are committing to a language-module rewrite, not just an English engine. ([GitHub][3])

# 2. Rust workspace layout

```txt
opengrammar-rs/
  crates/
    og-core/
    og-api/
    og-xml/
    og-rules/
    og-spell/
    og-tokenizer/
    og-tagger/
    og-languagemodel/
    og-langs/
    og-test-runner/
    og-compat/
```

## `og-core`

Core shared types:

```txt
Language
Document
Sentence
Token
AnalyzedToken
AnalyzedTokenReadings
AnalyzedSentence
Rule
TextLevelRule
RuleMatch
SuggestedReplacement
Category
IssueType
```

This crate replaces LanguageTool’s core checking abstractions such as `JLanguageTool`, `Rule`, `RuleMatch`, `AnalyzedSentence`, and `AnalyzedTokenReadings`. The current Java `JLanguageTool.java` imports and wires together markup handling, language models, rules, pattern rules, spelling rules, and analyzed text objects, which is the area your Rust core must replace. ([GitHub][4])

## `og-api`

Rust HTTP server.

```txt
POST /v2/check
GET  /v2/languages
GET  /v2/version
```

Keep `/v2/check` response compatible with LanguageTool.

## `og-xml`

LanguageTool XML parser and compiler.

Handles:

```txt
grammar.xml
style.xml
disambiguation.xml
grammar_custom.xml
categories
rulegroups
rules
patterns
tokens
exceptions
antipatterns
suggestions
examples
regexp
case_sensitive
skip
min/max
inflected
postags
```

LanguageTool supports `grammar_custom.xml` alongside `grammar.xml`, using the same XML syntax and non-conflicting rule IDs, so your loader should support that too. ([dev.languagetool.org][5])

## `og-rules`

Native Rust equivalent of Java rule classes.

This replaces Java rules like:

```txt
SpellingCheckRule
TextLevelRule
PatternRule
language-specific Rule subclasses
```

LanguageTool docs explicitly say rules that cannot be expressed in XML are written by extending Java `Rule` and implementing `match(AnalyzedSentence)`, or `TextLevelRule` for rules not working on the sentence level. ([dev.languagetool.org][2])

Your Rust equivalent:

```rust
pub trait Rule: Send + Sync {
    fn id(&self) -> &str;
    fn match_sentence(&self, sentence: &AnalyzedSentence, ctx: &RuleContext) -> Vec<RuleMatch>;
}

pub trait TextLevelRule: Send + Sync {
    fn id(&self) -> &str;
    fn match_text(&self, doc: &AnalyzedDocument, ctx: &RuleContext) -> Vec<RuleMatch>;
}
```

## `og-spell`

Rust replacement for LanguageTool spellchecking.

Responsibilities:

```txt
dictionary loading
ignore words
compound handling
suggestions
word frequency ranking
language-specific spelling rules
```

## `og-tokenizer`

Sentence splitting and tokenization.

Responsibilities:

```txt
sentence boundary detection
word tokenization
punctuation tokenization
URL/email detection
abbreviation handling
offset preservation
```

LanguageTool’s own older technical description is basically: take plain text, split into sentences, split sentences into words, find POS tags/base forms, then match analyzed sentences against patterns and Java rules. ([Daniel Naber][6])

## `og-tagger`

POS tagging and morphology.

Responsibilities:

```txt
lemma
part-of-speech tags
readings
base forms
language-specific analyzers
disambiguation support
```

This is one of the hardest parts for full compatibility.

## `og-languagemodel`

N-gram/language-model compatibility if you want parity with LT features using language models.

Do not add AI. This is for traditional statistical n-gram features only.

## `og-langs`

Language modules.

```txt
og-langs/
  en/
  de/
  fr/
  es/
  pt/
  pl/
  nl/
  ...
```

Each language module contains:

```txt
grammar.xml
style.xml
disambiguation.xml
spell dictionaries
tagger dictionaries
abbreviations
native Rust rule ports
tests
```

## `og-test-runner`

Rust equivalent of LT rule test runner.

LanguageTool docs say grammar rules can be tested via IDE/JUnit, command-line `testrules.sh`/`testrules.bat`, or Maven `mvn clean test`. ([GitHub][7]) Your Rust rewrite needs an equivalent runner.

## `og-compat`

Compatibility tooling:

```txt
Run Java LT and Rust OG on same input
Compare JSON responses
Compare matches
Compare offsets
Compare replacements
Compare rule IDs
Generate diff reports
```

# 3. Migration strategy

Do not manually rewrite random files blindly.

Use a compatibility-driven migration.

```txt
Step 1:
Freeze one LanguageTool upstream commit.

Step 2:
Vendor/copy required resource files:
- grammar.xml
- style.xml
- disambiguation.xml
- dictionaries
- test files
- example sentences

Step 3:
Build Rust engine skeleton.

Step 4:
Make Rust /v2/check return LT-shaped JSON.

Step 5:
Port XML parser.

Step 6:
Port tokenizer/sentence splitter.

Step 7:
Port PatternRule engine.

Step 8:
Port spellcheck.

Step 9:
Port taggers/morphology.

Step 10:
Port Java Rule classes language by language.

Step 11:
Run same test suite until parity.
```

# 4. Test strategy

This is the most important part.

Your real metric is not “does it compile?” It is:

```txt
For the same input,
same language,
same enabled/disabled rules,
same mother tongue,
same picky mode/settings,

Rust output == Java output
```

## Test categories

```txt
1. XML validation tests
2. XML rule example tests
3. Pattern rule tests
4. Sentence tokenizer tests
5. Word tokenizer tests
6. POS tagger tests
7. Disambiguator tests
8. Spellchecker tests
9. Java-rule-port tests
10. Full /v2/check golden tests
11. Cross-language regression tests
12. Performance tests
```

LanguageTool build/test logs show language rule testing includes XML validation for `grammar.xml`, `style.xml`, remote rule filters, pattern rule tests, rule ID uniqueness, and loading thousands of rules for a language. One reported English run loaded 6117 rules. ([GitHub][8])

## Golden output tests

Create fixtures:

```txt
fixtures/
  en/
    input_001.txt
    expected_java.json
    expected_rust.json
  de/
  fr/
  es/
```

Generate expected output by running official Java LanguageTool at the frozen commit.

Then Rust must match.

Compare:

```txt
rule.id
message
shortMessage
offset
length
replacements
category
issueType
context
```

Offsets must be exact.

# 5. XML compatibility target

Because you want a pure rewrite, XML compatibility is not optional. It is central.

Your Rust XML engine must eventually support the LT rule language.

Start order:

```txt
Level 1:
basic <rule>, <pattern>, <token>, <message>, <suggestion>

Level 2:
regexp, case sensitivity, exceptions, antipatterns

Level 3:
rulegroups, examples, categories, default enable/disable

Level 4:
POS constraints, inflected forms, lemmas, skip/min/max

Level 5:
backreferences, markers, suggestion transforms

Level 6:
disambiguation rules

Level 7:
filter hooks / Java RuleFilter equivalents rewritten in Rust
```

Do not invent another rule format.

# 6. Native Java rule porting strategy

Every Java rule becomes one of:

```txt
1. Already expressible in XML → convert to XML if safe
2. Native Rust rule → direct port
3. Language-specific Rust rule → lives in og-langs/<lang>
4. Unsupported initially → tracked in compatibility matrix
```

Create a matrix:

```txt
Rule ID | Java class | Language | Rust status | Tests passing | Notes
```

Example statuses:

```txt
not_started
stubbed
ported
parity_pass
parity_fail
intentionally_skipped
```

# 7. Language migration order

Even if the goal is all languages, do not port all at once.

Use this order:

```txt
1. English
2. German
3. French
4. Spanish
5. Portuguese
6. Dutch
7. Polish
8. Ukrainian
9. Arabic/Urdu/etc.
```

Reason: English/German/French/Spanish will expose most engine features early: tokenization, XML rules, morphology, compound words, spelling, style rules, and complex rule classes.

# 8. Compatibility harness

Build a CLI:

```bash
og-compat compare \
  --java-lt ./languagetool.jar \
  --rust-og ./target/release/og-server \
  --language en-US \
  --input fixtures/en/common_errors.txt
```

Output:

```txt
PASS: same matches
FAIL: offset mismatch in RULE_ID
FAIL: replacement mismatch in SPELLING_RULE
FAIL: Java found rule X, Rust missed it
FAIL: Rust found extra rule Y
```

This harness is your project manager.

# 9. API server parity

The API should accept LT-style params:

```txt
text
language
motherTongue
enabledRules
disabledRules
enabledCategories
disabledCategories
level
picky
```

Return LT-style response:

```txt
software
warnings
language
matches
sentenceRanges
extendedSentenceRanges
```

Focus first on:

```txt
text
language
matches
offset
length
replacements
rule
category
```

Then add the rest.

# 10. Hardest parts

The hard parts are not Axum or XML parsing.

The hard parts are:

```txt
exact offset compatibility
sentence splitting parity
tokenization parity
POS/morphology parity
disambiguation
spellchecker suggestion ranking
language-specific Java rules
compound-word languages like German
rules that depend on Java class behavior
test suite parity across 20+ languages
```

# 11. What to ignore permanently

For this rewrite:

```txt
No AI fallback
No LLM correction
No extension plan
No desktop
No LibreOffice
No Java server
No Java subprocess
No JVM embedding
No TOML/YAML grammar format
No “simplified product MVP” as the final architecture
```

# 12. Execution phases

## Phase 0 — Source audit — [NOT STARTED 0%]

```txt
Freeze upstream LanguageTool commit.
List all Maven modules.
List compute-related modules.
List non-compute modules to ignore.
List all languages.
List all XML files.
List all Java Rule classes.
List all tests.
```

Deliverable:

```txt
COMPATIBILITY_MATRIX.md
LANGUAGE_MODULES.md
JAVA_RULE_PORT_LIST.md
TEST_PORT_LIST.md
```

**Status:** None of the four deliverable documents exist. The upstream LT commit has not been formally frozen.

## Phase 1 — Rust API shell — [DONE 90%]

```txt
Axum server
/v2/check
/v2/languages
LT-compatible response structs
No real checking yet
```

Deliverable:

```txt
curl /v2/check works and returns valid LT-shaped JSON.
```

**Status:** All 3 endpoints working. All 10 planned params parsed. Response has all 6 top-level fields. Remaining gaps: `level`/`picky`/`motherTongue` are dead fields (parsed but not acted on), `language.name` missing from response, `RuleMatch` lacks `type` field, `longCode` always equals `code`.

## Phase 2 — Core text model — [DONE 75%]

```txt
Document
Sentence
Token
AnalyzedSentence
AnalyzedTokenReadings
RuleMatch
offset model
```

Deliverable:

```txt
Can represent same data model as Java engine.
```

**Status:** All 17 planned types exist. Checker pipeline works end-to-end. Gaps: no whitespace-before on tokens, no token type enum, no AnalyzedDocument type, Rule trait missing lifecycle/activation, two incompatible Tagger traits (og-core vs og-tagger), no RuleContext parameter.

## Phase 3 — Tokenizer and sentence splitter — [PARTIAL 50%]

```txt
Port sentence splitting behavior.
Port tokenization behavior.
Preserve exact offsets.
Add tests.
```

Deliverable:

```txt
Tokenizer/sentence tests pass against Java fixtures.
```

**Status:** Working sentence splitting and word tokenization with contraction handling, 130+ tests. Gaps: no SRX rule loading (37 abbreviations vs LT's hundreds), no URL/email detection, no hyphenated word handling, no numbered list detection. Not tested against Java fixtures.

## Phase 4 — XML parser/compiler — [PARTIAL 70%]

```txt
Parse grammar.xml.
Parse style.xml.
Parse examples.
Validate rule IDs.
Compile to Rule AST.
```

Deliverable:

```txt
Can load English grammar.xml without crashing.
```

**Status:** Can load real English grammar.xml (4000+ rules). XML levels 1-6 mostly implemented. Gaps: style.xml never loaded, `<rule description/sub_id>` attrs never parsed, `<url>`/`<short>` content dropped, example corrections always empty, no `<unify>`/`<phrases>`/`<while>`, no rule ID uniqueness validation.

## Phase 5 — Pattern rule engine — [FUNCTIONAL 75%]

```txt
Implement token matcher.
Implement regex matching.
Implement exceptions.
Implement antipatterns.
Implement suggestions.
```

Deliverable:

```txt
Simple XML rule tests pass.
```

**Status:** Full token matching (literal, regex, inflected, negate, POS, chunk, spacebefore, exceptions with scope, skip, min/max, OR/AND groups, antipatterns with overlap immunization, markers, backreferences, suggestion generation with case conversion and regexp transforms). Rayon parallel + first-token index. Gaps: most Java filters are stubs, `default_on`/`deprecated` not honored, `sub_id`/`url`/`short_message` not propagated to RuleMatch, date filters hardcoded to 2014.

## Phase 6 — Spellchecker — [STUB 30%]

```txt
Load dictionaries.
Detect unknown words.
Generate suggestions.
Rank suggestions.
Port spelling rule behavior.
```

Deliverable:

```txt
SPELLING_RULE parity for English basic fixtures.
```

**Status:** HashSet dictionary + brute-force Levenshtein suggestions (distance ≤ 3). SpellingCheckRule implements Rule trait. Can load plain word lists and detect unknown words. Gaps: no Hunspell .dic/.aff parsing, no compound handling, no word frequency ranking, no phonetic matching, O(N*M) suggestion performance, no language-specific spelling rules.

## Phase 7 — POS/morphology/tagging — [PARTIAL 55%]

```txt
Load tagger dictionaries.
Produce lemma/POS readings.
Support disambiguation inputs.
```

Deliverable:

```txt
AnalyzedTokenReadings parity for selected fixtures.
```

**Status:** EnglishTagger works with built-in ~2K-word dictionary + external dict_decoded.txt (259K lines) loading, heuristic unknown-word tagging, irregular verb/adjective lemmatization (~200 forms), morphological suffix stripping. Full Penn Treebank + LT extension tags. Gaps: no statistical POS tagger (LT uses trigram HMM), two incompatible Tagger traits, no auto-loading of external dicts at construction, possible IRREGULAR_FORMS bug (`running => go`), no morphological synthesis, English only.

## Phase 8 — Disambiguation — [PARTIAL 65%]

```txt
Port disambiguation XML.
Apply disambiguation before grammar rules.
Test POS-sensitive XML rules.
```

Deliverable:

```txt
POS-dependent pattern rules start passing.
```

**Status:** XmlDisambiguator loads real English disambiguation.xml (17K lines, >100 rules). Supports: text/regex matching, POS constraints, or/and groups, antipatterns, markers. Actions: setPos, replace, remove, add, filter, filterAll, ignoreSpelling. Applied before grammar rules in the pipeline. Gaps: unify action is a stub (no feature parsing), English only, disambiguation not tested against Java fixtures.

## Phase 9 — Native rule ports — [BARELY STARTED 10%]

```txt
Port Java Rule classes to Rust.
Port TextLevelRule classes.
Port RuleFilter-like hooks.
```

Deliverable:

```txt
Native rule parity per language.
```

**Status:** 4 generic native rules (WordRepeatRule, DoublePunctuationRule, UppercaseSentenceStartRule, CommaWhitespaceRule), 6 generic text-level rules (whitespace, unpaired brackets/quotes, long sentence, repeat beginning), 2 English-specific rules (AvsAnRule, SimpleReplaceRule). DateCheckFilter/NewYearDateFilter implemented as pattern engine filters. Gaps: no port tracking matrix, most Java filters are stubs, only English, no TextLevelRule ports per language.

## Phase 10 — Language modules — [ENGLISH ONLY 5%]

Repeat per language:

```txt
load resources
run XML validation
run pattern tests
run spelling tests
run full check golden tests
port missing native rules
```

Deliverable:

```txt
Language marked parity_pass in compatibility matrix.
```

**Status:** English loads grammar.xml, disambiguation.xml, tagger dictionaries, spelling dictionaries. Has 2 native rules + spelling rule. 0 other languages have any module code (no de/, fr/, es/, pt/, pl/, nl/ directories). Gaps: no style.xml loading, no abbreviation loading, no grammar_custom.xml, no generic language factory, registry disconnected from engine, resources loaded from external LT directory (not vendored).

## Phase 11 — Full test suite parity — [STUB 5%]

```txt
Port JUnit tests to Rust integration tests.
Run Java and Rust golden comparison.
CI enforces parity.
```

Deliverable:

```txt
cargo test passes.
og-compat reports acceptable parity.
```

**Status:** ~254 inline test functions across crates. og-test-runner can run XML example tests on real grammar.xml. og-compat has comparison types but no harness to run Java LT. Gaps: no golden test files or fixture directories, no CI pipeline, no clippy/rustfmt config, og-compat cannot invoke Java LT or Rust OG, GoldenFixture is dead code, many tests use hardcoded absolute paths.

## Phase 12 — Performance — [PARTIAL 20%]

Only after correctness.

```txt
Rule indexing
compiled regex cache
parallel sentence checking
dictionary memory optimization
zero-copy token spans where possible
```

Deliverable:

```txt
Rust equal or faster than Java on same fixtures.
```

**Status:** First-token word index (HashMap) for rule lookup. Rayon parallelism for sentence checking. Gaps: no compiled regex cache, no dictionary memory optimization (all HashSet<String>), no zero-copy token spans, spellchecker is O(N*M) per suggestion, FSA dictionary uses HashMap instead of trie/automaton.

# 13. Final project definition

This is not a new Grammarly clone plan.

This is:

```txt
A Rust-native reimplementation of LanguageTool’s API-facing compute engine,
with LanguageTool XML/resource/test compatibility,
excluding desktop, LibreOffice, Java runtime, and non-compute integrations.
```

Correct success criteria:

```txt
1. Same /v2/check API shape.
2. Same rule IDs where resources are ported.
3. Same offsets.
4. Same suggestions where deterministic.
5. Same test suite behavior.
6. Same multilingual resource model.
7. No Java dependency at runtime.
```

[1]: https://dev.languagetool.org/public-http-api.html?utm_source=chatgpt.com "Public HTTP Proofreading API - dev.languagetool.org"
[2]: https://dev.languagetool.org/development-overview.html?utm_source=chatgpt.com "Development Overview | dev.languagetool.org"
[3]: https://github.com/languagetool-org/languagetool?utm_source=chatgpt.com "languagetool-org/languagetool: Style and Grammar ..."
[4]: https://github.com/languagetool-org/languagetool/blob/master/languagetool-core/src/main/java/org/languagetool/JLanguageTool.java?utm_source=chatgpt.com "JLanguageTool.java"
[5]: https://dev.languagetool.org/tips-and-tricks.html?utm_source=chatgpt.com "Tips and Tricks | dev.languagetool.org"
[6]: https://www.danielnaber.de/publications/fosdem2014.pdf?utm_source=chatgpt.com "How we found a million style and grammar errors in the ..."
[7]: https://github.com/languagetool-org/languagetool-org.github.io/blob/master/tips-and-tricks.md?utm_source=chatgpt.com "languagetool-org.github.io/tips-and-tricks.md at master ..."
[8]: https://github.com/languagetool-org/languagetool/issues/10327?utm_source=chatgpt.com "[en] Maven broken build, LanguageTool-20240216- ..."
