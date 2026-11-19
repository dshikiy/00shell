//! Splits an input line into words, honouring quotes and backslash escapes.

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// A quote was opened but the line ended before it was closed.
    UnterminatedQuote,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

/// Tokenizes `line` the way a POSIX shell would split words:
///
/// * unquoted whitespace separates words;
/// * `'...'` keeps everything literally;
/// * `"..."` keeps everything except that `\"`, `\\`, `\$` and `` \` `` are unescaped;
/// * outside quotes a backslash escapes the next character;
/// * quotes can be glued to other text (`a"b c"d` is the single word `ab cd`),
///   and `""` yields an empty word.
pub fn tokenize(line: &str) -> Result<Vec<String>, ParseError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote = Quote::None;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match quote {
            Quote::Single => {
                if c == '\'' {
                    quote = Quote::None;
                } else {
                    current.push(c);
                }
            }
            Quote::Double => match c {
                '"' => quote = Quote::None,
                '\\' => match chars.peek() {
                    Some(&next) if matches!(next, '"' | '\\' | '$' | '`') => {
                        current.push(next);
                        chars.next();
                    }
                    _ => current.push('\\'),
                },
                _ => current.push(c),
            },
            Quote::None => match c {
                '\'' => {
                    quote = Quote::Single;
                    in_word = true;
                }
                '"' => {
                    quote = Quote::Double;
                    in_word = true;
                }
                '\\' => {
                    current.push(chars.next().unwrap_or('\\'));
                    in_word = true;
                }
                c if c.is_whitespace() => {
                    if in_word {
                        words.push(std::mem::take(&mut current));
                        in_word = false;
                    }
                }
                _ => {
                    current.push(c);
                    in_word = true;
                }
            },
        }
    }

    if quote != Quote::None {
        return Err(ParseError::UnterminatedQuote);
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        tokenize(line).unwrap()
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(words("  ls   -l  dir "), ["ls", "-l", "dir"]);
    }

    #[test]
    fn empty_and_blank_lines_yield_nothing() {
        assert!(words("").is_empty());
        assert!(words("   \t ").is_empty());
    }

    #[test]
    fn double_quotes_group_words() {
        assert_eq!(words(r#"echo "Hello There""#), ["echo", "Hello There"]);
    }

    #[test]
    fn single_quotes_are_literal() {
        assert_eq!(words(r#"echo 'a "b" \n'"#), ["echo", r#"a "b" \n"#]);
    }

    #[test]
    fn quotes_glue_to_adjacent_text() {
        assert_eq!(words(r#"a"b c"d"#), ["ab cd"]);
    }

    #[test]
    fn empty_quotes_make_an_empty_word() {
        assert_eq!(words(r#"echo """#), ["echo", ""]);
    }

    #[test]
    fn backslash_escapes() {
        assert_eq!(words(r"echo a\ b"), ["echo", "a b"]);
        assert_eq!(words(r#"echo "say \"hi\"""#), ["echo", r#"say "hi""#]);
        assert_eq!(words(r#"echo "a\nb""#), ["echo", r"a\nb"]);
    }

    #[test]
    fn unterminated_quote_is_reported() {
        assert_eq!(
            tokenize(r#"echo "oops"#),
            Err(ParseError::UnterminatedQuote)
        );
        assert_eq!(tokenize("echo 'oops"), Err(ParseError::UnterminatedQuote));
    }

    #[test]
    fn newlines_inside_quotes_are_kept() {
        assert_eq!(words("echo \"a\nb\""), ["echo", "a\nb"]);
    }
}
