use std::collections::BTreeSet;
use std::net::TcpListener;

use anyhow::{Result, ensure};

/// Port dianggap bebas kalau bind ke loopback berhasil. Ini cukup untuk dev
/// lokal karena semua upstream DevKu hanya mendengar di 127.0.0.1.
pub fn is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn first_free(from: u16, taken: &BTreeSet<u16>) -> Result<u16> {
    for port in from..=u16::MAX {
        if taken.contains(&port) {
            continue;
        }
        if is_free(port) {
            return Ok(port);
        }
    }
    anyhow::bail!("tidak ada port bebas mulai {from}")
}

/// Alokasi satu port per versi PHP. Versi terurut supaya hasilnya deterministik
/// antar start, jadi reload Caddy tidak mengubah upstream.
pub fn allocate(versions: &[String], start: u16) -> Result<Vec<(String, u16)>> {
    ensure!(start > 1024, "port awal harus di atas 1024");

    let mut taken = BTreeSet::new();
    let mut out = Vec::new();
    for version in versions {
        let port = first_free(start, &taken)?;
        taken.insert(port);
        out.push((version.clone(), port));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn allocate_is_deterministic_and_unique() {
        let versions = vec!["7.4.33".to_string(), "8.3.36".to_string()];
        let got = allocate(&versions, 9100).unwrap();
        let map: BTreeMap<_, _> = got.into_iter().collect();
        assert_eq!(map.keys().collect::<Vec<_>>(), vec!["7.4.33", "8.3.36"]);
        let ports: BTreeSet<_> = map.values().collect();
        assert_eq!(ports.len(), 2);
    }

    #[test]
    fn first_free_skips_taken() {
        let mut taken = BTreeSet::new();
        taken.insert(9100);
        taken.insert(9101);
        let port = first_free(9100, &taken).unwrap();
        assert!(port > 9101);
    }

    #[test]
    fn loopback_bind_detects_use() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!is_free(port));
        drop(listener);
        assert!(is_free(port));
    }
}
