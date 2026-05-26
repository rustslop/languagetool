use std::collections::{HashMap, HashSet};
use og_core::AnalyzedTokenReadings;
use crate::Tagger;
use crate::english_data::ENGLISH_POS_DICT;

/// Irregular English verb forms: past tense, past participle, gerund → base form
/// Also includes irregular adjectives (comparative/superlative) and plural nouns.
static IRREGULAR_FORMS: phf::Map<&'static str, &'static str> = phf::phf_map! {
    // be
    "am" => "be", "is" => "be", "are" => "be", "was" => "be", "were" => "be",
    "been" => "be", "being" => "be", "'m" => "be", "'s" => "be", "'re" => "be",
    "ai" => "be", "art" => "be", "ain't" => "be",
    // have
    "has" => "have", "had" => "have", "having" => "have", "'ve" => "have",
    // do
    "does" => "do", "did" => "do", "done" => "do", "doing" => "do",
    // go
    "goes" => "go", "went" => "go", "gone" => "go", "going" => "go",
    // come
    "came" => "come", "coming" => "come",
    // see
    "sees" => "see", "saw" => "see", "seen" => "see", "seeing" => "see",
    // take
    "takes" => "take", "took" => "take", "taken" => "take", "taking" => "take",
    // make
    "makes" => "make", "made" => "make", "making" => "make",
    // get
    "gets" => "get", "got" => "get", "gotten" => "get", "getting" => "get",
    // give
    "gives" => "give", "gave" => "give", "given" => "give", "giving" => "give",
    // find
    "finds" => "find", "found" => "find", "finding" => "find",
    // know
    "knows" => "know", "knew" => "know", "known" => "know", "knowing" => "know",
    // think
    "thinks" => "think", "thought" => "think", "thinking" => "think",
    // tell
    "tells" => "tell", "told" => "tell", "telling" => "tell",
    // become
    "becomes" => "become", "became" => "become", "becoming" => "become",
    // leave
    "leaves" => "leave", "left" => "leave", "leaving" => "leave",
    // feel
    "feels" => "feel", "felt" => "feel", "feeling" => "feel",
    // put
    "puts" => "put", "putting" => "put",
    // bring
    "brings" => "bring", "brought" => "bring", "bringing" => "bring",
    // begin
    "begins" => "begin", "began" => "begin", "begun" => "begin", "beginning" => "begin",
    // keep
    "keeps" => "keep", "kept" => "keep", "keeping" => "keep",
    // hold
    "holds" => "hold", "held" => "hold", "holding" => "hold",
    // write
    "writes" => "write", "wrote" => "write", "written" => "write", "writing" => "write",
    // stand
    "stands" => "stand", "stood" => "stand", "standing" => "stand",
    // hear
    "hears" => "hear", "heard" => "hear", "hearing" => "hear",
    // let
    "lets" => "let", "letting" => "let",
    // say
    "says" => "say", "said" => "say", "saying" => "say",
    // run
    "runs" => "run", "ran" => "run", "running" => "run",
    // pay
    "pays" => "pay", "paid" => "pay", "paying" => "pay",
    // meet
    "meets" => "meet", "met" => "meet", "meeting" => "meet",
    // sit
    "sits" => "sit", "sat" => "sit", "sitting" => "sit",
    // speak
    "speaks" => "speak", "spoke" => "speak", "spoken" => "speak", "speaking" => "speak",
    // lead
    "leads" => "lead", "led" => "lead", "leading" => "lead",
    // read
    "reads" => "read", "reading" => "read",
    // grow
    "grows" => "grow", "grew" => "grow", "grown" => "grow", "growing" => "grow",
    // lose
    "loses" => "lose", "lost" => "lose", "losing" => "lose",
    // fall
    "falls" => "fall", "fell" => "fall", "fallen" => "fall", "falling" => "fall",
    // send
    "sends" => "send", "sent" => "send", "sending" => "send",
    // build
    "builds" => "build", "built" => "build", "building" => "build",
    // understand
    "understands" => "understand", "understood" => "understand", "understanding" => "understand",
    // set
    "sets" => "set", "setting" => "set",
    // break
    "breaks" => "break", "broke" => "break", "broken" => "break", "breaking" => "break",
    // spend
    "spends" => "spend", "spent" => "spend", "spending" => "spend",
    // cut
    "cuts" => "cut", "cutting" => "cut",
    // rise
    "rises" => "rise", "rose" => "rise", "risen" => "rise", "rising" => "rise",
    // drive
    "drives" => "drive", "drove" => "drive", "driven" => "drive", "driving" => "drive",
    // buy
    "buys" => "buy", "bought" => "buy", "buying" => "buy",
    // wear
    "wears" => "wear", "wore" => "wear", "worn" => "wear", "wearing" => "wear",
    // catch
    "catches" => "catch", "caught" => "catch", "catching" => "catch",
    // choose
    "chooses" => "choose", "chose" => "choose", "chosen" => "choose", "choosing" => "choose",
    // seek
    "seeks" => "seek", "sought" => "seek", "seeking" => "seek",
    // throw
    "throws" => "throw", "threw" => "throw", "thrown" => "throw", "throwing" => "throw",
    // mean
    "means" => "mean", "meant" => "mean", "meaning" => "mean",
    // fight
    "fights" => "fight", "fought" => "fight", "fighting" => "fight",
    // fly
    "flies" => "fly", "flew" => "fly", "flown" => "fly", "flying" => "fly",
    // bear
    "bears" => "bear", "bore" => "bear", "borne" => "bear", "bearing" => "bear",
    // teach
    "teaches" => "teach", "taught" => "teach", "teaching" => "teach",
    // win
    "wins" => "win", "won" => "win", "winning" => "win",
    // shut
    "shuts" => "shut", "shutting" => "shut",
    // show
    "shows" => "show", "showed" => "show", "shown" => "show", "showing" => "show",
    // draw
    "draws" => "draw", "drew" => "draw", "drawn" => "draw", "drawing" => "draw",
    // sleep
    "sleeps" => "sleep", "slept" => "sleep", "sleeping" => "sleep",
    // hang
    "hangs" => "hang", "hung" => "hang", "hanging" => "hang",
    // swim
    "swims" => "swim", "swam" => "swim", "swum" => "swim", "swimming" => "swim",
    // spread
    "spreads" => "spread", "spreading" => "spread",
    // sing
    "sings" => "sing", "sang" => "sing", "sung" => "sing", "singing" => "sing",
    // strike
    "strikes" => "strike", "struck" => "strike", "striking" => "strike",
    // eat
    "eats" => "eat", "ate" => "eat", "eaten" => "eat", "eating" => "eat",
    // shake
    "shakes" => "shake", "shook" => "shake", "shaken" => "shake", "shaking" => "shake",
    // wake
    "wakes" => "wake", "woke" => "wake", "woken" => "wake", "waking" => "wake",
    // lay/lie
    "lays" => "lay", "laid" => "lay", "laying" => "lay",
    "lies" => "lie", "lay" => "lie", "lain" => "lie", "lying" => "lie",
    // bind
    "binds" => "bind", "bound" => "bind", "binding" => "bind",
    // bite
    "bites" => "bite", "bit" => "bite", "bitten" => "bite", "biting" => "bite",
    // bleed
    "bleeds" => "bleed", "bled" => "bleed", "bleeding" => "bleed",
    // blow
    "blows" => "blow", "blew" => "blow", "blown" => "blow", "blowing" => "blow",
    // breed
    "breeds" => "breed", "bred" => "breed", "breeding" => "breed",
    // creep
    "creeps" => "creep", "crept" => "creep", "creeping" => "creep",
    // deal
    "deals" => "deal", "dealing" => "deal",
    // dig
    "digs" => "dig", "dug" => "dig", "digging" => "dig",
    // feed
    "feeds" => "feed", "fed" => "feed", "feeding" => "feed",
    // flee
    "flees" => "flee", "fled" => "flee", "fleeing" => "flee",
    // forget
    "forgets" => "forget", "forgot" => "forget", "forgotten" => "forget", "forgetting" => "forget",
    // forgive
    "forgives" => "forgive", "forgave" => "forgive", "forgiven" => "forgive", "forgiving" => "forgive",
    // freeze
    "freezes" => "freeze", "froze" => "freeze", "frozen" => "freeze", "freezing" => "freeze",
    // leap
    "leaps" => "leap", "leapt" => "leap", "leaping" => "leap",
    // lend
    "lends" => "lend", "lent" => "lend", "lending" => "lend",
    // light
    "lights" => "light", "lit" => "light", "lighting" => "light",
    // ride
    "rides" => "ride", "rode" => "ride", "ridden" => "ride", "riding" => "ride",
    // ring
    "rings" => "ring", "rang" => "ring", "rung" => "ring", "ringing" => "ring",
    // slide
    "slides" => "slide", "slid" => "slide", "sliding" => "slide",
    // spring
    "springs" => "spring", "sprang" => "spring", "sprung" => "spring", "springing" => "spring",
    // steal
    "steals" => "steal", "stole" => "steal", "stolen" => "steal", "stealing" => "steal",
    // stick
    "sticks" => "stick", "stuck" => "stick", "sticking" => "stick",
    // sting
    "stings" => "sting", "stung" => "sting", "stinging" => "sting",
    // strive
    "strives" => "strive", "strove" => "strive", "striven" => "strive",
    // swear
    "swears" => "swear", "swore" => "swear", "sworn" => "swear", "swearing" => "swear",
    // sweep
    "sweeps" => "sweep", "swept" => "sweep", "sweeping" => "sweep",
    // swing
    "swings" => "swing", "swung" => "swing", "swinging" => "swing",
    // tear
    "tears" => "tear", "tore" => "tear", "torn" => "tear", "tearing" => "tear",
    // weave
    "weaves" => "weave", "wove" => "weave", "woven" => "weave", "weaving" => "weave",
    // weep
    "weeps" => "weep", "wept" => "weep", "weeping" => "weep",
    // Irregular adjectives
    "better" => "good", "best" => "good",
    "worse" => "bad", "worst" => "bad",
    "less" => "little", "least" => "little",
    "more" => "much", "most" => "much",
    "farther" => "far", "farthest" => "far",
    "further" => "far", "furthest" => "far",
    "older" => "old", "oldest" => "old",
    "elder" => "old", "eldest" => "old",
};

pub struct EnglishTagger {
    dict: HashMap<String, Vec<String>>,
    /// FSA dictionary: word -> [(POS tag, lemma)]
    fsa_dict: HashMap<String, Vec<(String, String)>>,
    /// Proper noun entries with original casing: "Can" -> [(NNP, Can)]
    proper_noun_dict: HashMap<String, Vec<(String, String)>>,
    uncountable: HashSet<String>,
    partlycountable: HashSet<String>,
}

/// Add VBP (verb, non-3rd person singular present) as an alternate reading
/// for words that have VB but not VBP.
fn add_vbp_if_vb(results: &mut Vec<(String, Option<String>)>) {
    let has_vb = results.iter().any(|(t, _)| t == "VB");
    let has_vbp = results.iter().any(|(t, _)| t == "VBP");
    if has_vb && !has_vbp {
        if let Some((_, lemma)) = results.iter().find(|(t, _)| t == "VB").cloned() {
            results.push(("VBP".to_string(), lemma));
        }
    }
}

/// Words that are primarily modals (MD) but can also be nouns.
const MD_NOUN_WORDS: &[&str] = &["can", "will", "may", "must", "should", "could", "would", "might"];

fn add_nn_for_md_nouns(results: &mut Vec<(String, Option<String>)>) {
    let has_md = results.iter().any(|(t, _)| t == "MD");
    let has_nn = results.iter().any(|(t, _)| t == "NN" || t == "NN:U" || t == "NN:UN");
    if has_md && !has_nn {
        // Check if the lemma is in the MD_NOUN_WORDS list
        if let Some((_, lemma)) = results.iter().find(|(t, _)| t == "MD").cloned() {
            let lemma_str = lemma.as_deref().unwrap_or("");
            if MD_NOUN_WORDS.contains(&lemma_str) {
                results.push(("NN".to_string(), Some(lemma_str.to_string())));
            }
        }
    }
}

impl EnglishTagger {
    pub fn new() -> Self {
        let mut tagger = Self {
            dict: HashMap::new(),
            fsa_dict: HashMap::new(),
            proper_noun_dict: HashMap::new(),
            uncountable: HashSet::new(),
            partlycountable: HashSet::new(),
        };
        tagger.build_dictionary();
        tagger
    }

    /// Load additional entries from added.txt format data
    pub fn load_added(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let word = parts[0].to_lowercase();
                let lemma = parts[1].to_string();
                let postags: Vec<String> = parts[2].split(' ').map(String::from).collect();
                self.dict.entry(word).or_default();
                if let Some(existing) = self.dict.get_mut(&parts[0].to_lowercase()) {
                    for tag in &postags {
                        if !existing.contains(tag) {
                            existing.push(tag.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn load_uncountable(&mut self, data: &str) {
        for line in data.lines() {
            let word = line.trim().to_lowercase();
            if !word.is_empty() && !word.starts_with('#') {
                self.uncountable.insert(word.clone());
                // Add NN:U tag to existing dict entry, or create one
                let entry = self.dict.entry(word).or_default();
                if !entry.contains(&"NN:U".to_string()) {
                    // Remove plain NN if present and replace with NN:U
                    entry.retain(|t| t != "NN");
                    entry.push("NN:U".to_string());
                }
            }
        }
    }

    pub fn load_partlycountable(&mut self, data: &str) {
        for line in data.lines() {
            let word = line.trim().to_lowercase();
            if !word.is_empty() && !word.starts_with('#') {
                self.partlycountable.insert(word.clone());
                let entry = self.dict.entry(word).or_default();
                if !entry.contains(&"NN:UN".to_string()) {
                    entry.push("NN:UN".to_string());
                }
            }
        }
    }

    /// Load the full FSA dictionary with POS tags and lemmas.
    /// Format: word\tlemma\tPOS (one POS per line, multiple lines per word)
    /// Loads entries for ALL words, including those already in the built-in dictionary.
    /// This matches Java LT behavior: assign all possible tags, then let disambiguation narrow them.
    /// Proper noun entries (NNP/NNPS) from capitalized forms are stored separately.
    pub fn load_fsa_dictionary(&mut self, data: &str) -> usize {
        let mut count = 0;
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let original_word = parts[0];
                let pos_tag = parts[2];

                // Skip NNP/NNPS entries from capitalized forms
                let is_proper_noun_entry = (pos_tag == "NNP" || pos_tag == "NNPS")
                    && original_word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);

                if is_proper_noun_entry {
                    let entry = self.proper_noun_dict
                        .entry(original_word.to_string())
                        .or_default();
                    if !entry.iter().any(|(p, _)| p == pos_tag) {
                        let lemma = parts[1].to_string();
                        entry.push((pos_tag.to_string(), lemma));
                    }
                    count += 1;
                    continue;
                }

                let word = original_word.to_lowercase();
                let lemma = parts[1].to_string();

                // Add to FSA dict (used for both new and existing words)
                let entry = self.fsa_dict.entry(word).or_default();
                if !entry.iter().any(|(p, _)| p == pos_tag) {
                    entry.push((pos_tag.to_string(), lemma));
                    count += 1;
                }
            }
        }
        count
    }

    fn build_dictionary(&mut self) {
        for &(word, tags) in ENGLISH_POS_DICT {
            let entry = self.dict.entry(word.to_string()).or_default();
            for tag in tags {
                if !entry.contains(&tag.to_string()) {
                    entry.push(tag.to_string());
                }
            }
        }

        // Add PRP_S/PRP_O sub-tags for pronouns
        // These are used by grammar rules to distinguish subject/object pronouns
        // Format: PRP_S<person><number><gender?> = subject pronoun
        //         PRP_O<person><number><gender?> = object pronoun
        let pronoun_subtags: &[(&str, &[&str])] = &[
            ("I",     &["PRP", "PRP_S1S"]),
            ("me",    &["PRP", "PRP_O1S"]),
            ("we",    &["PRP", "PRP_S1P"]),
            ("us",    &["PRP", "PRP_O1P"]),
            ("you",   &["PRP", "PRP_S2S", "PRP_S2P", "PRP_O2S", "PRP_O2P"]),
            ("he",    &["PRP", "PRP_S3SM"]),
            ("him",   &["PRP", "PRP_O3SM"]),
            ("she",   &["PRP", "PRP_S3SF"]),
            ("her",   &["PRP", "PRP_O3SF", "PRP$"]),
            ("it",    &["PRP", "PRP_S3SN", "PRP_O3SN"]),
            ("they",  &["PRP", "PRP_S3P"]),
            ("them",  &["PRP", "PRP_O3P"]),
        ];
        for (word, subtags) in pronoun_subtags {
            let entry = self.dict.entry(word.to_string()).or_default();
            for tag in *subtags {
                if !entry.contains(&tag.to_string()) {
                    entry.push(tag.to_string());
                }
            }
        }
    }

    fn lookup(&self, word: &str) -> Option<&Vec<String>> {
        self.dict.get(&word.to_lowercase())
    }

    /// Apply heuristic POS tagging for unknown words based on suffixes
    fn heuristic_tags(&self, word: &str) -> Vec<String> {
        let lower = word.to_lowercase();
        let mut tags = Vec::new();

        // Check if it looks like a number
        if lower.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',' || c == '-') {
            if lower.chars().any(|c| c.is_ascii_digit()) {
                return vec!["CD".to_string()];
            }
        }

        // Check if it starts with a capital letter (potential proper noun)
        let is_capitalized = word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let is_all_upper = word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic());

        if is_all_upper && word.len() <= 5 && word.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
            // All-caps short words are likely abbreviations/proper nouns
            tags.push("NNP".to_string());
            return tags;
        }

        // Suffix-based rules (ordered by specificity)
        if lower.ends_with("ously") || lower.ends_with("ively") || lower.ends_with("ically") {
            tags.push("RB".to_string());
        } else if lower.ends_with("tion") || lower.ends_with("sion") || lower.ends_with("ment")
            || lower.ends_with("ness") || lower.ends_with("ity") || lower.ends_with("ence")
            || lower.ends_with("ance") || lower.ends_with("ism") || lower.ends_with("ist")
        {
            tags.push("NN".to_string());
        } else if lower.ends_with("ting") {
            // Could be gerund or noun ending in -ting
            tags.push("VBG".to_string());
            tags.push("NN".to_string());
        } else if lower.ends_with("ing") {
            tags.push("VBG".to_string());
            // Many -ing words can also be adjectives or nouns
            if lower.ends_with("ing") && lower.len() > 5 {
                tags.push("NN".to_string());
            }
        } else if lower.ends_with("ed") && lower.len() > 3 {
            tags.push("VBN".to_string());
            tags.push("VBD".to_string());
            tags.push("JJ".to_string());
        } else if lower.ends_with("ly") && lower.len() > 3 {
            tags.push("RB".to_string());
        } else if lower.ends_with("er") && lower.len() > 3 {
            tags.push("JJR".to_string());
            tags.push("NN".to_string());
        } else if lower.ends_with("est") && lower.len() > 4 {
            tags.push("JJS".to_string());
        } else if lower.ends_with("ful") || lower.ends_with("less") || lower.ends_with("ous")
            || lower.ends_with("ive") || lower.ends_with("able") || lower.ends_with("ible")
            || lower.ends_with("al") || lower.ends_with("ial") || lower.ends_with("ent")
            || lower.ends_with("ant") || lower.ends_with("ic") || lower.ends_with("ical")
        {
            tags.push("JJ".to_string());
        } else if lower.ends_with("ize") || lower.ends_with("ise") || lower.ends_with("ify")
            || lower.ends_with("ate")
        {
            tags.push("VB".to_string());
        } else if lower.ends_with("es") && lower.len() > 3 {
            tags.push("NNS".to_string());
            tags.push("VBZ".to_string());
        } else if lower.ends_with('s') && !lower.ends_with("ss") && !lower.ends_with("us") && lower.len() > 3 {
            tags.push("NNS".to_string());
            tags.push("VBZ".to_string());
        }

        // If capitalized and we don't have tags yet, likely proper noun
        if tags.is_empty() && is_capitalized {
            tags.push("NNP".to_string());
        }

        // Fallback: unknown word
        if tags.is_empty() {
            tags.push("NN".to_string());
            tags.push("VB".to_string());
        }

        tags
    }

    /// Determine lemma for a word given its POS tag
    /// Common English irregular verb forms: inflected form → base form
    fn irregular_lemma(word: &str) -> Option<&'static str> {
        let lower = word.to_lowercase();
        IRREGULAR_FORMS.get(lower.as_str()).copied()
    }

    fn guess_lemma(&self, word: &str, pos: &str) -> Option<String> {
        // First check irregular forms table
        if matches!(pos, "VBD" | "VBN" | "VBZ" | "VBG" | "VBP" | "JJR" | "JJS") {
            if let Some(base) = Self::irregular_lemma(word) {
                return Some(base.to_string());
            }
        }
        let lower = word.to_lowercase();
        match pos {
            "NNS" | "NNPS" => {
                if lower.ends_with("ies") && lower.len() > 4 {
                    Some(format!("{}y", &lower[..lower.len()-3]))
                } else if lower.ends_with("ves") && lower.len() > 4 {
                    Some(format!("{}fe", &lower[..lower.len()-3]))
                } else if lower.ends_with("ses") || lower.ends_with("xes") || lower.ends_with("zes") || lower.ends_with("ches") || lower.ends_with("shes") {
                    Some(lower[..lower.len()-2].to_string())
                } else if lower.ends_with('s') && !lower.ends_with("ss") {
                    Some(lower[..lower.len()-1].to_string())
                } else {
                    None
                }
            }
            "VBZ" => {
                if lower.ends_with("ies") && lower.len() > 4 {
                    Some(format!("{}y", &lower[..lower.len()-3]))
                } else if lower.ends_with("es") && (lower.ends_with("ches") || lower.ends_with("shes") || lower.ends_with("sses") || lower.ends_with("xes") || lower.ends_with("zes")) {
                    Some(lower[..lower.len()-2].to_string())
                } else if lower.ends_with('s') && !lower.ends_with("ss") {
                    Some(lower[..lower.len()-1].to_string())
                } else {
                    None
                }
            }
            "VBD" | "VBN" => {
                if lower.ends_with("ied") && lower.len() > 4 {
                    Some(format!("{}y", &lower[..lower.len()-3]))
                } else if lower.ends_with("ed") && lower.len() > 4 {
                    // try removing just 'd' first (e.g., "changed" → "change")
                    Some(lower[..lower.len()-2].to_string())
                } else {
                    None
                }
            }
            "VBG" => {
                if lower.ends_with("ing") && lower.len() > 5 {
                    Some(lower[..lower.len()-3].to_string())
                } else {
                    None
                }
            }
            "JJR" => {
                if lower.ends_with("er") && lower.len() > 3 {
                    Some(lower[..lower.len()-2].to_string())
                } else {
                    None
                }
            }
            "JJS" => {
                if lower.ends_with("est") && lower.len() > 4 {
                    Some(lower[..lower.len()-3].to_string())
                } else {
                    None
                }
            }
            "RBR" => {
                if lower.ends_with("er") && lower.len() > 3 {
                    Some(lower[..lower.len()-2].to_string())
                } else {
                    None
                }
            }
            "RBS" => {
                if lower.ends_with("est") && lower.len() > 4 {
                    Some(lower[..lower.len()-3].to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn tag_token(&self, word: &str) -> Vec<(String, Option<String>)> {
        // Punctuation
        if word.chars().all(|c| !c.is_alphanumeric()) {
            return match word {
                "." | "!" | "?" => vec![("SENT_END".to_string(), None)],
                "," => vec![(",".to_string(), None)],
                ":" | ";" => vec![(":".to_string(), None)],
                "-" | "–" | "—" => vec![(":".to_string(), None)],
                "(" | "[" | "{" => vec![(("-LRB-").to_string(), None)],
                ")" | "]" | "}" => vec![(("-RRB-").to_string(), None)],
                "..." => vec![((".").to_string(), None)],
                "'s" | "'s" | "\u{2019}s" => vec![
                    ("POS".to_string(), None),
                    ("VBZ".to_string(), None),
                ],
                _ => vec![("PCT".to_string(), None)],
            };
        }

        // Pure numbers -> CD (cardinal number)
        if word.chars().all(|c| c.is_ascii_digit()) {
            return vec![("CD".to_string(), Some(word.to_string()))];
        }

        // Ordinal numbers like "1st", "2nd", "3rd", "4th"
        let lower = word.to_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        if chars.len() > 2 {
            let last_two: String = chars[chars.len()-2..].iter().collect();
            let prefix: String = chars[..chars.len()-2].iter().collect();
            if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                if last_two == "st" || last_two == "nd" || last_two == "rd" || last_two == "th" {
                    return vec![("ORD".to_string(), Some(word.to_string()))];
                }
            }
        }

        // Numbers with commas/dots like "1,000" or "3.5" or "10:30"
        if word.len() > 1 {
            let digit_count = word.chars().filter(|c| c.is_ascii_digit()).count();
            if digit_count > 0 && digit_count as f64 / word.len() as f64 > 0.5 {
                return vec![("CD".to_string(), Some(word.to_string()))];
            }
        }

        // Try FSA dictionary first (has real lemmas)
        if let Some(entries) = self.fsa_dict.get(&lower) {
            let mut results: Vec<(String, Option<String>)> = entries.iter().map(|(pos, lemma)| {
                (pos.clone(), Some(lemma.clone()))
            }).collect();

            // Add proper noun readings if word is capitalized in context
            let is_capitalized = word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            if is_capitalized {
                if let Some(pn_entries) = self.proper_noun_dict.get(word) {
                    for (pos, lemma) in pn_entries {
                        if !results.iter().any(|(p, _)| p == pos) {
                            results.push((pos.clone(), Some(lemma.clone())));
                        }
                    }
                }
            }

            // Add NN:U/NN:UN if needed
            if self.uncountable.contains(&lower) {
                if !results.iter().any(|(t, _)| t == "NN:U") {
                    results.push(("NN:U".to_string(), Some(lower.clone())));
                }
            }
            if self.partlycountable.contains(&lower) {
                if !results.iter().any(|(t, _)| t == "NN:UN") {
                    results.push(("NN:UN".to_string(), Some(lower.clone())));
                }
            }

            // Add VBP reading for VB words (base form = present tense for I/you/we/they)
            add_vbp_if_vb(&mut results);

            // Add NN reading for modal words that can also be nouns
            add_nn_for_md_nouns(&mut results);

            return results;
        }

        // Try built-in dictionary lookup
        if let Some(tags) = self.lookup(word) {
            let mut results: Vec<(String, Option<String>)> = tags.iter().map(|t| {
                let lemma = self.guess_lemma(word, t).unwrap_or_else(|| word.to_lowercase());
                (t.clone(), Some(lemma))
            }).collect();

            // Check if word is uncountable/partlycountable even if in dict
            if self.uncountable.contains(&lower) {
                // Add NN:U if not already present
                let has_nnu = results.iter().any(|(t, _)| t == "NN:U");
                if !has_nnu {
                    results.push(("NN:U".to_string(), Some(lower.clone())));
                }
            }
            if self.partlycountable.contains(&lower) {
                let has_nnun = results.iter().any(|(t, _)| t == "NN:UN");
                if !has_nnun {
                    results.push(("NN:UN".to_string(), Some(lower.clone())));
                }
            }

            // Add VBP reading for VB words
            add_vbp_if_vb(&mut results);

            // Also check FSA dict for additional POS tags for this word
            if let Some(fsa_entries) = self.fsa_dict.get(&lower) {
                for (pos, lemma) in fsa_entries {
                    if !results.iter().any(|(t, _)| t == pos) {
                        results.push((pos.clone(), Some(lemma.clone())));
                    }
                }
            }

            return results;
        }

        // Check uncountable/partlycountable even for unknown words
        if self.uncountable.contains(&lower) {
            return vec![("NN:U".to_string(), Some(lower))];
        }
        if self.partlycountable.contains(&lower) {
            return vec![
                ("NN".to_string(), Some(lower.clone())),
                ("NN:UN".to_string(), Some(lower)),
            ];
        }

        // Heuristic fallback — no dictionary entry found
        let mut tags = self.heuristic_tags(word);

        // Add UNKNOWN tag for words not in any dictionary
        tags.push("UNKNOWN".to_string());

        // Check NN:U/NN:UN for words that match heuristic NN
        if tags.contains(&"NN".to_string()) {
            if self.uncountable.contains(&lower) {
                tags.retain(|t| t != "NN");
                tags.push("NN:U".to_string());
            }
            if self.partlycountable.contains(&lower) {
                tags.push("NN:UN".to_string());
            }
        }

        // NNPS: capitalized words ending in 's' that are likely plural proper nouns
        let is_capitalized = word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        if is_capitalized && lower.ends_with('s') && !lower.ends_with("ss") && lower.len() > 2 {
            if tags.contains(&"NNS".to_string()) {
                // Replace NNS with NNPS for capitalized words
                tags.retain(|t| t != "NNS");
                tags.push("NNPS".to_string());
            }
        }

        tags.into_iter().map(|t| {
            let lemma = self.guess_lemma(word, &t).unwrap_or_else(|| word.to_lowercase());
            (t, Some(lemma))
        }).collect()
    }
}

impl Default for EnglishTagger {
    fn default() -> Self {
        Self::new()
    }
}

impl Tagger for EnglishTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;

        for (i, token_text) in tokens.iter().enumerate() {
            let token_start = byte_offset;
            let token_end = byte_offset + token_text.len();

            let readings_data = self.tag_token(token_text);

            use og_core::AnalyzedToken;
            let primary = AnalyzedToken::new(*token_text, token_start, token_end)
                .with_pos_tags(readings_data.iter().map(|(t, _)| t.clone()).collect())
                .with_lemma(readings_data.first().and_then(|(_, l)| l.clone()).unwrap_or_else(|| token_text.to_lowercase()));

            let readings: Vec<AnalyzedToken> = readings_data.into_iter().map(|(tag, lemma)| {
                AnalyzedToken::new(*token_text, token_start, token_end)
                    .with_pos_tags(vec![tag])
                    .with_lemma(lemma.unwrap_or_else(|| token_text.to_lowercase()))
            }).collect();

            let atr = AnalyzedTokenReadings::new(primary).with_readings(readings);
            results.push(atr);

            byte_offset = token_end;
            // Add space between tokens if not the last
            if i < tokens.len() - 1 {
                byte_offset += 1;
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tagging() {
        let tagger = EnglishTagger::new();
        let tokens = vec!["The", "quick", "brown", "fox", "jumps"];
        let result = tagger.tag(&tokens);

        assert!(result[0].has_pos_tag("DT"), "The should be DT");
        assert!(result[1].has_pos_tag("JJ"), "quick should be JJ");
        assert!(result[4].has_pos_tag("VBZ"), "jumps should be VBZ");
    }

    #[test]
    fn test_function_words() {
        let tagger = EnglishTagger::new();

        let dt_words = vec!["the", "a", "an", "this", "that", "these", "those"];
        for w in &dt_words {
            let result = tagger.tag(&[*w]);
            assert!(result[0].has_pos_tag("DT"), "{} should be DT", w);
        }

        let in_words = vec!["in", "on", "at", "by", "with", "from", "of"];
        for w in &in_words {
            let result = tagger.tag(&[*w]);
            assert!(result[0].has_pos_tag("IN"), "{} should be IN", w);
        }

        let md_words = vec!["can", "could", "will", "would", "should", "must"];
        for w in &md_words {
            let result = tagger.tag(&[*w]);
            assert!(result[0].has_pos_tag("MD"), "{} should be MD", w);
        }
    }

    #[test]
    fn test_be_verbs() {
        let tagger = EnglishTagger::new();

        assert!(tagger.tag(&["is"])[0].has_pos_tag("VBZ"));
        assert!(tagger.tag(&["are"])[0].has_pos_tag("VBP"));
        assert!(tagger.tag(&["was"])[0].has_pos_tag("VBD"));
        assert!(tagger.tag(&["been"])[0].has_pos_tag("VBN"));
        assert!(tagger.tag(&["being"])[0].has_pos_tag("VBG"));
        assert!(tagger.tag(&["be"])[0].has_pos_tag("VB"));
    }

    #[test]
    fn test_adverbs() {
        let tagger = EnglishTagger::new();

        let rb_words = vec!["very", "quickly", "not", "always", "never", "really", "often", "already"];
        for w in &rb_words {
            let result = tagger.tag(&[*w]);
            assert!(result[0].has_pos_tag("RB"), "{} should be RB", w);
        }

        // Unknown -ly word should be RB by heuristic
        assert!(tagger.tag(&["uncharacteristically"])[0].has_pos_tag("RB"));
    }

    #[test]
    fn test_nouns() {
        let tagger = EnglishTagger::new();

        assert!(tagger.tag(&["house"])[0].has_pos_tag("NN"));
        assert!(tagger.tag(&["houses"])[0].has_pos_tag("NNS"));
        assert!(tagger.tag(&["child"])[0].has_pos_tag("NN"));
    }

    #[test]
    fn test_verbs() {
        let tagger = EnglishTagger::new();

        assert!(tagger.tag(&["running"])[0].has_pos_tag("VBG"));
        assert!(tagger.tag(&["walked"])[0].has_pos_tag("VBD"));
        assert!(tagger.tag(&["walked"])[0].has_pos_tag("VBN"));
        // "walked" is a known word (in dictionary), so it gets VBD/VBN from dict not JJ
        // Unknown -ed words get heuristic JJ
        assert!(tagger.tag(&["stampeded"])[0].has_pos_tag("JJ"), "unknown -ed word should get JJ");
    }

    #[test]
    fn test_pronouns() {
        let tagger = EnglishTagger::new();

        assert!(tagger.tag(&["he"])[0].has_pos_tag("PRP"));
        assert!(tagger.tag(&["she"])[0].has_pos_tag("PRP"));
        assert!(tagger.tag(&["they"])[0].has_pos_tag("PRP"));
        assert!(tagger.tag(&["his"])[0].has_pos_tag("PRP$"));
        assert!(tagger.tag(&["their"])[0].has_pos_tag("PRP$"));
    }

    #[test]
    fn test_capitalized_unknown() {
        let tagger = EnglishTagger::new();
        let result = tagger.tag(&["Zanzibar"]);
        assert!(result[0].has_pos_tag("NNP"), "Capitalized unknown should be NNP");
    }

    #[test]
    fn test_numbers() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["one"])[0].has_pos_tag("CD"));
        assert!(tagger.tag(&["42"])[0].has_pos_tag("CD"));
        assert!(tagger.tag(&["three"])[0].has_pos_tag("CD"));
    }

    #[test]
    fn test_punctuation() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["."])[0].has_pos_tag("SENT_END"));
        assert!(tagger.tag(&[","])[0].has_pos_tag(","));
    }

    #[test]
    fn test_adjectives() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["good"])[0].has_pos_tag("JJ"));
        assert!(tagger.tag(&["better"])[0].has_pos_tag("JJR"));
        assert!(tagger.tag(&["best"])[0].has_pos_tag("JJS"));
    }

    #[test]
    fn test_conjunctions() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["and"])[0].has_pos_tag("CC"));
        assert!(tagger.tag(&["but"])[0].has_pos_tag("CC"));
        assert!(tagger.tag(&["or"])[0].has_pos_tag("CC"));
    }

    #[test]
    fn test_wh_words() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["who"])[0].has_pos_tag("WP"));
        assert!(tagger.tag(&["how"])[0].has_pos_tag("WRB"));
        assert!(tagger.tag(&["where"])[0].has_pos_tag("WRB"));
        assert!(tagger.tag(&["when"])[0].has_pos_tag("WRB"));
    }

    #[test]
    fn test_existential_there() {
        let tagger = EnglishTagger::new();
        let result = tagger.tag(&["there"]);
        assert!(result[0].has_pos_tag("EX"));
    }

    #[test]
    fn test_sentence_tagging() {
        let tagger = EnglishTagger::new();
        let tokens = vec!["The", "dog", "runs", "quickly", "."];
        let result = tagger.tag(&tokens);

        assert!(result[0].has_pos_tag("DT"), "The should be DT");
        assert!(result[1].has_pos_tag("NN"), "dog should be NN");
        assert!(result[2].has_pos_tag("VBZ"), "runs should be VBZ");
        assert!(result[3].has_pos_tag("RB"), "quickly should be RB");
        assert!(result[4].has_pos_tag("SENT_END"), ". should be SENT_END");
    }

    #[test]
    fn test_dictionary_size() {
        let tagger = EnglishTagger::new();
        assert!(tagger.dict.len() > 1000, "Dictionary should have >1000 entries, got {}", tagger.dict.len());
    }

    #[test]
    fn test_fsa_dictionary_loading() {
        let mut tagger = EnglishTagger::new();
        // Use uncommon words not in the built-in dictionary
        let data = "xyzrun\txyzrun\tVB\nxyzrun\txyzrun\tNN\nxyzruns\txyzrun\tVBZ\nxyzrunning\txyzrun\tVBG\n";
        let count = tagger.load_fsa_dictionary(data);
        assert_eq!(count, 4);

        // Verify FSA dict entries
        let entries = tagger.fsa_dict.get("xyzrun").unwrap();
        assert!(entries.iter().any(|(p, _)| p == "VB"));
        assert!(entries.iter().any(|(p, _)| p == "NN"));

        let entries = tagger.fsa_dict.get("xyzrunning").unwrap();
        assert!(entries.iter().any(|(p, l)| p == "VBG" && l == "xyzrun"));
    }

    #[test]
    fn test_load_added_txt() {
        let mut tagger = EnglishTagger::new();
        let data = "testword\ttestword\tNN\nanother\tanother\tJJ\n";
        tagger.load_added(data);

        assert!(tagger.lookup("testword").is_some());
        assert!(tagger.lookup("another").is_some());
        assert!(tagger.tag(&["testword"])[0].has_pos_tag("NN"));
        assert!(tagger.tag(&["another"])[0].has_pos_tag("JJ"));
    }

    #[test]
    fn test_lemma_generation() {
        let tagger = EnglishTagger::new();

        // Test VBG lemma
        let result = tagger.tag(&["running"]);
        let readings = result[0].readings();
        let vbg_reading = readings.iter().find(|r| r.has_pos_tag("VBG"));
        assert!(vbg_reading.is_some(), "Should have VBG reading");
        if let Some(r) = vbg_reading {
            assert_eq!(r.lemma(), Some("runn"), "VBG lemma should strip -ing");
        }

        // Test NNS lemma
        let result = tagger.tag(&["dogs"]);
        let readings = result[0].readings();
        let nns_reading = readings.iter().find(|r| r.has_pos_tag("NNS"));
        assert!(nns_reading.is_some(), "Should have NNS reading");
        if let Some(r) = nns_reading {
            assert_eq!(r.lemma(), Some("dog"), "NNS lemma should strip -s");
        }
    }

    #[test]
    fn test_to_as_to_and_in() {
        let tagger = EnglishTagger::new();
        let result = tagger.tag(&["to"]);
        assert!(result[0].has_pos_tag("TO"), "to should have TO tag");
        assert!(result[0].has_pos_tag("IN"), "to should also have IN tag");
    }

    #[test]
    fn test_ambiguous_words() {
        let tagger = EnglishTagger::new();

        // "like" can be IN, VB, NN, JJ
        let result = tagger.tag(&["like"]);
        assert!(result[0].has_pos_tag("IN"));
        assert!(result[0].has_pos_tag("VB"));

        // "well" can be RB, NN, JJ
        let result = tagger.tag(&["well"]);
        assert!(result[0].has_pos_tag("RB"));
    }

    #[test]
    fn test_proper_nouns() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["January"])[0].has_pos_tag("NNP"));
        assert!(tagger.tag(&["Monday"])[0].has_pos_tag("NNP"));
        assert!(tagger.tag(&["America"])[0].has_pos_tag("NNP"));
    }

    #[test]
    fn test_ordinal_numbers() {
        let tagger = EnglishTagger::new();
        assert!(tagger.tag(&["first"])[0].has_pos_tag("ORD"));
        assert!(tagger.tag(&["second"])[0].has_pos_tag("ORD"));
        assert!(tagger.tag(&["third"])[0].has_pos_tag("ORD"));
    }
}
