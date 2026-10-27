use og_core::{AnalyzedToken, AnalyzedTokenReadings};

pub struct ChineseTagger;

impl ChineseTagger {
    pub fn new() -> Self {
        Self
    }

    pub fn as_analyzed_token(word: &str) -> AnalyzedToken {
        if !word.contains('/') {
            return AnalyzedToken::new(" ", 0, 1);
        }
        let parts: Vec<&str> = word.split('/').collect();
        if parts[0].is_empty() && parts[parts.len() - 1] == "w" {
            let cut = word.len().saturating_sub(2);
            let text = &word[..cut];
            let pos = &word[cut + 1..];
            let mut token = AnalyzedToken::new(text, 0, text.len());
            if !pos.is_empty() {
                token = token.with_pos_tags(vec![pos.to_string()]);
            }
            return token;
        }
        let mut token = AnalyzedToken::new(parts[0], 0, parts[0].len());
        if !parts[1].is_empty() {
            token = token.with_pos_tags(vec![parts[1].to_string()]);
        }
        token
    }

    pub fn tag_token_strings(&self, tokens: &[String]) -> Vec<AnalyzedTokenReadings> {
        let mut readings = Vec::with_capacity(tokens.len());
        let mut pos = 0usize;
        for word in tokens {
            let at = Self::as_analyzed_token(word);
            let len = at.token().len();
            let mut adjusted = AnalyzedToken::new(at.token(), pos, pos + len);
            if !at.pos_tags().is_empty() {
                adjusted = adjusted.with_pos_tags(at.pos_tags().to_vec());
            }
            readings.push(AnalyzedTokenReadings::new(adjusted));
            pos += len;
        }
        readings
    }
}

impl Default for ChineseTagger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zh::word_tokenizer::ChineseWordTokenizer;

    fn my_assert(text: &str, expected: &str) {
        let tokenizer = ChineseWordTokenizer::new();
        let tagger = ChineseTagger::new();
        let terms = tokenizer.tokenize_java_format(text);
        let readings = tagger.tag_token_strings(&terms);
        let rendered: Vec<String> = readings
            .iter()
            .map(|r| {
                let t = r.token();
                format!(
                    "{}/[{}]{}",
                    t.token(),
                    t.lemma().unwrap_or("null"),
                    t.pos_tags().first().map(String::as_str).unwrap_or("null")
                )
            })
            .collect();
        assert_eq!(rendered.join(" -- "), expected);
    }

    #[test]
    fn java_pin_tagger_test_sentence_one() {
        my_assert(
            "主任强调指出错误的地方。",
            "主任/[null]n -- 强调/[null]v -- 指出/[null]v -- 错误/[null]n -- 的/[null]uj -- \
             地方/[null]n -- 。/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_two() {
        my_assert(
            "她胸前挂着一块碧绿的玉。",
            "她/[null]r -- 胸前/[null]s -- 挂/[null]v -- 着/[null]uz -- 一块/[null]s -- \
             碧绿/[null]z -- 的/[null]uj -- 玉/[null]n -- 。/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_three() {
        my_assert(
            "“鲯鳅”的研究结果有什么奥妙？",
            "“/[null]w -- 鲯/[null]n -- 鳅/[null]x -- ”/[null]w -- 的/[null]uj -- \
             研究/[null]vn -- 结果/[null]n -- 有/[null]v -- 什么/[null]r -- 奥妙/[null]n -- \
             ？/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_four() {
        my_assert(
            "我们的女组长真是尺竿头更进一步。",
            "我们/[null]r -- 的/[null]uj -- 女/[null]b -- 组长/[null]n -- 真是/[null]d -- \
             尺/[null]q -- 竿/[null]ng -- 头/[null]n -- 更进一步/[null]l -- 。/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_five() {
        my_assert(
            "国务院，非国家工作人员不能随便进去的地方。",
            "国务院/[null]nt -- ，/[null]w -- 非/[null]h -- 国家/[null]n -- 工作/[null]vn -- \
             人员/[null]n -- 不能/[null]v -- 随便/[null]ad -- 进去/[null]v -- 的/[null]uj -- \
             地方/[null]n -- 。/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_six() {
        my_assert(
            "“哇……”珠海北师大操场上的师生大吃一惊！",
            "“/[null]w -- 哇/[null]o -- ……/[null]w -- ”/[null]w -- 珠海/[null]ns -- \
             北师大/[null]j -- 操场/[null]n -- 上/[null]f -- 的/[null]uj -- 师生/[null]n -- \
             大吃一惊/[null]l -- ！/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_seven() {
        my_assert(
            "在炎热的暑假里，我和其他同学们参加了姜老师的一个项目。",
            "在/[null]p -- 炎热/[null]a -- 的/[null]uj -- 暑假/[null]t -- 里/[null]f -- \
             ，/[null]w -- 我/[null]r -- 和/[null]c -- 其他/[null]r -- 同学/[null]n -- \
             们/[null]k -- 参加/[null]v -- 了/[null]ul -- 姜/[null]n -- 老师/[null]n -- \
             的/[null]uj -- 一个/[null]mq -- 项目/[null]n -- 。/[null]w",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_eight() {
        my_assert(
            "“咕咚，”一台联想ThinkPad T系列电脑从关羽的宿舍飞了下来。",
            "“/[null]w -- 咕咚/[null]o -- ，/[null]w -- ”/[null]w -- 一/[null]m -- 台/[null]q \
             -- 联想/[null]nz -- ThinkPad/[null]nx --  /[null]w -- T/[null]nx -- 系列/[null]q \
             -- 电脑/[null]n -- 从/[null]p -- 关/[null]v -- 羽/[null]q -- 的/[null]uj -- \
             宿舍/[null]n -- 飞/[null]v -- 了/[null]ul -- 下来/[null]v -- 。/[null]w",
        );
    }

    #[test]
    fn space_only_terms_decode_via_the_java_special_case() {
        let at = ChineseTagger::as_analyzed_token("notelement");
        assert_eq!(at.token(), " ");
        assert!(at.pos_tags().is_empty());
        assert_eq!(at.lemma(), None);
        let at = ChineseTagger::as_analyzed_token("/w");
        assert_eq!(at.token(), "");
        assert_eq!(at.pos_tags(), ["w".to_string()].as_slice());
        let readings = ChineseTagger::new().tag_token_strings(&["x/y".to_string()]);
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].token().token(), "x");
        assert_eq!(readings[0].token().pos_tags(), ["y".to_string()].as_slice());
    }
}
