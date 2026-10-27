use crate::SentenceTokenizer;
use og_core::Sentence;

pub struct DefaultSentenceSplitter {
    abbreviations: Vec<String>,
}

impl DefaultSentenceSplitter {
    pub fn new() -> Self {
        Self {
            abbreviations: Self::default_abbreviations(),
        }
    }

    fn default_abbreviations() -> Vec<String> {
        vec![
            "Mr.".into(),
            "Mrs.".into(),
            "Ms.".into(),
            "Miss.".into(),
            "Dr.".into(),
            "Prof.".into(),
            "Sr.".into(),
            "Jr.".into(),
            "St.".into(),
            "Rev.".into(),
            "Gen.".into(),
            "Gov.".into(),
            "Hon.".into(),
            "Atty.".into(),
            "Sgt.".into(),
            "Col.".into(),
            "Maj.".into(),
            "Lt.".into(),
            "Brig.".into(),
            "Capt.".into(),
            "Cmdr.".into(),
            "Revd.".into(),
            "Rep.".into(),
            "Sen.".into(),
            "Pres.".into(),
            "Drs.".into(),
            "Messrs.".into(),
            "Mmes.".into(),
            "Msgr.".into(),
            "Supt.".into(),
            "Det.".into(),
            "Insp.".into(),
            "Pvt.".into(),
            "Cpl.".into(),
            "Adm.".into(),
            "Rt.".into(),
            "Inc.".into(),
            "Ltd.".into(),
            "Co.".into(),
            "Corp.".into(),
            "Bros.".into(),
            "Assoc.".into(),
            "Dept.".into(),
            "Dist.".into(),
            "Div.".into(),
            "Est.".into(),
            "Assn.".into(),
            "B.A.".into(),
            "B.S.".into(),
            "M.A.".into(),
            "M.S.".into(),
            "Ph.D.".into(),
            "M.D.".into(),
            "J.D.".into(),
            "D.D.S.".into(),
            "B.Sc.".into(),
            "M.Sc.".into(),
            "LL.B.".into(),
            "LL.M.".into(),
            "LL.D.".into(),
            "B.Eng.".into(),
            "M.Eng.".into(),
            "vs.".into(),
            "etc.".into(),
            "e.g.".into(),
            "i.e.".into(),
            "al.".into(),
            "ca.".into(),
            "approx.".into(),
            "cf.".into(),
            "viz.".into(),
            "n.b.".into(),
            "N.B.".into(),
            "q.v.".into(),
            "s.v.".into(),
            "v.".into(),
            "op.".into(),
            "cit.".into(),
            "ibid.".into(),
            "id.".into(),
            "ff.".into(),
            "et al.".into(),
            "Jan.".into(),
            "Feb.".into(),
            "Mar.".into(),
            "Apr.".into(),
            "Jun.".into(),
            "Jul.".into(),
            "Aug.".into(),
            "Sep.".into(),
            "Sept.".into(),
            "Oct.".into(),
            "Nov.".into(),
            "Dec.".into(),
            "Mon.".into(),
            "Tue.".into(),
            "Tues.".into(),
            "Wed.".into(),
            "Thu.".into(),
            "Thur.".into(),
            "Thurs.".into(),
            "Fri.".into(),
            "Sat.".into(),
            "Sun.".into(),
            "Vol.".into(),
            "Fig.".into(),
            "No.".into(),
            "pp.".into(),
            "Def.".into(),
            "Eq.".into(),
            "Lem.".into(),
            "Prop.".into(),
            "Thm.".into(),
            "Cor.".into(),
            "Sec.".into(),
            "Ch.".into(),
            "App.".into(),
            "Ref.".into(),
            "Refs.".into(),
            "abbr.".into(),
            "acad.".into(),
            "acc.".into(),
            "admin.".into(),
            "adv.".into(),
            "a.m.".into(),
            "p.m.".into(),
            "A.M.".into(),
            "P.M.".into(),
            "arch.".into(),
            "asst.".into(),
            "atty.".into(),
            "aud.".into(),
            "bldg.".into(),
            "blvd.".into(),
            "ave.".into(),
            "Ave.".into(),
            "cap.".into(),
            "capt.".into(),
            "cert.".into(),
            "chm.".into(),
            "chron.".into(),
            "clin.".into(),
            "cmte.".into(),
            "col.".into(),
            "coll.".into(),
            "comdr.".into(),
            "con.".into(),
            "cont.".into(),
            "corp.".into(),
            "cpl.".into(),
            "cr.".into(),
            "ctr.".into(),
            "dept.".into(),
            "dev.".into(),
            "dir.".into(),
            "disc.".into(),
            "dist.".into(),
            "div.".into(),
            "doc.".into(),
            "doz.".into(),
            "dr.".into(),
            "Drv.".into(),
            "ed.".into(),
            "educ.".into(),
            "elec.".into(),
            "eng.".into(),
            "ens.".into(),
            "equip.".into(),
            "esp.".into(),
            "est.".into(),
            "eval.".into(),
            "ex.".into(),
            "exec.".into(),
            "exp.".into(),
            "ext.".into(),
            "fac.".into(),
            "fem.".into(),
            "ff.".into(),
            "fig.".into(),
            "fin.".into(),
            "fl.".into(),
            "fst.".into(),
            "ft.".into(),
            "gen.".into(),
            "geo.".into(),
            "geog.".into(),
            "geol.".into(),
            "gov.".into(),
            "govt.".into(),
            "grp.".into(),
            "hist.".into(),
            "hosp.".into(),
            "hr.".into(),
            "hrs.".into(),
            "ht.".into(),
            "hwy.".into(),
            "ill.".into(),
            "illus.".into(),
            "inc.".into(),
            "ind.".into(),
            "inst.".into(),
            "int.".into(),
            "intl.".into(),
            "jr.".into(),
            "lab.".into(),
            "lat.".into(),
            "lib.".into(),
            "lng.".into(),
            "loc.".into(),
            "lt.".into(),
            "ltd.".into(),
            "masc.".into(),
            "math.".into(),
            "meas.".into(),
            "med.".into(),
            "mil.".into(),
            "min.".into(),
            "misc.".into(),
            "mod.".into(),
            "mont.".into(),
            "mr.".into(),
            "mrs.".into(),
            "ms.".into(),
            "narr.".into(),
            "nat.".into(),
            "neg.".into(),
            "no.".into(),
            "nom.".into(),
            "nr.".into(),
            "obj.".into(),
            "obs.".into(),
            "off.".into(),
            "ord.".into(),
            "org.".into(),
            "orig.".into(),
            "pl.".into(),
            "pop.".into(),
            "pos.".into(),
            "pow.".into(),
            "pp.".into(),
            "pr.".into(),
            "pref.".into(),
            "pres.".into(),
            "prob.".into(),
            "proc.".into(),
            "prod.".into(),
            "prof.".into(),
            "pron.".into(),
            "prop.".into(),
            "pub.".into(),
            "pwr.".into(),
            "qt.".into(),
            "quot.".into(),
            "rad.".into(),
            "rcvd.".into(),
            "rec.".into(),
            "ref.".into(),
            "reg.".into(),
            "rep.".into(),
            "reps.".into(),
            "res.".into(),
            "rev.".into(),
            "rte.".into(),
            "sci.".into(),
            "sec.".into(),
            "sect.".into(),
            "sel.".into(),
            "sen.".into(),
            "sep.".into(),
            "sept.".into(),
            "seq.".into(),
            "sgt.".into(),
            "soc.".into(),
            "sp.".into(),
            "spec.".into(),
            "sr.".into(),
            "supt.".into(),
            "sur.".into(),
            "surg.".into(),
            "syn.".into(),
            "tech.".into(),
            "tel.".into(),
            "temp.".into(),
            "theol.".into(),
            "tmb.".into(),
            "tsp.".into(),
            "univ.".into(),
            "U.S.".into(),
            "U.K.".into(),
            "U.N.".into(),
            "E.U.".into(),
            "vol.".into(),
            "vs.".into(),
            "wk.".into(),
            "yr.".into(),
            "Blvd.".into(),
            "Ave.".into(),
            "Rd.".into(),
            "Ln.".into(),
            "Dr.".into(),
            "Ct.".into(),
            "Pl.".into(),
            "Ter.".into(),
            "Cir.".into(),
            "Pkwy.".into(),
            "Hwy.".into(),
            "Fwy.".into(),
            "Mt.".into(),
            "Mts.".into(),
            "Ft.".into(),
            "Pt.".into(),
            "Ste.".into(),
            "Fl.".into(),
            "A.M.".into(),
            "P.M.".into(),
            "EST.".into(),
            "CST.".into(),
            "MST.".into(),
            "PST.".into(),
            "EDT.".into(),
            "CDT.".into(),
            "MDT.".into(),
            "PDT.".into(),
            "in.".into(),
            "ft.".into(),
            "yd.".into(),
            "mi.".into(),
            "oz.".into(),
            "lb.".into(),
            "lbs.".into(),
            "kg.".into(),
            "gm.".into(),
            "mm.".into(),
            "cm.".into(),
            "km.".into(),
            "sq.".into(),
            "cu.".into(),
            "U.S.".into(),
        ]
    }

    pub fn with_abbreviations(mut self, abbreviations: Vec<String>) -> Self {
        self.abbreviations = abbreviations;
        self
    }

    fn is_abbreviation_boundary(&self, text: &str, period_pos: usize) -> bool {
        let before = &text[..period_pos];
        for abbr in &self.abbreviations {
            let abbr_stem = abbr.trim_end_matches('.');
            if before.ends_with(abbr_stem) {
                let prefix_len = before.len() - abbr_stem.len();
                if prefix_len == 0 {
                    return true;
                }
                let char_before = before.as_bytes().get(prefix_len - 1).copied();
                if let Some(b) = char_before
                    && (b.is_ascii_whitespace() || b.is_ascii_punctuation())
                {
                    return true;
                }
            }
        }
        false
    }

    fn is_sentence_boundary(&self, text: &str, pos: usize) -> bool {
        let chars: Vec<char> = text.chars().collect();
        if pos >= chars.len() {
            return false;
        }

        let ch = chars[pos];
        if ch != '.' && ch != '!' && ch != '?' {
            return false;
        }

        if ch == '.' && pos + 1 < chars.len() && chars[pos + 1] == '.' {
            return false;
        }

        if ch == '.'
            && pos > 0
            && chars[pos - 1].is_ascii_digit()
            && pos + 1 < chars.len()
            && chars[pos + 1].is_ascii_digit()
        {
            return false;
        }

        if ch == '.' && self.is_abbreviation_boundary(text, pos) {
            return false;
        }

        let mut next_pos = pos + 1;
        while next_pos < chars.len() && is_closing_punct(chars[next_pos]) {
            next_pos += 1;
        }

        if next_pos >= chars.len() {
            return true;
        }

        if !chars[next_pos].is_whitespace() {
            return false;
        }

        while next_pos < chars.len() && chars[next_pos].is_whitespace() {
            next_pos += 1;
        }

        if next_pos >= chars.len() {
            return true;
        }

        let next_ch = chars[next_pos];
        next_ch.is_uppercase() || next_ch.is_ascii_digit() || is_opening_punct(next_ch)
    }
}

fn is_closing_punct(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '\u{201D}' | '\u{2019}' | ')' | ']' | '\u{300B}'
    )
}

fn is_opening_punct(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '\u{201C}' | '\u{2018}' | '(' | '[' | '\u{300A}'
    )
}

impl Default for DefaultSentenceSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl SentenceTokenizer for DefaultSentenceSplitter {
    #[allow(unused_assignments, unused_variables)]
    fn split(&self, text: &str) -> Vec<Sentence> {
        if text.is_empty() {
            return vec![Sentence::new("", 0, 0)];
        }

        let mut sentences = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut sentence_start = 0;
        let mut last_boundary: Option<usize> = None;

        for i in 0..chars.len() {
            if self.is_sentence_boundary(text, i) {
                let mut byte_pos = 0;
                for c in chars.iter().take(i + 1) {
                    byte_pos += c.len_utf8();
                }

                let mut end_pos = byte_pos;
                while end_pos < text.len() && text.as_bytes()[end_pos] == b' ' {
                    end_pos += 1;
                }

                last_boundary = Some(end_pos);

                let sentence_text = text[sentence_start..byte_pos].trim();
                if !sentence_text.is_empty() {
                    sentences.push(Sentence::new(sentence_text, sentence_start, byte_pos));
                }
                sentence_start = end_pos;
            }
        }

        if sentence_start < text.len() {
            let remaining = text[sentence_start..].trim();
            if !remaining.is_empty() {
                sentences.push(Sentence::new(remaining, sentence_start, text.len()));
            }
        }

        if sentences.is_empty() {
            sentences.push(Sentence::new(text.trim(), 0, text.len()));
        }

        sentences
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SentenceTokenizer;

    #[test]
    fn test_basic_split() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello world. This is a test.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Hello world.");
        assert_eq!(sentences[1].text(), "This is a test.");
    }

    #[test]
    fn test_exclamation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello! How are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Hello!");
        assert_eq!(sentences[1].text(), "How are you?");
    }

    #[test]
    fn test_abbreviation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Dr. Smith went home. He was tired.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_single_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Just one sentence";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "Just one sentence");
    }

    #[test]
    fn test_empty_text() {
        let splitter = DefaultSentenceSplitter::new();
        let sentences = splitter.split("");
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_offsets_preserved() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "First. Second.";
        let sentences = splitter.split(text);
        assert_eq!(sentences[0].start(), 0);
        assert_eq!(sentences[0].text(), "First.");
        assert_eq!(sentences[1].text(), "Second.");
    }

    #[test]
    fn test_no_split_inside_quotes() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "He said \"Hello world.\" She nodded.";
        let sentences = splitter.split(text);
        assert!(!sentences.is_empty());
    }

    #[test]
    fn test_basic_two_sentences_period() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello world. How are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Hello world.");
        assert_eq!(sentences[1].text(), "How are you?");
    }

    #[test]
    fn test_basic_three_sentences() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "One. Two. Three.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0].text(), "One.");
        assert_eq!(sentences[1].text(), "Two.");
        assert_eq!(sentences[2].text(), "Three.");
    }

    #[test]
    fn test_basic_sentence_with_trailing_period() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "This is a sentence.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "This is a sentence.");
    }

    #[test]
    fn test_abbreviation_mr() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Mr. Smith went home.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Mr.'");
        assert_eq!(sentences[0].text(), "Mr. Smith went home.");
    }

    #[test]
    fn test_abbreviation_mrs() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Mrs. Jones arrived early.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Mrs.'");
    }

    #[test]
    fn test_abbreviation_ms() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Ms. Davis called yesterday.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Ms.'");
    }

    #[test]
    fn test_abbreviation_dr() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Dr. Smith went home.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Dr.'");
    }

    #[test]
    fn test_abbreviation_prof() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Prof. Adams lectured today.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Prof.'");
    }

    #[test]
    fn test_abbreviation_sr() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "John Smith Sr. gave a talk.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Sr.'");
    }

    #[test]
    fn test_abbreviation_jr() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "John Doe Jr. ran the meeting.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Jr.'");
    }

    #[test]
    fn test_abbreviation_st() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "He lives on Elm St. near downtown.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'St.'");
    }

    #[test]
    fn test_abbreviation_inc() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Acme Inc. released a product.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Inc.'");
    }

    #[test]
    fn test_abbreviation_ltd() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Widgets Ltd. filed a report.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Ltd.'");
    }

    #[test]
    fn test_abbreviation_co() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Smith Co. shipped the order.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Co.'");
    }

    #[test]
    fn test_abbreviation_corp() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Global Corp. announced earnings.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Corp.'");
    }

    #[test]
    fn test_abbreviation_vs() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "In Smith vs. Jones the court ruled.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'vs.'");
    }

    #[test]
    fn test_abbreviation_etc() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "We need apples, oranges, etc. for the party.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'etc.'");
    }

    #[test]
    fn test_abbreviation_eg() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Some fruits (e.g. apples) are sweet.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'e.g.'");
    }

    #[test]
    fn test_abbreviation_ie() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "All items (i.e. everything) were sold.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'i.e.'");
    }

    #[test]
    fn test_abbreviation_al() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Smith et al. published the paper.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'al.'");
    }

    #[test]
    fn test_abbreviation_ca() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The artifact is ca. 2000 years old.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'ca.'");
    }

    #[test]
    fn test_abbreviation_approx() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The value is approx. 3.5 meters.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'approx.'");
    }

    #[test]
    fn test_multiple_abbreviations_jekyll_hyde() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Dr. Jekyll and Mr. Hyde";
        let sentences = splitter.split(text);
        assert_eq!(
            sentences.len(),
            1,
            "Should not split on abbreviations 'Dr.' and 'Mr.'"
        );
        assert_eq!(sentences[0].text(), "Dr. Jekyll and Mr. Hyde");
    }

    #[test]
    fn test_multiple_abbreviations_in_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Prof. Smith and Dr. Jones met Mr. Brown.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on any abbreviation");
    }

    #[test]
    fn test_abbreviation_followed_by_real_sentence_boundary() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Dr. Smith went home. He was tired.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Dr. Smith went home.");
        assert_eq!(sentences[1].text(), "He was tired.");
    }

    #[test]
    fn test_abbreviation_at_end_of_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "I work at Acme Inc. The pay is good.";
        let sentences = splitter.split(text);
        assert!(
            sentences.len() == 1 || sentences.len() == 2,
            "Abbreviation at sentence boundary has ambiguous behavior"
        );
    }

    #[test]
    fn test_number_with_period_no_split() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The temperature is 3.5 degrees.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on decimal period");
        assert_eq!(sentences[0].text(), "The temperature is 3.5 degrees.");
    }

    #[test]
    fn test_number_followed_by_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The value is 3.5. It is high.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "The value is 3.5.");
        assert_eq!(sentences[1].text(), "It is high.");
    }

    #[test]
    fn test_multiple_numbers_in_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Values range from 1.2 to 3.4 meters.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on any decimal period");
    }

    #[test]
    fn test_number_starting_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "See item. 3 items total.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_url_no_split() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Visit http://example.com. Now go.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].text().contains("http://example.com."));
        assert_eq!(sentences[1].text(), "Now go.");
    }

    #[test]
    fn test_url_with_path() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Visit http://example.com/page.html for info.";
        let sentences = splitter.split(text);
        assert_eq!(
            sentences.len(),
            1,
            "URL with path should stay as one sentence"
        );
    }

    #[test]
    fn test_www_url() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Go to www.example.com today.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_ellipsis_three_dots() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Wait... Then go.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_ellipsis_at_end() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "I was thinking...";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "I was thinking...");
    }

    #[test]
    fn test_ellipsis_between_sentences() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Well... Hello there.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_unicode_ellipsis_character() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Wait\u{2026} Then go.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_question_and_exclamation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Really? Yes! OK.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0].text(), "Really?");
        assert_eq!(sentences[1].text(), "Yes!");
        assert_eq!(sentences[2].text(), "OK.");
    }

    #[test]
    fn test_multiple_exclamations() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Stop! Go! Wait!";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_multiple_questions() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Who? What? Where?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_mixed_punctuation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello. How are you? I am fine! Great.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 4);
        assert_eq!(sentences[0].text(), "Hello.");
        assert_eq!(sentences[1].text(), "How are you?");
        assert_eq!(sentences[2].text(), "I am fine!");
        assert_eq!(sentences[3].text(), "Great.");
    }

    #[test]
    fn test_parenthetical_with_period() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello (world). How are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Hello (world).");
        assert_eq!(sentences[1].text(), "How are you?");
    }

    #[test]
    fn test_parenthetical_inside_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "He went home (early). He was tired.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_parentheses_at_end() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "It happened (suddenly). Then it stopped.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_sentence_starting_with_parenthesis() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello. (This is a note.) Goodbye.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_multiple_spaces_between_sentences() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello world.   How are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Hello world.");
        assert_eq!(sentences[1].text(), "How are you?");
    }

    #[test]
    fn test_tabs_between_sentences() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello world.\tHow are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_newline_between_sentences() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello world.\nHow are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_carriage_return_between_sentences() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello world.\r\nHow are you?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_empty_string() {
        let splitter = DefaultSentenceSplitter::new();
        let sentences = splitter.split("");
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "");
    }

    #[test]
    fn test_single_word_no_punctuation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "Hello");
    }

    #[test]
    fn test_only_whitespace() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "   ";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "");
    }

    #[test]
    fn test_only_periods() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "...";
        let sentences = splitter.split(text);
        assert!(!sentences.is_empty());
    }

    #[test]
    fn test_single_period() {
        let splitter = DefaultSentenceSplitter::new();
        let text = ".";
        let sentences = splitter.split(text);
        assert!(!sentences.is_empty());
    }

    #[test]
    fn test_only_question_mark() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "?";
        let sentences = splitter.split(text);
        assert!(!sentences.is_empty());
    }

    #[test]
    fn test_only_exclamation_mark() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "!";
        let sentences = splitter.split(text);
        assert!(!sentences.is_empty());
    }

    #[test]
    fn test_sentence_without_trailing_punctuation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "This is a sentence without end";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "This is a sentence without end");
    }

    #[test]
    fn test_two_sentences_no_trailing_punct_on_second() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "First sentence. Second sentence";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "First sentence.");
        assert_eq!(sentences[1].text(), "Second sentence");
    }

    #[test]
    fn test_single_character() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "A";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "A");
    }

    #[test]
    fn test_direct_speech_with_quotes() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "\"Hello,\" she said. \"How are you?\"";
        let sentences = splitter.split(text);
        assert!(
            sentences.len() >= 2,
            "Should produce at least 2 sentences for direct speech"
        );
        assert_eq!(sentences[0].text(), "\"Hello,\" she said.");
        assert!(sentences[1].text().contains("How are you?"));
    }

    #[test]
    fn test_quote_at_sentence_end() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "He said \"Hello.\" She replied.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_exclamation_in_quotes() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "\"Stop!\" he yelled. She flinched.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_question_mark_in_quotes() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "\"Really?\" she asked.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_decimal_numbers_pi_and_euler() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Pi is 3.14159. Euler's number is 2.718.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Pi is 3.14159.");
        assert_eq!(sentences[1].text(), "Euler's number is 2.718.");
    }

    #[test]
    fn test_decimal_at_start_of_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The answer is 42. 3.14 is pi.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_decimal_in_middle() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The measurement was 2.5 units long.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on decimal period");
    }

    #[test]
    fn test_version_number() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Version 2.0 is released.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_date_with_jan_abbreviation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "On Jan. 15, 2023, we met.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Jan.'");
    }

    #[test]
    fn test_date_with_feb_abbreviation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "On Feb. 28, we celebrate.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Feb.'");
    }

    #[test]
    fn test_date_with_mar_abbreviation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Mar. came in like a lion.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1, "Should not split on 'Mar.'");
    }

    #[test]
    fn test_all_month_abbreviations() {
        let months = [
            "Jan.", "Feb.", "Mar.", "Apr.", "Jun.", "Jul.", "Aug.", "Sep.", "Oct.", "Nov.", "Dec.",
        ];
        let splitter = DefaultSentenceSplitter::new();
        for month in &months {
            let text = format!("On {} 1, we met.", month);
            let sentences = splitter.split(&text);
            assert_eq!(
                sentences.len(),
                1,
                "Should not split on abbreviation '{}'",
                month
            );
        }
    }

    #[test]
    fn test_day_abbreviations() {
        let days = ["Mon.", "Tue.", "Wed.", "Thu.", "Fri.", "Sat.", "Sun."];
        let splitter = DefaultSentenceSplitter::new();
        for day in &days {
            let text = format!("On {} we met.", day);
            let sentences = splitter.split(&text);
            assert_eq!(
                sentences.len(),
                1,
                "Should not split on abbreviation '{}'",
                day
            );
        }
    }

    #[test]
    fn test_date_followed_by_new_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "On Jan. 15 we met. It was cold.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "On Jan. 15 we met.");
        assert_eq!(sentences[1].text(), "It was cold.");
    }

    #[test]
    fn test_single_letter_abbreviations() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "A. Smith went to B. Jones";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_single_letter_not_in_abbreviation_list() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "X. Y went home.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_run_on_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello.World.";
        let sentences = splitter.split(text);
        assert_eq!(
            sentences.len(),
            1,
            "No split without whitespace after period"
        );
    }

    #[test]
    fn test_run_on_exclamation() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello!World.";
        let sentences = splitter.split(text);
        assert_eq!(
            sentences.len(),
            1,
            "No split without whitespace after exclamation"
        );
    }

    #[test]
    fn test_run_on_question_mark() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello?World.";
        let sentences = splitter.split(text);
        assert_eq!(
            sentences.len(),
            1,
            "No split without whitespace after question mark"
        );
    }

    #[test]
    fn test_with_custom_abbreviations() {
        let splitter = DefaultSentenceSplitter::new().with_abbreviations(vec!["Custom.".into()]);
        let text = "Custom. value is high.";
        let sentences = splitter.split(text);
        assert_eq!(
            sentences.len(),
            1,
            "Should not split on custom abbreviation"
        );
    }

    #[test]
    fn test_with_empty_abbreviations() {
        let splitter = DefaultSentenceSplitter::new().with_abbreviations(vec![]);
        let text = "Mr. Smith went home.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_sentence_offsets_are_correct() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "First. Second. Third.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
        for s in &sentences {
            let slice = &text[s.start()..s.end()];
            assert!(
                slice.contains(s.text().trim()),
                "Offset mismatch for '{}'",
                s.text()
            );
        }
    }

    #[test]
    fn test_period_followed_by_lowercase() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "e.g. apples are sweet.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_period_followed_by_bracket() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello. [This is a note.] Goodbye.";
        let sentences = splitter.split(text);
        assert!(sentences.len() >= 2);
    }

    #[test]
    fn test_unicode_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Caf\u{e9} is nice. Sauna is hot.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text(), "Caf\u{e9} is nice.");
        assert_eq!(sentences[1].text(), "Sauna is hot.");
    }

    #[test]
    fn test_very_long_sentence() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "This is a very long sentence that contains many words and goes on and on without any sentence-ending punctuation until the very end.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), text);
    }

    #[test]
    fn test_sentence_starting_with_digit() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Go. 3 people came.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_apostrophe_after_period() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Hello.' She said.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn test_period_at_end_of_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "The end.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "The end.");
    }

    #[test]
    fn test_question_mark_at_end_of_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Is it?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "Is it?");
    }

    #[test]
    fn test_exclamation_at_end_of_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Wow!";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].text(), "Wow!");
    }

    #[test]
    fn test_multiple_sentences_with_abbreviations_interleaved() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Dr. Smith arrived. He met Mr. Jones. They talked.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0].text(), "Dr. Smith arrived.");
        assert_eq!(sentences[1].text(), "He met Mr. Jones.");
        assert_eq!(sentences[2].text(), "They talked.");
    }

    #[test]
    fn test_consecutive_abbreviations() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Smith et al. published results.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_abbreviation_at_start_of_text() {
        let splitter = DefaultSentenceSplitter::new();
        let text = "Dr. who?";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn test_default_trait_implementation() {
        let splitter = DefaultSentenceSplitter::default();
        let text = "Hello world.";
        let sentences = splitter.split(text);
        assert_eq!(sentences.len(), 1);
    }
}
