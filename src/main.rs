// src/main.rs — 完整 REPL
use lisp_rs::{
    env::LispEnv,
    interpreter::{default_env, eval},
    lexer::tokenize,
    parser::parse,
};
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

/// 读取可能跨多行的输入
fn read_input(stdin: &io::Stdin) -> Option<String> {
    let mut buffer = String::new();
    let mut depth: i32 = 0;
    let mut got_input = false;

    loop {
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                if got_input {
                    break;
                } else {
                    return None;
                }
            }
            Ok(_) => {
                got_input = true;
                buffer.push_str(&line);
                let mut in_string = false;
                let mut escape = false;
                for ch in line.chars() {
                    match ch {
                        '"' if !escape => in_string = !in_string,
                        '\\' if in_string => escape = !escape,
                        '(' if !in_string => depth += 1,
                        ')' if !in_string => depth -= 1,
                        _ => escape = false,
                    }
                }
                if depth <= 0 {
                    break;
                }
                print!("... ");
                io::stdout().flush().unwrap();
            }
            Err(_) => break,
        }
    }
    Some(buffer)
}

/// 求值一行（或多行）Lisp 源码
fn eval_input(input: &str, env: &mut LispEnv) -> Result<String, String> {
    let tokens = tokenize(input);
    if tokens.is_empty() {
        return Ok("nil".to_string());
    }
    let mut remaining: &[&str] = &tokens;
    let mut results = Vec::new();
    while !remaining.is_empty() {
        let (exp, rest) = parse(remaining).map_err(|e| match e {
            lisp_rs::LispErr::Reason(msg) => msg,
        })?;
        remaining = rest;

        match eval(&exp, env) {
            Ok(val) => results.push(val),
            Err(e) => match e {
                lisp_rs::LispErr::Reason(msg) => return Err(msg),
            },
        }
    }
    if results.is_empty() {
        Ok("nil".to_string())
    } else if results.len() == 1 {
        Ok(format!("{}", results[0]))
    } else {
        Ok(results
            .iter()
            .map(|r| format!("{}", r))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // 如果有命令行参数，当作脚本文件执行
    if args.len() > 1 {
        let filename = &args[1];
        let content = match fs::read_to_string(filename) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("无法读取文件 '{}': {}", filename, e);
                std::process::exit(1);
            }
        };
        let mut env = default_env();
        match eval_input(&content, &mut env) {
            Ok(result) => {
                if !result.is_empty() && result != "nil" {
                    println!("{}", result);
                }
            }
            Err(e) => {
                eprintln!("错误: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    println!("Lisp-rs REPL v0.2.0");
    println!("输入 :help 查看帮助, :q 退出, Ctrl+D 退出\n");
    let mut env = default_env();
    let stdin = io::stdin();
    loop {
        print!(">>> ");
        io::stdout().flush().unwrap();
        let input = match read_input(&stdin) {
            Some(s) => s.trim().to_string(),
            None => {
                println!("再见！");
                break;
            }
        };
        if input.is_empty() {
            continue;
        }
        if input.starts_with(':') {
            match input.as_str() {
                ":q" | ":quit" | ":exit" => {
                    println!("再见！");
                    break;
                }
                ":help" => {
                    println!("特殊形式: if define lambda begin set! let cond and or quote");
                    println!("内置函数: + - * / = > < >= <= not list cons car cdr cadr caddr");
                    println!("命令: :q 退出, :help 帮助");
                    continue;
                }
                _ => {
                    println!("未知命令: {}", input);
                    continue;
                }
            }
        }
        match eval_input(&input, &mut env) {
            Ok(result) => println!("{}", result),
            Err(e) => println!("错误: {}", e),
        }
    }
}
