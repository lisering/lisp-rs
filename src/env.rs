// src/env.rs

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::hash::{BuildHasher, Hasher};
use crate::{LispExp, LispErr};
use crate::interner;

/// FX 哈希器 — 用黄金比例常数做快速搅拌
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    fn new() -> Self {
        FxHasher { hash: 0 }
    }

    fn write_u64(&mut self, i: u64) {
        self.hash = self.hash
            .wrapping_add(i)
            .wrapping_add(0x9e3779b97f4a7c15)
            .rotate_left(5)
            .wrapping_mul(0x9e3779b97f4a7c15);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(buf));
        }
    }
    fn finish(&self) -> u64 { self.hash }
}

/// 工厂类型 — 让 HashMap 能创建 FxHasher 实例
#[derive(Clone, Default)]
pub struct BuildFxHasher;

impl BuildHasher for BuildFxHasher {
    type Hasher = FxHasher;
    fn build_hasher(&self) -> FxHasher { FxHasher::new() }
}

/// 环境 — 就像一个通讯录: 名字 → 值
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LispEnv {
    pub data: HashMap<u64, LispExp, BuildFxHasher>,
    pub outer: Option<Rc<RefCell<LispEnv>>>,
}

impl LispEnv {
    pub fn new() -> Self {
        LispEnv { data: HashMap::default(), outer: None }
    }

    pub fn with_outer(outer: Rc<RefCell<LispEnv>>) -> Self {
        LispEnv { data: HashMap::default(), outer: Some(outer) }
    }

    pub fn set(&mut self, key: u64, value: LispExp) {
        self.data.insert(key, value);
    }

    pub fn get(&self, key: u64) -> Result<LispExp, LispErr> {
        if let Some(v) = self.data.get(&key) {
            return Ok(v.clone());
        }
        if let Some(outer) = &self.outer {
            return outer.borrow().get(key);
        }
        Err(LispErr::Reason(format!("未定义的变量: {}", interner::lookup(key))))
    }

    /// set! — 沿 outer 链找到已有绑定并修改
    pub fn set_upward(&mut self, key: u64, value: LispExp) -> Result<(), LispErr> {
        if let Some(v) = self.data.get_mut(&key) {
            *v = value;
            return Ok(());
        }
        if let Some(outer) = &self.outer {
            return outer.borrow_mut().set_upward(key, value);
        }
        Err(LispErr::Reason(format!("set! 失败: 变量 {} 未定义", interner::lookup(key))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interner;

    #[test]
    fn test_env_set_get() {
        let mut env = LispEnv::new();
        env.set(interner::intern("x"), LispExp::Number(42.0));
        assert_eq!(env.get(interner::intern("x")).unwrap(), LispExp::Number(42.0));
    }

    #[test]
    fn test_env_undefined() {
        let env = LispEnv::new();
        assert!(env.get(interner::intern("y")).is_err());
    }

    #[test]
    fn test_nested_env_lookup() {
        let mut outer = LispEnv::new();
        outer.set(interner::intern("x"), LispExp::Number(10.0));
        let outer_rc = Rc::new(RefCell::new(outer));
        let inner = LispEnv::with_outer(outer_rc);
        assert_eq!(inner.get(interner::intern("x")).unwrap(), LispExp::Number(10.0));
    }
}
