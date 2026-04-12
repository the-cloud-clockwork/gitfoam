use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024; // 2 MiB per file
const ENTROPY_MIN_LEN: usize = 20;
const ENTROPY_THRESHOLD: f64 = 4.5;

fn rules() -> &'static [Regex] {
    static RULES: OnceLock<Vec<Regex>> = OnceLock::new();
    RULES.get_or_init(|| {
        let patterns: &[&str] = &[
            r"AKIA[0-9A-Z]{16}",
            r"ASIA[0-9A-Z]{16}",
            r"ghp_[A-Za-z0-9]{36,}",
            r"ghs_[A-Za-z0-9]{36,}",
            r"gho_[A-Za-z0-9]{36,}",
            r"ghu_[A-Za-z0-9]{36,}",
            r"github_pat_[A-Za-z0-9_]{40,}",
            r"xox[baprs]-[A-Za-z0-9-]{10,}",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
            r"eyJ[A-Za-z0-9_=-]+\.[A-Za-z0-9_=-]+\.[A-Za-z0-9_.+/=-]+",
            r"(?i)(password|passwd|pwd|api[_-]?key|secret|token|auth[_-]?token|access[_-]?key)\s*[:=]\s*['\x22]?[A-Za-z0-9+/=_\-]{16,}['\x22]?",
            r"sk-[A-Za-z0-9]{32,}",
            r"sk-ant-[A-Za-z0-9\-_]{20,}",
            r"AIza[0-9A-Za-z\-_]{35}",
        ];
        patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect()
    })
}

fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    let bytes = s.as_bytes();
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    let mut h = 0.0;
    for &c in counts.iter() {
        if c > 0 {
            let p = c as f64 / len;
            h -= p * p.log2();
        }
    }
    h
}

fn high_entropy_token(line: &str) -> bool {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '_' || c == '-'))
        .any(|tok| {
            tok.len() >= ENTROPY_MIN_LEN && shannon_entropy(tok) >= ENTROPY_THRESHOLD
        })
}

pub fn scan_file(path: &Path) -> Result<Option<String>> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    if !meta.is_file() {
        return Ok(None);
    }
    if meta.len() > MAX_SCAN_BYTES as u64 {
        return Ok(None);
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    // Skip binary files (NUL byte heuristic)
    if bytes.contains(&0u8) {
        return Ok(None);
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    for re in rules() {
        if re.is_match(text) {
            return Ok(Some(format!("regex:{}", re.as_str())));
        }
    }
    for line in text.lines() {
        if line.len() < ENTROPY_MIN_LEN {
            continue;
        }
        if high_entropy_token(line) {
            return Ok(Some("entropy".into()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detects_aws_key() {
        let mut f = tempfile_like("aws");
        writeln!(f.1, "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE").unwrap();
        let hit = scan_file(&f.0).unwrap();
        assert!(hit.is_some());
    }

    #[test]
    fn passes_clean_file() {
        let mut f = tempfile_like("clean");
        writeln!(f.1, "hello world\nthis is fine").unwrap();
        let hit = scan_file(&f.0).unwrap();
        assert!(hit.is_none());
    }

    fn tempfile_like(name: &str) -> (std::path::PathBuf, fs::File) {
        let p = std::env::temp_dir().join(format!("gitfoam-test-{}-{}", name, std::process::id()));
        let f = fs::File::create(&p).unwrap();
        (p, f)
    }
}
