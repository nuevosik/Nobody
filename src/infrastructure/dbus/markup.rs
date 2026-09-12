pub fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut copied_until = 0;
    let mut tag_start = None;
    let mut first_after_lt = false;
    let mut starts_with_space = false;
    let mut first_trimmed = None;

    for (idx, ch) in s.char_indices() {
        let Some(start) = tag_start else {
            if ch == '<' {
                tag_start = Some(idx);
                first_after_lt = true;
                starts_with_space = false;
                first_trimmed = None;
            }
            continue;
        };

        if ch == '<' {
            out.push_str(&s[copied_until..idx]);
            copied_until = idx;
            tag_start = Some(idx);
            first_after_lt = true;
            starts_with_space = false;
            first_trimmed = None;
            continue;
        }

        if first_after_lt {
            starts_with_space = ch.is_whitespace();
            first_after_lt = false;
        }
        if first_trimmed.is_none() && !ch.is_whitespace() {
            first_trimmed = Some(ch);
        }
        if ch != '>' {
            continue;
        }

        let looks_like_tag = !starts_with_space
            && first_trimmed
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?');
        if looks_like_tag {
            out.push_str(&s[copied_until..start]);
        } else {
            out.push_str(&s[copied_until..idx + 1]);
        }
        copied_until = idx + 1;
        tag_start = None;
    }
    out.push_str(&s[copied_until..]);

    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_markup_basic() {
        assert_eq!(strip_markup("<b>oi</b> &amp; ola"), "oi & ola");
    }

    #[test]
    fn preserves_unmatched_openers() {
        let input = "<".repeat(1024);
        assert_eq!(strip_markup(&input), input);
    }

    #[test]
    fn nested_openers_are_processed_left_to_right() {
        assert_eq!(strip_markup("<<b>tail"), "<tail");
    }

    #[test]
    fn preserves_tags_that_start_with_whitespace() {
        assert_eq!(strip_markup("a < b> c <\tb>"), "a < b> c <\tb>");
    }

    #[test]
    fn strips_valid_tags_and_decodes_supported_entities() {
        assert_eq!(
            strip_markup("<i>one</i> &lt; &gt; &quot;x&quot; &apos; &#39; &#x27; &#34; &amp;"),
            "one < > \"x\" ' ' ' \" &"
        );
    }
}
