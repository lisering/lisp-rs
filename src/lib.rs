// src/lib.rs
pub mod env;
pub mod interner;
pub mod interpreter;
pub mod lexer;
pub mod parser;

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Lambda 表达式（用户自定义函数）
#[derive(Clone, Debug, PartialEq)]
pub struct LispLambda {
    pub params: Vec<u64>,                      // 参数名列表（驻留后的 ID）
    pub rest: Option<u64>,                     // 变参: rest 参数的符号 ID
    pub body: Box<LispExp>,                    // 函数体，用 Box 避免无限嵌套
    pub env: Rc<RefCell<crate::env::LispEnv>>, // 记住"出生"时的环境
}

#[derive(Clone, Debug, PartialEq)]
#[allow(unpredictable_function_pointer_comparisons)]
pub enum LispExp {
    Number(f64),
    Symbol(u64), // 驻留后的整数 ID
    List(Vec<LispExp>),
    Func(fn(&[LispExp]) -> Result<LispExp, LispErr>),
    Lambda(Box<LispLambda>),
    Macro(Box<LispLambda>), // 宏（结构和 Lambda 一样，但求值方式不同）
    Bool(bool),
    Nil,
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LispErr {
    Reason(String),
}

/// Display trait — 让 LispExp 能用 {} 格式化打印
impl fmt::Display for LispExp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LispExp::Number(n) => {
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            LispExp::Symbol(id) => write!(f, "{}", interner::lookup(*id)),
            LispExp::List(els) => {
                write!(f, "(")?;
                for (i, e) in els.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
            LispExp::Bool(b) => write!(f, "{}", if *b { "#t" } else { "#f" }),
            LispExp::Nil => write!(f, "nil"),
            LispExp::String(s) => write!(f, "\"{}\"", s),
            LispExp::Func(_) => write!(f, "#<builtin-function>"),
            LispExp::Lambda(lam) => write!(
                f,
                "#<lambda ({})>",
                lam.params
                    .iter()
                    .map(|&id| interner::lookup(id))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            LispExp::Macro(lam) => write!(
                f,
                "#<macro ({})>",
                lam.params
                    .iter()
                    .map(|&id| interner::lookup(id))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }
    }
}
