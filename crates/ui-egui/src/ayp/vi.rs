//! Vietnamese labels, search synonyms and error explanations for the AYP panels (`vi.tsv`).
use crate::i18n::catalog::Catalog;
use std::collections::HashMap;
use std::sync::OnceLock;

static CAT: OnceLock<Catalog> = OnceLock::new();
static SYN: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
static ERR: OnceLock<Vec<(String, String)>> = OnceLock::new();
const TSV: &str = include_str!("vi.tsv");

fn cat() -> &'static Catalog {
    CAT.get_or_init(|| Catalog::parse(TSV))
}

/// Vietnamese text for an English label; the input itself when there is no row.
pub fn vi(english: &str) -> &str {
    cat().plain(english).unwrap_or(english)
}

pub fn has(english: &str) -> bool {
    cat().plain(english).is_some()
}

/// Extra search words for a command (from `syn` rows).
pub fn synonyms(english: &str) -> Vec<&'static str> {
    let m = SYN.get_or_init(|| {
        let mut m: HashMap<String, Vec<String>> = HashMap::new();
        for (ctx, src, tr) in crate::i18n::catalog::parse_entries(TSV).0 {
            if ctx == "syn" {
                m.entry(src).or_default().extend(tr.split('|').map(str::to_string));
            }
        }
        m
    });
    m.get(english).map(|v| v.iter().map(String::as_str).collect()).unwrap_or_default()
}

/// Explain an engine error in Vietnamese; unknown errors are returned unchanged.
pub fn reason(engine_error: &str) -> String {
    let rows = ERR.get_or_init(|| {
        crate::i18n::catalog::parse_entries(TSV)
            .0
            .into_iter()
            .filter(|(c, _, _)| c == "err")
            .map(|(_, s, t)| (s.to_lowercase(), t))
            .collect()
    });
    let low = engine_error.to_lowercase();
    rows.iter().find(|(s, _)| low.contains(s.as_str())).map(|(_, t)| t.clone()).unwrap_or_else(|| engine_error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_labels_are_vietnamese() {
        assert_eq!(vi("Add Edit"), "Tách");
        assert_eq!(vi("Add Frame Hold"), "Dừng hình");
        assert_eq!(vi("no such label"), "no such label");
    }
    #[test]
    fn synonyms_are_listed() {
        assert!(synonyms("Remove Pauses").contains(&"khoảng lặng"));
    }
    #[test]
    fn errors_are_explained() {
        assert!(reason("select clips first").contains("chọn"));
    }
    #[test]
    fn tsv_parses_cleanly() {
        assert!(crate::i18n::catalog::parse_entries(TSV).1.is_empty());
    }
    #[test]
    fn every_menu_command_has_a_row() {
        let missing: Vec<_> = filmcraft_engine::command_specs()
            .iter()
            .filter(|c| !c.menu.is_empty() && !has(c.label))
            .map(|c| c.label)
            .collect();
        assert!(missing.is_empty(), "{missing:#?}");
    }
}
