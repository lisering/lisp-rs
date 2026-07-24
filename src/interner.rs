// src/interner.rs
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

struct Interner {
    id_to_str: Vec<String>,
    str_to_id: HashMap<String, u64>,
}

impl Interner {
    fn new() -> Self {
        Interner {
            id_to_str: Vec::new(),
            str_to_id: HashMap::new(),
        }
    }

    fn intern(&mut self, s: &str) -> u64 {
        if let Some(&id) = self.str_to_id.get(s) { return id; }
        let id = self.id_to_str.len() as u64;
        self.id_to_str.push(s.to_string());
        self.str_to_id.insert(s.to_string(), id);
        id
    }

    fn lookup(&self, id: u64) -> String {
        self.id_to_str.get(id as usize)
            .cloned()
            .unwrap_or_else(|| format!("<unknown:{}>", id))
    }
}

static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();

pub fn intern(s: &str) -> u64 {
    let mut interner = INTERNER
        .get_or_init(|| RwLock::new(Interner::new()))
        .write().unwrap();
    interner.intern(s)
}

pub fn lookup(id: u64) -> String {
    let interner = INTERNER
        .get_or_init(|| RwLock::new(Interner::new()))
        .read().unwrap();
    interner.lookup(id)
}

/// 预定义符号集合 — 一次性 intern 所有特殊形式符号
pub struct PredefinedSyms {
    pub if_sym: u64,
    pub define: u64,
    pub lambda: u64,
    pub begin: u64,
    pub set_bang: u64,
    pub let_sym: u64,
    pub cond_sym: u64,
    pub and_sym: u64,
    pub or_sym: u64,
    pub let_star: u64,
    pub letrec: u64,
    pub quote: u64,
    pub defmacro: u64,
    pub quasiquote: u64,
    pub unquote: u64,
    pub unquote_splicing: u64,
}

pub fn predefined() -> PredefinedSyms {
    PredefinedSyms {
        if_sym: intern("if"),
        define: intern("define"),
        lambda: intern("lambda"),
        begin: intern("begin"),
        set_bang: intern("set!"),
        let_sym: intern("let"),
        cond_sym: intern("cond"),
        and_sym: intern("and"),
        or_sym: intern("or"),
        let_star: intern("let*"),
        letrec: intern("letrec"),
        quote: intern("quote"),
        defmacro: intern("defmacro"),
        quasiquote: intern("quasiquote"),
        unquote: intern("unquote"),
        unquote_splicing: intern("unquote-splicing"),
    }
}
