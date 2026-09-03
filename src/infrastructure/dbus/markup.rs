//! Infrastructure/dbus — remove markup, preserva `<` literal e decodifica entidades.
pub fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            // sem fechamento → literal
            let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == '>') else {
                out.push(chars[i]);
                i += 1;
                continue;
            };
            let end = i + 1 + rel_end;
            // conteúdo entre < e >
            let inner: String = chars[i + 1..end].iter().collect();
            // tag válida não começa com espaço e não contém '<' aninhado
            let starts_with_space = chars.get(i + 1).is_some_and(|c| c.is_whitespace());
            let has_nested_lt = inner.contains('<');
            let trimmed = inner.trim();
            let looks_like_tag = !starts_with_space
                && !has_nested_lt
                && !trimmed.is_empty()
                && trimmed
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?');
            if looks_like_tag {
                // pula tag inteira
                i = end + 1;
                continue;
            } else {
                // não é tag → '<' literal
                out.push('<');
                i += 1;
                continue;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    // Ordem importa: &amp; por último para não double-decode &lt;
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
}
