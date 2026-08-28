// src/interpreter.rs
use crate::env::LispEnv;
use crate::interner;
use crate::interner::{PredefinedSyms, predefined};
use crate::lexer::tokenize;
use crate::parser::parse;
use crate::{LispErr, LispExp, LispLambda};
use std::cell::RefCell;
use std::io::BufRead;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 求值函数 — TCO 蹦床版本
pub fn eval(exp: &LispExp, env: &mut LispEnv) -> Result<LispExp, LispErr> {
    let mut current_exp = exp.clone();
    let mut current_env = std::mem::take(env);

    loop {
        match &current_exp {
            LispExp::Number(_)
            | LispExp::Bool(_)
            | LispExp::Nil
            | LispExp::Func(_)
            | LispExp::Lambda(_)
            | LispExp::Macro(_)
            | LispExp::String(_) => {
                *env = current_env;
                return Ok(current_exp.clone());
            }

            LispExp::Symbol(s) => {
                let res = current_env.get(*s);
                *env = current_env;
                return res;
            }

            LispExp::List(elements) => {
                if elements.is_empty() {
                    *env = current_env;
                    return Ok(LispExp::Nil);
                }

                // ── 特殊形式检查 ──
                if let LispExp::Symbol(sym) = &elements[0] {
                    let args = &elements[1..];
                    let p = predefined();

                    // ---- if（尾位置优化）----
                    if *sym == p.if_sym && elements.len() == 4 {
                        let cond = eval(&elements[1], &mut current_env)?;
                        let is_true = !matches!(cond, LispExp::Bool(false) | LispExp::Nil);
                        current_exp = if is_true {
                            elements[2].clone()
                        } else {
                            elements[3].clone()
                        };
                        continue;
                    }

                    // ---- define（支持递归定义）----
                    if *sym == p.define {
                        if let LispExp::Symbol(name) = &elements[1] {
                            let shared_env =
                                Rc::new(RefCell::new(std::mem::take(&mut current_env)));
                            shared_env.borrow_mut().set(*name, LispExp::Nil);
                            current_env = LispEnv::with_outer(shared_env.clone());
                            let value = eval(&elements[2], &mut current_env)?;
                            current_env = LispEnv::with_outer(shared_env.clone());
                            shared_env.borrow_mut().set(*name, value);
                            *env = current_env;
                            return Ok(LispExp::Nil);
                        } else {
                            *env = current_env;
                            return Err(LispErr::Reason(
                                "define 的第一个参数必须是符号".to_string(),
                            ));
                        }
                    }

                    // ---- quote — 不求值，直接返回 ----
                    if *sym == p.quote {
                        *env = current_env;
                        return Ok(elements[1].clone());
                    }

                    // ---- lambda ----
                    if *sym == p.lambda {
                        let (params, rest) = match &elements[1] {
                            LispExp::List(pl) => parse_lambda_params(pl),
                            _ => {
                                *env = current_env;
                                return Err(LispErr::Reason("lambda 的参数必须是列表".to_string()));
                            }
                        };
                        let body = elements[2].clone();
                        let lambda = LispExp::Lambda(Box::new(LispLambda {
                            params,
                            rest,
                            body: Box::new(body),
                            env: Rc::new(RefCell::new(current_env.clone())),
                        }));
                        *env = current_env;
                        return Ok(lambda);
                    }

                    // ---- defmacro ----
                    if *sym == p.defmacro {
                        if let LispExp::Symbol(name) = &elements[1] {
                            let (params, rest) = match &elements[2] {
                                LispExp::List(pl) => parse_lambda_params(pl),
                                _ => {
                                    *env = current_env;
                                    return Err(LispErr::Reason(
                                        "defmacro 的参数必须是列表".to_string(),
                                    ));
                                }
                            };
                            let body = elements[3].clone();
                            let mac = LispExp::Macro(Box::new(LispLambda {
                                params,
                                rest,
                                body: Box::new(body),
                                env: Rc::new(RefCell::new(current_env.clone())),
                            }));
                            current_env.set(*name, mac);
                            *env = current_env;
                            return Ok(LispExp::Nil);
                        }
                    }

                    // ---- begin — 顺序求值 ----
                    if *sym == p.begin {
                        if args.is_empty() {
                            *env = current_env;
                            return Ok(LispExp::Nil);
                        }
                        for arg in &args[..args.len() - 1] {
                            eval(arg, &mut current_env)?;
                        }
                        current_exp = args.last().unwrap().clone();
                        continue;
                    }

                    // ---- set! — 修改已有绑定 ----
                    if *sym == p.set_bang {
                        if args.len() != 2 {
                            *env = current_env;
                            return Err(LispErr::Reason("set! 需要 2 个参数".into()));
                        }
                        if let LispExp::Symbol(name) = &args[0] {
                            let value = eval(&args[1], &mut current_env)?;
                            current_env.set_upward(*name, value)?;
                            *env = current_env;
                            return Ok(LispExp::Nil);
                        }
                    }

                    // ---- let — 局部绑定（脱糖为 lambda 调用）----
                    if *sym == p.let_sym {
                        let bindings = &args[0];
                        let body_exprs = &args[1..];
                        let mut names = Vec::new();
                        let mut vals = Vec::new();
                        if let LispExp::List(binds) = bindings {
                            for bind in binds {
                                if let LispExp::List(b) = bind {
                                    if let LispExp::Symbol(n) = &b[0] {
                                        names.push(LispExp::Symbol(*n));
                                        vals.push(b[1].clone());
                                    }
                                }
                            }
                        }

                        let body = if body_exprs.len() == 1 {
                            body_exprs[0].clone()
                        } else {
                            LispExp::List(
                                std::iter::once(LispExp::Symbol(predefined().begin))
                                    .chain(body_exprs.iter().cloned())
                                    .collect(),
                            )
                        };

                        let lambda = LispExp::List(vec![
                            LispExp::Symbol(predefined().lambda),
                            LispExp::List(names),
                            body,
                        ]);
                        let mut call = vec![lambda];
                        call.extend(vals);
                        current_exp = LispExp::List(call);
                        continue;
                    }

                    // ---- cond — 多路分支 ----
                    if *sym == p.cond_sym {
                        let mut new_exp: Option<LispExp> = None;
                        for clause in args {
                            if let LispExp::List(els) = clause {
                                if els.is_empty() {
                                    continue;
                                }
                                let test = &els[0];
                                let body = &els[1..];
                                let is_else = matches!(test, LispExp::Symbol(id) if interner::lookup(*id) == "else");
                                let passed = is_else || {
                                    let r = eval(test, &mut current_env)?;
                                    !matches!(r, LispExp::Bool(false) | LispExp::Nil)
                                };
                                if passed {
                                    if body.is_empty() {
                                        *env = current_env;
                                        return Ok(LispExp::Nil);
                                    }
                                    new_exp = Some(if body.len() == 1 {
                                        body[0].clone()
                                    } else {
                                        LispExp::List(
                                            std::iter::once(LispExp::Symbol(predefined().begin))
                                                .chain(body.iter().cloned())
                                                .collect(),
                                        )
                                    });
                                    break;
                                }
                            }
                        }
                        if let Some(exp) = new_exp {
                            current_exp = exp;
                            continue;
                        }
                        *env = current_env;
                        return Ok(LispExp::Nil);
                    }

                    // ---- and — 短路逻辑与 ----
                    if *sym == p.and_sym {
                        if args.is_empty() {
                            *env = current_env;
                            return Ok(LispExp::Bool(true));
                        }
                        for arg in &args[..args.len() - 1] {
                            let v = eval(arg, &mut current_env)?;
                            if matches!(v, LispExp::Bool(false) | LispExp::Nil) {
                                *env = current_env;
                                return Ok(v);
                            }
                        }
                        current_exp = args.last().unwrap().clone();
                        continue;
                    }

                    // ---- or — 短路逻辑或 ----
                    if *sym == p.or_sym {
                        if args.is_empty() {
                            *env = current_env;
                            return Ok(LispExp::Bool(false));
                        }
                        for arg in &args[..args.len() - 1] {
                            let v = eval(arg, &mut current_env)?;
                            if !matches!(v, LispExp::Bool(false) | LispExp::Nil) {
                                *env = current_env;
                                return Ok(v);
                            }
                        }
                        current_exp = args.last().unwrap().clone();
                        continue;
                    }

                    // ---- let* — 顺序绑定（脱糖为嵌套 let）----
                    if *sym == p.let_star {
                        let bindings = &args[0];
                        let body_exprs = &args[1..];
                        let binds: Vec<&LispExp> = if let LispExp::List(b) = bindings {
                            b.iter().collect()
                        } else {
                            vec![]
                        };

                        let body = if body_exprs.len() == 1 {
                            body_exprs[0].clone()
                        } else {
                            LispExp::List(
                                std::iter::once(LispExp::Symbol(predefined().begin))
                                    .chain(body_exprs.iter().cloned())
                                    .collect(),
                            )
                        };

                        let mut result = body.clone();
                        for bind in binds.iter().rev() {
                            if let LispExp::List(b) = bind {
                                if b.len() >= 2 {
                                    if let LispExp::Symbol(n) = &b[0] {
                                        let val = b[1].clone();
                                        result = LispExp::List(vec![
                                            LispExp::Symbol(predefined().let_sym),
                                            LispExp::List(vec![LispExp::List(vec![
                                                LispExp::Symbol(*n),
                                                val,
                                            ])]),
                                            result,
                                        ]);
                                    }
                                }
                            }
                        }
                        if binds.is_empty() {
                            result = body;
                        }
                        current_exp = result;
                        continue;
                    }

                    // ---- letrec — 递归绑定 ----
                    if *sym == p.letrec {
                        let bindings = &args[0];
                        let body_exprs = &args[1..];

                        let shared_env = Rc::new(RefCell::new(current_env.clone()));

                        if let LispExp::List(binds) = bindings {
                            for bind in binds {
                                if let LispExp::List(b) = bind {
                                    if let LispExp::Symbol(n) = &b[0] {
                                        shared_env.borrow_mut().set(*n, LispExp::Nil);
                                    }
                                }
                            }
                        }

                        let mut eval_env = LispEnv::with_outer(shared_env.clone());
                        if let LispExp::List(binds) = bindings {
                            for bind in binds {
                                if let LispExp::List(b) = bind {
                                    if b.len() >= 2 {
                                        if let LispExp::Symbol(n) = &b[0] {
                                            let val = eval(&b[1], &mut eval_env)?;
                                            shared_env.borrow_mut().set(*n, val);
                                        }
                                    }
                                }
                            }
                        }

                        let body = if body_exprs.len() == 1 {
                            body_exprs[0].clone()
                        } else {
                            LispExp::List(
                                std::iter::once(LispExp::Symbol(predefined().begin))
                                    .chain(body_exprs.iter().cloned())
                                    .collect(),
                            )
                        };
                        current_env = LispEnv::with_outer(shared_env);
                        current_exp = body;
                        continue;
                    }

                    // ---- quasiquote ----
                    if *sym == p.quasiquote {
                        let expanded = qq_expand(&args[0], &p);
                        current_exp = expanded;
                        continue;
                    }

                    // ── 宏展开 ──
                    let mut macro_expansion: Option<LispExp> = None;
                    if let Ok(LispExp::Macro(mac)) = current_env.get(*sym) {
                        let mut new_env = LispEnv::with_outer(mac.env.clone());
                        for (param, arg) in mac.params.iter().zip(args.iter()) {
                            new_env.set(*param, arg.clone());
                        }
                        if let Some(rest_id) = mac.rest {
                            let rest_args: Vec<LispExp> = args
                                .get(mac.params.len()..)
                                .map(|s| s.to_vec())
                                .unwrap_or_default();
                            new_env.set(rest_id, LispExp::List(rest_args));
                        }
                        macro_expansion = Some(eval(&mac.body, &mut new_env)?);
                    }
                    if let Some(expanded) = macro_expansion {
                        current_exp = expanded;
                        continue;
                    }
                }

                // ── 普通函数调用 ──
                let func = eval(&elements[0], &mut current_env)?;
                let args: Vec<LispExp> = elements[1..]
                    .iter()
                    .map(|a| eval(a, &mut current_env))
                    .collect::<Result<_, _>>()?;

                match func {
                    LispExp::Func(f) => {
                        *env = current_env;
                        return f(&args);
                    }
                    LispExp::Lambda(lambda) => {
                        let mut new_env = LispEnv::with_outer(lambda.env.clone());
                        for (param, arg) in lambda.params.iter().zip(args.iter()) {
                            new_env.set(*param, arg.clone());
                        }
                        if let Some(rest_id) = lambda.rest {
                            let extra: Vec<LispExp> = args
                                .get(lambda.params.len()..)
                                .map(|s| s.to_vec())
                                .unwrap_or_default();
                            new_env.set(rest_id, LispExp::List(extra));
                        }
                        current_exp = lambda.body.as_ref().clone();
                        current_env = new_env;
                        continue;
                    }
                    _ => {
                        *env = current_env;
                        return Err(LispErr::Reason("不是函数".to_string()));
                    }
                }
            }
        }
    }
}

/// 解析 lambda/defmacro 参数列表，处理变参 (a . rest)
fn parse_lambda_params(pl: &[LispExp]) -> (Vec<u64>, Option<u64>) {
    let dot_id = interner::intern(".");
    let mut params = Vec::new();
    let mut rest = None;
    let mut i = 0;
    while i < pl.len() {
        if let LispExp::Symbol(n) = &pl[i] {
            if *n == dot_id && i + 1 < pl.len() {
                if let LispExp::Symbol(rest_n) = &pl[i + 1] {
                    rest = Some(*rest_n);
                }
                break;
            }
            params.push(*n);
        }
        i += 1;
    }
    (params, rest)
}

/// quasiquote 展开函数
fn qq_expand(exp: &LispExp, p: &PredefinedSyms) -> LispExp {
    use LispExp::*;
    let quote = interner::intern("quote");
    let cons = interner::intern("cons");
    let append = interner::intern("append");

    match exp {
        Number(_) | String(_) | Bool(_) | Nil => List(vec![Symbol(quote), exp.clone()]),
        Symbol(_) => List(vec![Symbol(quote), exp.clone()]),
        List(elements) if !elements.is_empty() => {
            // 检查第一个元素是否是 (unquote x) 即 ,x
            if let List(inner) = &elements[0] {
                if inner.len() == 2 {
                    if let Symbol(s) = &inner[0] {
                        if *s == p.unquote {
                            return inner[1].clone();
                        }
                    }
                }
            }

            // 普通列表 → 从右向左构建 cons 链
            let mut result = List(vec![Symbol(quote), List(vec![])]);

            for el in elements.iter().rev() {
                if let List(inner) = el {
                    if inner.len() == 2 {
                        if let Symbol(s) = &inner[0] {
                            if *s == p.unquote_splicing {
                                result = List(vec![Symbol(append), inner[1].clone(), result]);
                                continue;
                            }
                            if *s == p.unquote {
                                result = List(vec![Symbol(cons), inner[1].clone(), result]);
                                continue;
                            }
                        }
                    }
                }
                // 普通元素 → (cons <展开此元素> <已累积的结果>)
                let expanded = qq_expand(el, p);
                result = List(vec![Symbol(cons), expanded, result]);
            }
            result
        }
        List(_) => List(vec![Symbol(quote), List(vec![])]),
        _ => List(vec![Symbol(quote), exp.clone()]),
    }
}

/// 辅助函数: 从源码字符串直接求值
pub fn eval_str(source: &str, env: &mut LispEnv) -> Result<LispExp, LispErr> {
    let tokens = tokenize(source);
    let (exp, _) = parse(&tokens)?;
    eval(&exp, env)
}

/// 结构相等 — 递归比较嵌套列表
fn lisp_equal(a: &LispExp, b: &LispExp) -> bool {
    match (a, b) {
        (LispExp::List(a_els), LispExp::List(b_els)) => {
            a_els.len() == b_els.len() && a_els.iter().zip(b_els).all(|(x, y)| lisp_equal(x, y))
        }
        _ => a == b,
    }
}

pub fn default_env() -> LispEnv {
    let mut env = LispEnv::new();

    // ── 算术 ──
    env.set(
        interner::intern("+"),
        LispExp::Func(|args| {
            let sum: f64 = args
                .iter()
                .filter_map(|a| {
                    if let LispExp::Number(n) = a {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .sum();
            Ok(LispExp::Number(sum))
        }),
    );

    env.set(
        interner::intern("-"),
        LispExp::Func(|args| {
            let nums: Vec<f64> = args
                .iter()
                .filter_map(|a| {
                    if let LispExp::Number(n) = a {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .collect();
            if nums.len() == 1 {
                Ok(LispExp::Number(-nums[0]))
            } else {
                Ok(LispExp::Number(nums[0] - nums[1..].iter().sum::<f64>()))
            }
        }),
    );

    env.set(
        interner::intern("*"),
        LispExp::Func(|args| {
            let product: f64 = args
                .iter()
                .filter_map(|a| {
                    if let LispExp::Number(n) = a {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .product();
            Ok(LispExp::Number(product))
        }),
    );

    env.set(
        interner::intern("/"),
        LispExp::Func(|args| {
            let nums: Vec<f64> = args
                .iter()
                .filter_map(|a| {
                    if let LispExp::Number(n) = a {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .collect();
            if nums.len() == 1 {
                Ok(LispExp::Number(1.0 / nums[0]))
            } else {
                Ok(LispExp::Number(nums[0] / nums[1..].iter().product::<f64>()))
            }
        }),
    );

    // ── 比较函数 ──
    env.set(
        interner::intern("="),
        LispExp::Func(|args| {
            if let (LispExp::Number(a), LispExp::Number(b)) = (&args[0], &args[1]) {
                Ok(LispExp::Bool(a == b))
            } else {
                Err(LispErr::Reason("= 需要数字".to_string()))
            }
        }),
    );

    env.set(
        interner::intern(">"),
        LispExp::Func(|args| {
            if let (LispExp::Number(a), LispExp::Number(b)) = (&args[0], &args[1]) {
                Ok(LispExp::Bool(a > b))
            } else {
                Err(LispErr::Reason("> 需要数字".to_string()))
            }
        }),
    );

    env.set(
        interner::intern("<"),
        LispExp::Func(|args| {
            if let (LispExp::Number(a), LispExp::Number(b)) = (&args[0], &args[1]) {
                Ok(LispExp::Bool(a < b))
            } else {
                Err(LispErr::Reason("< 需要数字".to_string()))
            }
        }),
    );

    env.set(
        interner::intern(">="),
        LispExp::Func(|args| {
            if let (LispExp::Number(a), LispExp::Number(b)) = (&args[0], &args[1]) {
                Ok(LispExp::Bool(a >= b))
            } else {
                Err(LispErr::Reason(">= 需要数字".to_string()))
            }
        }),
    );

    env.set(
        interner::intern("<="),
        LispExp::Func(|args| {
            if let (LispExp::Number(a), LispExp::Number(b)) = (&args[0], &args[1]) {
                Ok(LispExp::Bool(a <= b))
            } else {
                Err(LispErr::Reason("<= 需要数字".to_string()))
            }
        }),
    );

    // ── 逻辑非 ──
    env.set(
        interner::intern("not"),
        LispExp::Func(|args| {
            let is_false = matches!(args[0], LispExp::Bool(false) | LispExp::Nil);
            Ok(LispExp::Bool(is_false))
        }),
    );

    // ── 列表操作 ──
    env.set(
        interner::intern("list"),
        LispExp::Func(|args| Ok(LispExp::List(args.to_vec()))),
    );

    env.set(
        interner::intern("cons"),
        LispExp::Func(|args| match &args[1] {
            LispExp::List(els) => {
                let mut new_list = vec![args[0].clone()];
                new_list.extend(els.clone());
                Ok(LispExp::List(new_list))
            }
            LispExp::Nil => Ok(LispExp::List(vec![args[0].clone()])),
            _ => Err(LispErr::Reason("cons 第二个参数必须是列表".into())),
        }),
    );

    env.set(
        interner::intern("car"),
        LispExp::Func(|args| match &args[0] {
            LispExp::List(els) if !els.is_empty() => Ok(els[0].clone()),
            LispExp::List(_) => Err(LispErr::Reason("car: 空列表".into())),
            _ => Err(LispErr::Reason("car 需要列表".into())),
        }),
    );

    env.set(
        interner::intern("cdr"),
        LispExp::Func(|args| match &args[0] {
            LispExp::List(els) if !els.is_empty() => Ok(LispExp::List(els[1..].to_vec())),
            LispExp::List(_) => Err(LispErr::Reason("cdr: 空列表".into())),
            _ => Err(LispErr::Reason("cdr 需要列表".into())),
        }),
    );

    env.set(
        interner::intern("cadr"),
        LispExp::Func(|args| match &args[0] {
            LispExp::List(els) if els.len() >= 2 => Ok(els[1].clone()),
            _ => Err(LispErr::Reason("cadr 需要至少 2 个元素的列表".into())),
        }),
    );

    env.set(
        interner::intern("caddr"),
        LispExp::Func(|args| match &args[0] {
            LispExp::List(els) if els.len() >= 3 => Ok(els[2].clone()),
            _ => Err(LispErr::Reason("caddr 需要至少 3 个元素的列表".into())),
        }),
    );

    env.set(
        interner::intern("append"),
        LispExp::Func(|args| {
            let mut result = Vec::new();
            for arg in args {
                match arg {
                    LispExp::List(els) => result.extend(els.clone()),
                    LispExp::Nil => {}
                    _ => return Err(LispErr::Reason("append 参数必须是列表".into())),
                }
            }
            Ok(LispExp::List(result))
        }),
    );

    env.set(
        interner::intern("length"),
        LispExp::Func(|args| match &args[0] {
            LispExp::List(els) => Ok(LispExp::Number(els.len() as f64)),
            LispExp::Nil => Ok(LispExp::Number(0.0)),
            _ => Err(LispErr::Reason("length 需要列表".into())),
        }),
    );

    env.set(
        interner::intern("reverse"),
        LispExp::Func(|args| match &args[0] {
            LispExp::List(els) => {
                let mut r = els.clone();
                r.reverse();
                Ok(LispExp::List(r))
            }
            LispExp::Nil => Ok(LispExp::Nil),
            _ => Err(LispErr::Reason("reverse 需要列表".into())),
        }),
    );

    env.set(
        interner::intern("member"),
        LispExp::Func(|args| match &args[1] {
            LispExp::List(els) => {
                for i in 0..els.len() {
                    if els[i] == args[0] {
                        return Ok(LispExp::List(els[i..].to_vec()));
                    }
                }
                Ok(LispExp::Bool(false))
            }
            _ => Err(LispErr::Reason("member 第二个参数需要列表".into())),
        }),
    );

    // ── 类型谓词 ──
    env.set(
        interner::intern("null?"),
        LispExp::Func(|args| Ok(LispExp::Bool(matches!(args[0], LispExp::Nil)))),
    );
    env.set(
        interner::intern("number?"),
        LispExp::Func(|args| Ok(LispExp::Bool(matches!(args[0], LispExp::Number(_))))),
    );
    env.set(
        interner::intern("symbol?"),
        LispExp::Func(|args| Ok(LispExp::Bool(matches!(args[0], LispExp::Symbol(_))))),
    );
    env.set(
        interner::intern("boolean?"),
        LispExp::Func(|args| Ok(LispExp::Bool(matches!(args[0], LispExp::Bool(_))))),
    );
    env.set(
        interner::intern("string?"),
        LispExp::Func(|args| Ok(LispExp::Bool(matches!(args[0], LispExp::String(_))))),
    );
    env.set(
        interner::intern("procedure?"),
        LispExp::Func(|args| {
            Ok(LispExp::Bool(matches!(
                args[0],
                LispExp::Func(_) | LispExp::Lambda(_) | LispExp::Macro(_)
            )))
        }),
    );
    env.set(
        interner::intern("pair?"),
        LispExp::Func(|args| {
            Ok(LispExp::Bool(
                matches!(&args[0], LispExp::List(els) if !els.is_empty()),
            ))
        }),
    );
    env.set(
        interner::intern("list?"),
        LispExp::Func(|args| {
            Ok(LispExp::Bool(matches!(
                args[0],
                LispExp::List(_) | LispExp::Nil
            )))
        }),
    );

    // ── 相等比较 ──
    env.set(
        interner::intern("eq?"),
        LispExp::Func(|args| Ok(LispExp::Bool(args[0] == args[1]))),
    );
    env.set(
        interner::intern("equal?"),
        LispExp::Func(|args| Ok(LispExp::Bool(lisp_equal(&args[0], &args[1])))),
    );

    // ── 高阶函数 ──
    env.set(
        interner::intern("map"),
        LispExp::Func(|args| {
            let func = &args[0];
            let list = match &args[1] {
                LispExp::List(els) => els,
                LispExp::Nil => return Ok(LispExp::Nil),
                _ => return Err(LispErr::Reason("map 第二个参数需要列表".into())),
            };
            let mut results = Vec::new();
            for el in list {
                match func {
                    LispExp::Func(f) => results.push(f(&[el.clone()])?),
                    LispExp::Lambda(lam) => {
                        let mut env = LispEnv::with_outer(lam.env.clone());
                        if let Some(param) = lam.params.first() {
                            env.set(*param, el.clone());
                        }
                        results.push(eval(&lam.body, &mut env)?);
                    }
                    _ => return Err(LispErr::Reason("map 第一个参数需要函数".into())),
                }
            }
            Ok(LispExp::List(results))
        }),
    );

    env.set(
        interner::intern("apply"),
        LispExp::Func(|args| {
            let arg_list = match &args[1] {
                LispExp::List(els) => els.clone(),
                _ => vec![],
            };
            match &args[0] {
                LispExp::Func(f) => f(&arg_list),
                LispExp::Lambda(lam) => {
                    let mut env = LispEnv::with_outer(lam.env.clone());
                    for (p, a) in lam.params.iter().zip(arg_list.iter()) {
                        env.set(*p, a.clone());
                    }
                    if let Some(rest_id) = lam.rest {
                        let extra: Vec<LispExp> = arg_list
                            .get(lam.params.len()..)
                            .map(|s| s.to_vec())
                            .unwrap_or_default();
                        env.set(rest_id, LispExp::List(extra));
                    }
                    eval(&lam.body, &mut env)
                }
                _ => Err(LispErr::Reason("apply 第一个参数需要函数".into())),
            }
        }),
    );

    env.set(
        interner::intern("filter"),
        LispExp::Func(|args| {
            let pred = &args[0];
            let list = match &args[1] {
                LispExp::List(els) => els,
                _ => return Err(LispErr::Reason("filter 第二个参数需要列表".into())),
            };
            let mut results = Vec::new();
            for el in list {
                let keep = match pred {
                    LispExp::Func(f) => {
                        !matches!(f(&[el.clone()])?, LispExp::Bool(false) | LispExp::Nil)
                    }
                    LispExp::Lambda(lam) => {
                        let mut env = LispEnv::with_outer(lam.env.clone());
                        if let Some(p) = lam.params.first() {
                            env.set(*p, el.clone());
                        }
                        !matches!(
                            eval(&lam.body, &mut env)?,
                            LispExp::Bool(false) | LispExp::Nil
                        )
                    }
                    _ => return Err(LispErr::Reason("filter 第一个参数需要函数".into())),
                };
                if keep {
                    results.push(el.clone());
                }
            }
            Ok(LispExp::List(results))
        }),
    );

    // ── gensym — 卫生宏 ──
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    env.set(
        interner::intern("gensym"),
        LispExp::Func(|args| {
            let prefix = if let Some(LispExp::String(s)) = args.first() {
                s.clone()
            } else {
                "g".into()
            };
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            Ok(LispExp::Symbol(interner::intern(&format!(
                "{}__{}",
                prefix, id
            ))))
        }),
    );

    // ── error ──
    env.set(
        interner::intern("error"),
        LispExp::Func(|args| {
            let msg = args
                .first()
                .map(|a| format!("{}", a))
                .unwrap_or("error".into());
            Err(LispErr::Reason(msg))
        }),
    );

    // ── I/O 函数 ──
    env.set(
        interner::intern("display"),
        LispExp::Func(|args| {
            if let Some(arg) = args.first() {
                match arg {
                    LispExp::String(s) => print!("{}", s),
                    _ => print!("{}", arg),
                }
            }
            Ok(LispExp::Nil)
        }),
    );

    env.set(
        interner::intern("newline"),
        LispExp::Func(|_args| {
            println!();
            Ok(LispExp::Nil)
        }),
    );

    env.set(
        interner::intern("read"),
        LispExp::Func(|_args| {
            let stdin = std::io::stdin();
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => Ok(LispExp::Nil),
                Ok(_) => Ok(LispExp::String(line.trim_end().to_string())),
                Err(_) => Err(LispErr::Reason("read: 读取输入失败".into())),
            }
        }),
    );

    env
}

// ── 测试 ──
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_number() {
        let n = LispExp::Number(42.0);
        assert_eq!(n, LispExp::Number(42.0));
    }

    #[test]
    fn test_eval_number() {
        let mut env = LispEnv::new();
        let exp = LispExp::Number(42.0);
        let result = eval(&exp, &mut env).unwrap();
        assert_eq!(result, LispExp::Number(42.0));
    }

    #[test]
    fn test_eval_str_number() {
        let mut env = LispEnv::new();
        assert_eq!(eval_str("42", &mut env).unwrap(), LispExp::Number(42.0));
    }

    #[test]
    fn test_eval_symbol() {
        let mut env = LispEnv::new();
        env.set(interner::intern("x"), LispExp::Number(42.0));
        assert_eq!(eval_str("x", &mut env).unwrap(), LispExp::Number(42.0));
    }

    #[test]
    fn test_eval_addition() {
        let mut env = default_env();
        assert_eq!(eval_str("(+ 1 2)", &mut env).unwrap(), LispExp::Number(3.0));
    }

    // ── 步骤 17: 减法 ──
    #[test]
    fn test_subtraction() {
        let mut env = default_env();
        // 二元减法
        assert_eq!(
            eval_str("(- 10 3)", &mut env).unwrap(),
            LispExp::Number(7.0)
        );
        // 多参数减法: 10 - 2 - 3 = 5
        assert_eq!(
            eval_str("(- 10 2 3)", &mut env).unwrap(),
            LispExp::Number(5.0)
        );
        // 单参数取负: (- 5) = -5
        assert_eq!(
            eval_str("(- 5)", &mut env).unwrap(),
            LispExp::Number(-5.0)
        );
    }

    // ── 步骤 17: 乘法 ──
    #[test]
    fn test_multiplication() {
        let mut env = default_env();
        // 二元乘法
        assert_eq!(
            eval_str("(* 2 3)", &mut env).unwrap(),
            LispExp::Number(6.0)
        );
        // 多参数乘法: 2 * 3 * 4 = 24
        assert_eq!(
            eval_str("(* 2 3 4)", &mut env).unwrap(),
            LispExp::Number(24.0)
        );
        // 单参数乘法: (* 5) = 5
        assert_eq!(
            eval_str("(* 5)", &mut env).unwrap(),
            LispExp::Number(5.0)
        );
    }

    // ── 步骤 17: 除法 ──
    #[test]
    fn test_division() {
        let mut env = default_env();
        // 二元除法
        assert_eq!(
            eval_str("(/ 10 2)", &mut env).unwrap(),
            LispExp::Number(5.0)
        );
        // 多参数除法: 100 / 5 / 2 = 10
        assert_eq!(
            eval_str("(/ 100 5 2)", &mut env).unwrap(),
            LispExp::Number(10.0)
        );
        // 单参数取倒数: (/ 5) = 0.2
        assert_eq!(
            eval_str("(/ 5)", &mut env).unwrap(),
            LispExp::Number(0.2)
        );
    }

    #[test]
    fn test_if_true_branch() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(if #t 1 2)", &mut env).unwrap(),
            LispExp::Number(1.0)
        );
    }

    #[test]
    fn test_if_false_branch() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(if #f 1 2)", &mut env).unwrap(),
            LispExp::Number(2.0)
        );
    }

    #[test]
    fn test_if_with_comparison() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(if (= 1 1) 10 20)", &mut env).unwrap(),
            LispExp::Number(10.0)
        );
    }

    #[test]
    fn test_define_and_lookup() {
        let mut env = default_env();
        eval_str("(define x 10)", &mut env).unwrap();
        assert_eq!(eval_str("x", &mut env).unwrap(), LispExp::Number(10.0));
    }

    #[test]
    fn test_define_then_use_in_calc() {
        let mut env = default_env();
        eval_str("(define x 10)", &mut env).unwrap();
        assert_eq!(
            eval_str("(+ x 5)", &mut env).unwrap(),
            LispExp::Number(15.0)
        );
    }

    #[test]
    fn test_lambda_creation() {
        // lambda 创建后，返回的是一个函数值 LispExp::Lambda
        match eval_str("(lambda (x) (* x x))", &mut default_env()).unwrap() {
            LispExp::Lambda(_) => {}
            other => panic!("expected LispExp::Lambda, got {:?}", other),
        }
    }

    #[test]
    fn test_lambda_call() {
        let mut env = default_env();
        eval_str("(define add (lambda (a b) (+ a b)))", &mut env).unwrap();
        assert_eq!(
            eval_str("(add 3 4)", &mut env).unwrap(),
            LispExp::Number(7.0)
        );
    }

    #[test]
    fn test_lambda_direct_call() {
        let mut env = default_env();
        assert_eq!(
            eval_str("((lambda (x) (* x x)) 5)", &mut env).unwrap(),
            LispExp::Number(25.0)
        );
    }

    #[test]
    fn test_closure() {
        let mut env = default_env();
        eval_str(
            "(define make-adder (lambda (n) (lambda (x) (+ x n))))",
            &mut env,
        )
        .unwrap();
        eval_str("(define add5 (make-adder 5))", &mut env).unwrap();
        assert_eq!(
            eval_str("(add5 10)", &mut env).unwrap(),
            LispExp::Number(15.0)
        );
    }

    #[test]
    fn test_tail_call_optimization() {
        let mut env = default_env();
        eval_str(
            "(define loop (lambda (n) (if (= n 0) \"done\" (loop (- n 1)))))",
            &mut env,
        )
        .unwrap();
        let result = eval_str("(loop 10000)", &mut env).unwrap();
        assert_eq!(result, LispExp::String("done".to_string()));
    }

    // ── 步骤 44: begin ──
    #[test]
    fn test_begin() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(begin 1 2 3)", &mut env).unwrap(),
            LispExp::Number(3.0)
        );
        assert_eq!(eval_str("(begin)", &mut env).unwrap(), LispExp::Nil);
    }

    // ── 步骤 45: set! ──
    #[test]
    fn test_set_bang() {
        let mut env = default_env();
        eval_str("(define x 10)", &mut env).unwrap();
        assert_eq!(eval_str("x", &mut env).unwrap(), LispExp::Number(10.0));
        eval_str("(set! x 20)", &mut env).unwrap();
        assert_eq!(eval_str("x", &mut env).unwrap(), LispExp::Number(20.0));
    }

    // ── 步骤 46: let ──
    #[test]
    fn test_let() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(let ((x 1) (y 2)) (+ x y))", &mut env).unwrap(),
            LispExp::Number(3.0)
        );
        assert_eq!(
            eval_str("(let () 42)", &mut env).unwrap(),
            LispExp::Number(42.0)
        );
    }

    // ── 步骤 47: cond ──
    #[test]
    fn test_cond() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(cond ((> 3 5) 1) ((< 3 5) 2) (else 3))", &mut env).unwrap(),
            LispExp::Number(2.0)
        );
        assert_eq!(
            eval_str("(cond ((> 3 5) 1))", &mut env).unwrap(),
            LispExp::Nil
        );
    }

    // ── 步骤 50: let* ──
    #[test]
    fn test_let_star() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(let* ((x 1) (y (+ x 1))) (+ x y))", &mut env).unwrap(),
            LispExp::Number(3.0)
        );
        assert_eq!(
            eval_str("(let* ((a 1) (b (+ a 1)) (c (+ b 1))) c)", &mut env).unwrap(),
            LispExp::Number(3.0)
        );
    }

    // ── 步骤 51: letrec ──
    #[test]
    fn test_letrec() {
        let mut env = default_env();
        let result = eval_str(
            "(letrec ((even? (lambda (n) (if (= n 0) #t (odd? (- n 1))))) \
                       (odd?  (lambda (n) (if (= n 0) #f (even? (- n 1)))))) \
              (even? 10))",
            &mut env,
        )
        .unwrap();
        assert_eq!(result, LispExp::Bool(true));
    }

    // ── 步骤 52: < ──
    #[test]
    fn test_less_than() {
        let mut env = default_env();
        assert_eq!(eval_str("(< 3 5)", &mut env).unwrap(), LispExp::Bool(true));
        assert_eq!(eval_str("(< 5 3)", &mut env).unwrap(), LispExp::Bool(false));
    }

    // ── 步骤 60: length ──
    #[test]
    fn test_length() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(length (list 1 2 3))", &mut env).unwrap(),
            LispExp::Number(3.0)
        );
        assert_eq!(
            eval_str("(length nil)", &mut env).unwrap(),
            LispExp::Number(0.0)
        );
        assert_eq!(
            eval_str("(length (list))", &mut env).unwrap(),
            LispExp::Number(0.0)
        );
    }

    // ── 步骤 61: reverse ──
    #[test]
    fn test_reverse() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(reverse (list 1 2 3))", &mut env).unwrap(),
            LispExp::List(vec![
                LispExp::Number(3.0),
                LispExp::Number(2.0),
                LispExp::Number(1.0),
            ])
        );
        assert_eq!(eval_str("(reverse nil)", &mut env).unwrap(), LispExp::Nil);
    }

    // ── 步骤 62: member ──
    #[test]
    fn test_member() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(member 2 (list 1 2 3))", &mut env).unwrap(),
            LispExp::List(vec![LispExp::Number(2.0), LispExp::Number(3.0)])
        );
        assert_eq!(
            eval_str("(member 5 (list 1 2 3))", &mut env).unwrap(),
            LispExp::Bool(false)
        );
    }

    // ── 步骤 63-65: 类型谓词 ──
    #[test]
    fn test_type_predicates() {
        let mut env = default_env();
        assert_eq!(
            eval_str("(null? nil)", &mut env).unwrap(),
            LispExp::Bool(true)
        );
        assert_eq!(
            eval_str("(null? 0)", &mut env).unwrap(),
            LispExp::Bool(false)
        );
        assert_eq!(
            eval_str("(number? 42)", &mut env).unwrap(),
            LispExp::Bool(true)
        );
        assert_eq!(
            eval_str("(number? \"hello\")", &mut env).unwrap(),
            LispExp::Bool(false)
        );
        assert_eq!(
            eval_str("(symbol? 'x)", &mut env).unwrap(),
            LispExp::Bool(true)
        );
    }

    // ── 步骤 71b: defmacro ──
    #[test]
    fn test_defmacro_basic() {
        let mut env = default_env();
        eval_str("(defmacro twice (x) (list '+ x x))", &mut env).unwrap();
        assert_eq!(
            eval_str("(twice 5)", &mut env).unwrap(),
            LispExp::Number(10.0)
        );
    }

    #[test]
    fn test_defmacro_when() {
        let mut env = default_env();
        eval_str(
            "(defmacro when (condition . body) (list 'if condition (cons 'begin body) 'nil))",
            &mut env,
        )
        .unwrap();
        assert_eq!(
            eval_str("(when #t 42)", &mut env).unwrap(),
            LispExp::Number(42.0)
        );
        assert_eq!(eval_str("(when #f 42)", &mut env).unwrap(), LispExp::Nil);
    }

    // ── 步骤 73b: display/newline ──
    #[test]
    fn test_display_returns_nil() {
        let mut env = default_env();
        assert_eq!(eval_str("(display 42)", &mut env).unwrap(), LispExp::Nil);
        assert_eq!(
            eval_str("(display \"hello\")", &mut env).unwrap(),
            LispExp::Nil
        );
    }

    #[test]
    fn test_newline_returns_nil() {
        let mut env = default_env();
        assert_eq!(eval_str("(newline)", &mut env).unwrap(), LispExp::Nil);
    }
}
