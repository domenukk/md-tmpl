//! Compatibility shims for `no_std` / `std` mode.
//!
//! Re-exports [`HashMap`], [`HashSet`], and [`Lazy`] from either `std` or
//! `hashbrown`/`spin` depending on the active feature set. All other types
//! (`Arc`, `Cow`, `Vec`, `String`, `fmt`, etc.) are available via `alloc`
//! and `core` regardless of mode.

// -- HashMap / HashSet --------------------------------------------------------

// Always use hashbrown's HashMap/HashSet — its default hasher (foldhash/ahash)
// is ~3× faster than SipHash for short keys like template field names.
// Template data is not adversarial, so DOS-resistant hashing is unnecessary.
// -- Lazy static --------------------------------------------------------------
/// Lazy initializer — [`std::sync::LazyLock`] under `std`,
/// [`spin::LazyLock`] under `no_std`.
#[cfg(feature = "std")]
pub use std::sync::LazyLock as Lazy;

#[cfg(feature = "serde")]
pub(crate) use hashbrown::hash_map;
pub(crate) use hashbrown::{HashMap, HashSet};
/// Lazy initializer — [`std::sync::LazyLock`] under `std`,
/// [`spin::LazyLock`] under `no_std`.
#[cfg(not(feature = "std"))]
pub use spin::LazyLock as Lazy;

#[cfg(test)]
mod tests {
    use alloc::{
        string::{String, ToString},
        vec,
    };

    use super::*;

    // -- HashMap tests --------------------------------------------------------

    #[test]
    fn hashmap_insert_and_get() {
        let mut m = HashMap::new();
        m.insert("key".to_string(), 42);
        assert_eq!(m.get("key"), Some(&42));
    }

    #[test]
    fn hashmap_default_is_empty() {
        let m: HashMap<String, i32> = HashMap::default();
        assert!(m.is_empty());
    }

    #[test]
    fn hashmap_overwrite_key() {
        let mut m = HashMap::new();
        m.insert("k".to_string(), 1);
        m.insert("k".to_string(), 2);
        assert_eq!(m["k"], 2);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn hashmap_remove() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), 10);
        assert_eq!(m.remove("a"), Some(10));
        assert!(m.is_empty());
    }

    #[test]
    fn hashmap_contains_key() {
        let mut m = HashMap::new();
        m.insert("present".to_string(), ());
        assert!(m.contains_key("present"));
        assert!(!m.contains_key("absent"));
    }

    #[test]
    fn hashmap_iter() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), 1);
        m.insert("b".to_string(), 2);
        let mut keys: vec::Vec<&String> = m.keys().collect();
        keys.sort();
        assert_eq!(keys, vec![&"a".to_string(), &"b".to_string()]);
    }

    // -- HashSet tests --------------------------------------------------------

    #[test]
    fn hashset_insert_and_contains() {
        let mut s = HashSet::new();
        assert!(s.insert("hello".to_string()));
        assert!(s.contains("hello"));
        assert!(!s.contains("world"));
    }

    #[test]
    fn hashset_default_is_empty() {
        let s: HashSet<String> = HashSet::default();
        assert!(s.is_empty());
    }

    #[test]
    fn hashset_no_duplicates() {
        let mut s = HashSet::new();
        assert!(s.insert("x".to_string()));
        assert!(!s.insert("x".to_string()));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn hashset_remove() {
        let mut s = HashSet::new();
        s.insert("item".to_string());
        assert!(s.remove("item"));
        assert!(!s.contains("item"));
    }

    // -- Lazy tests -----------------------------------------------------------

    #[test]
    fn lazy_initializes_on_first_access() {
        static COUNTER: Lazy<i32> = Lazy::new(|| 42);
        assert_eq!(*COUNTER, 42);
    }

    #[test]
    fn lazy_returns_same_value_on_subsequent_access() {
        static VAL: Lazy<String> = Lazy::new(|| String::from("initialized"));
        assert_eq!(&*VAL, "initialized");
        assert_eq!(&*VAL, "initialized"); // second access
    }

    #[test]
    fn lazy_with_vec() {
        static ITEMS: Lazy<vec::Vec<i32>> = Lazy::new(|| vec![1, 2, 3]);
        assert_eq!(ITEMS.len(), 3);
        assert_eq!(ITEMS[0], 1);
    }
}
