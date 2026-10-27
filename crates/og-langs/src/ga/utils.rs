pub fn is_upper_vowel(c: char) -> bool {
    matches!(
        c,
        'A' | 'E'
            | 'I'
            | 'O'
            | 'U'
            | '\u{00C1}'
            | '\u{00C9}'
            | '\u{00CD}'
            | '\u{00D3}'
            | '\u{00DA}'
    )
}

pub fn is_lower_vowel(c: char) -> bool {
    matches!(
        c,
        'a' | 'e'
            | 'i'
            | 'o'
            | 'u'
            | '\u{00E1}'
            | '\u{00E9}'
            | '\u{00ED}'
            | '\u{00F3}'
            | '\u{00FA}'
    )
}

pub fn is_vowel(c: char) -> bool {
    is_lower_vowel(c) || is_upper_vowel(c)
}

pub fn is_upper_lenitable(c: char) -> bool {
    matches!(c, 'B' | 'C' | 'D' | 'F' | 'G' | 'M' | 'P' | 'S' | 'T')
}

pub fn is_lower_lenitable(c: char) -> bool {
    matches!(c, 'b' | 'c' | 'd' | 'f' | 'g' | 'm' | 'p' | 's' | 't')
}

pub fn is_s_lenitable(c: char) -> bool {
    matches!(
        c,
        'l' | 'n'
            | 'r'
            | 'a'
            | 'e'
            | 'i'
            | 'o'
            | 'u'
            | '\u{00E1}'
            | '\u{00E9}'
            | '\u{00ED}'
            | '\u{00F3}'
            | '\u{00FA}'
    )
}

pub fn lenite(input: &str) -> String {
    let mut chars = input.chars();
    let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
        return input.to_string();
    };
    // Java tests Character.isUpperCase(1) ('1' is never uppercase) so the
    // inserted char is always lowercase 'h' (Utils.java:367).
    let lenitable = is_lower_lenitable(first) || is_upper_lenitable(first);
    let is_s = first == 'S' || first == 's';
    let s_ok = second.to_lowercase().next().is_some_and(is_s_lenitable);
    if !lenitable || is_s && !s_ok {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 1);
    out.push(first);
    out.push('h');
    out.push(second);
    out.push_str(chars.as_str());
    out
}

pub fn eclipse(input: &str) -> String {
    let Some(first) = input.chars().next() else {
        return input.to_string();
    };
    if is_upper_vowel(first) {
        return format!("n{input}");
    }
    if is_lower_vowel(first) {
        return format!("n-{input}");
    }
    match first.to_lowercase().next() {
        Some('b') => format!("m{input}"),
        Some('c') => format!("g{input}"),
        Some('d') | Some('g') => format!("n{input}"),
        Some('f') => format!("bh{input}"),
        Some('p') => format!("b{input}"),
        Some('t') => format!("d{input}"),
        _ => input.to_string(),
    }
}

pub fn starts_with_mutated_uppercase(s: &str) -> bool {
    if ["mB", "gC", "nD", "bhF", "nG", "bP", "dT", "tS"]
        .iter()
        .any(|prefix| s.starts_with(prefix))
    {
        return true;
    }
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some('n' | 't' | 'h'), Some(vowel)) => is_upper_vowel(vowel),
        _ => false,
    }
}

pub fn starts_with_uppercase(s: &str) -> bool {
    starts_with_mutated_uppercase(s) || s.chars().next().is_some_and(char::is_uppercase)
}

pub fn is_all_uppercase(s: &str) -> bool {
    let mut start_from = 0usize;
    if starts_with_mutated_uppercase(s) {
        start_from = if s.starts_with("bhF") { 2 } else { 1 };
    }
    s.chars()
        .skip(start_from)
        .all(|c| !c.is_alphabetic() || c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenite_inserts_the_quirk_lowercase_h() {
        assert_eq!(lenite("cat"), "chat");
        assert_eq!(lenite("Poblacht"), "Phoblacht");
        assert_eq!(lenite("slán"), "shlán");
        assert_eq!(
            lenite("srón"),
            "shrón",
            "s before r lenites (Utils.java:331)"
        );
        assert_eq!(lenite("Sliabh"), "Shliabh");
        assert_eq!(lenite("scéal"), "scéal", "s before c does not lenite");
        assert_eq!(lenite("Sráid"), "Shráid", "s before r lenites");
        assert_eq!(lenite("áit"), "áit");
        assert_eq!(lenite("b"), "b");
    }

    #[test]
    fn eclipse_covers_vowels_and_consonants() {
        assert_eq!(eclipse("Éirinn"), "nÉirinn");
        assert_eq!(eclipse("éin"), "n-éin");
        assert_eq!(eclipse("bean"), "mbean");
        assert_eq!(eclipse("carr"), "gcarr");
        assert_eq!(eclipse("doras"), "ndoras");
        assert_eq!(eclipse("geata"), "ngeata");
        assert_eq!(eclipse("focal"), "bhfocal");
        assert_eq!(eclipse("peann"), "bpeann");
        assert_eq!(eclipse("túr"), "dtúr");
        assert_eq!(eclipse("sliabh"), "sliabh");
        assert_eq!(eclipse(""), "");
    }

    #[test]
    fn mutated_uppercase_prefixes_count_as_uppercase() {
        assert!(starts_with_uppercase("mBreatain"));
        assert!(starts_with_uppercase("gCarr"));
        assert!(starts_with_uppercase("bhFocal"));
        assert!(starts_with_uppercase("nÉirinn"));
        assert!(starts_with_uppercase("tSeomra"));
        assert!(starts_with_uppercase("Carr"));
        assert!(!starts_with_uppercase("carr"));
        assert!(is_all_uppercase("GCARR"));
        assert!(is_all_uppercase("bhFOCAL"));
        assert!(!is_all_uppercase("bhFocal"));
        assert!(is_all_uppercase("123"));
    }
}
