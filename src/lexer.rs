// src/lexer.rs

/// 把源码字符串切成 Token（零拷贝版）
pub fn tokenize(input: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        match ch {
            '(' => tokens.push(&input[i..=i]),
            ')' => tokens.push(&input[i..=i]),
            '\'' => tokens.push(&input[i..=i]),
            '`' => tokens.push(&input[i..=i]),
            ',' => {
                // 检查 ,@ (comma-at)
                if let Some(&(j, '@')) = chars.peek() {
                    chars.next();
                    tokens.push(&input[i..=j]);
                } else {
                    tokens.push(&input[i..=i]);
                }
            }
            '"' => {
                let start = i;
                while let Some((j, c)) = chars.next() {
                    if c == '\\' { chars.next(); continue; }
                    if c == '"' {
                        tokens.push(&input[start..=j]);
                        break;
                    }
                }
            }
            ';' => {
                while let Some((_, c)) = chars.peek() {
                    if *c == '\n' { break; }
                    chars.next();
                }
            }
            c if c.is_whitespace() => {}
            _ => {
                let start = i;
                while let Some((_, c)) = chars.peek() {
                    if c.is_whitespace() || *c == '(' || *c == ')' 
                        || *c == '\'' || *c == '`' || *c == ',' { break; }
                    chars.next();
                }
                let end = chars.peek().map_or(input.len(), |(j, _)| *j);
                tokens.push(&input[start..end]);
            }
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        assert_eq!(tokenize("42"), ["42"]);
    }

    #[test]
    fn test_tokenize_whitespace() {
        assert_eq!(tokenize("  42  "), ["42"]);
    }

    #[test]
    fn test_tokenize_parens() {
        assert_eq!(
            tokenize("(+ 1 2)"),
            ["(", "+", "1", "2", ")"]
        );
    }

    #[test]
    fn test_tokenize_empty() {
        let result = tokenize("");
        assert!(result.is_empty(), "空字符串应该返回空 token 列表，不应 panic");
    }

    #[test]
    fn test_tokenize_comment() {
        assert_eq!(tokenize("42 ; this is a comment"), vec!["42"]);
    }

    #[test]
    fn test_tokenize_string_literal() {
        assert_eq!(tokenize("\"hello\""), vec!["\"hello\""]);
    }

    #[test]
    fn test_comment_ignored() {
        assert_eq!(
            tokenize("(+ 1 2) ; 这是一条注释"),
            ["(", "+", "1", "2", ")"]
        );
    }
}
