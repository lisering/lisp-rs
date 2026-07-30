// src/parser.rs
use crate::interner;
use crate::{LispErr, LispExp};
//  ↑
//  "从当前 crate(本项目) 拿 LispExp 和 LispErr 过来用"

/// 解析 Token 列表 → 表达式
/// 返回: (解析出的表达式, 还剩几个 Token 没处理)
pub fn parse<'a>(tokens: &'a [&'a str]) -> Result<(LispExp, &'a [&'a str]), LispErr> {
    let (token, rest) = tokens
        .split_first()
        .ok_or(LispErr::Reason("没有 Token 了".to_string()))?;

    match *token {
        // token 是 &&str，*token 是 &str
        "(" => read_seq(rest), // 左括号 → 开始读列表
        ")" => Err(LispErr::Reason("多余的 )".to_string())),
        "'" => {
            // quote 缩写: 'x → (quote x)
            let (quoted, rest2) = parse(rest)?;
            Ok((
                LispExp::List(vec![LispExp::Symbol(interner::intern("quote")), quoted]),
                rest2,
            ))
        }
        "`" => {
            // quasiquote: `x → (quasiquote x)
            let (quoted, rest2) = parse(rest)?;
            Ok((
                LispExp::List(vec![
                    LispExp::Symbol(interner::intern("quasiquote")),
                    quoted,
                ]),
                rest2,
            ))
        }
        "," => {
            // unquote: ,x → (unquote x)
            let (unquoted, rest2) = parse(rest)?;
            Ok((
                LispExp::List(vec![LispExp::Symbol(interner::intern("unquote")), unquoted]),
                rest2,
            ))
        }
        ",@" => {
            // unquote-splicing: ,@x → (unquote-splicing x)
            let (unquoted, rest2) = parse(rest)?;
            Ok((
                LispExp::List(vec![
                    LispExp::Symbol(interner::intern("unquote-splicing")),
                    unquoted,
                ]),
                rest2,
            ))
        }
        _ => Ok((parse_atom(*token), rest)),
    }
}

/// 读列表: 从左括号之后开始, 遇到 ) 结束
fn read_seq<'a>(tokens: &'a [&'a str]) -> Result<(LispExp, &'a [&'a str]), LispErr> {
    let mut elements = Vec::new(); // 空列表
    let mut remaining = tokens; // 还剩的 Token

    loop {
        // ← 一直循环, 直到遇到 )
        let (token, rest) = remaining
            .split_first()
            .ok_or(LispErr::Reason("缺少 )".to_string()))?;

        if *token == ")" {
            // 遇到了 ) → 列表结束, 返回收集到的所有元素
            return Ok((LispExp::List(elements), rest));
        }

        // 递归: 调用 parse 解析下一个元素
        // (这个元素本身可能又是一个列表!)
        let (exp, new_rest) = parse(remaining)?;
        elements.push(exp); // 加入列表
        remaining = new_rest; // 更新剩余 Token
    }
}

fn parse_atom(token: &str) -> LispExp {
    // 先试着当数字解析...
    if let Ok(num) = token.parse::<f64>() {
        return LispExp::Number(num);
    }
    // 布尔值
    if token == "#t" {
        return LispExp::Bool(true);
    }
    if token == "#f" {
        return LispExp::Bool(false);
    }
    // 空值
    if token == "nil" {
        return LispExp::Nil;
    }
    // 字符串字面量: "hello" → String("hello")
    if token.starts_with('"') && token.ends_with('"') && token.len() >= 2 {
        return LispExp::String(token[1..token.len() - 1].to_string());
    }
    // 不是数字就当符号（驻留为 u64 ID）
    LispExp::Symbol(interner::intern(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interner;

    #[test]
    fn test_parse_symbol() {
        let tokens = vec!["x"];
        let (exp, _) = parse(&tokens).unwrap();
        assert_eq!(exp, LispExp::Symbol(interner::intern("x")));
    }

    #[test]
    fn test_unclosed_list_error() {
        assert!(parse(&["(", "+", "1"]).is_err());
    }

    #[test]
    fn test_unexpected_close_error() {
        assert!(parse(&[")"]).is_err());
    }
}
