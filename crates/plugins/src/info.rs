//! What a plug-in says about itself: its id and name, checked before they reach the picker or
//! the preferences file.

use tunebox_visualizer_sdk::abi::{MAX_ID_BYTES, MAX_NAME_CHARS};

/// A plug-in look's id and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookInfo {
    /// Lowercase ASCII letters, digits and `-`.
    pub id: String,
    pub name: String,
}

/// Checks the id and name a plug-in reported (as raw bytes from its memory).
pub fn check(id: &[u8], name: &[u8]) -> Result<LookInfo, String> {
    let id_ok = !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    if !id_ok {
        return Err(format!(
            "its id must be 1 to {MAX_ID_BYTES} lowercase letters, digits and hyphens, but is “{}”",
            String::from_utf8_lossy(id).chars().take(MAX_ID_BYTES).collect::<String>()
        ));
    }
    let name = std::str::from_utf8(name).map_err(|_| "its name is not UTF-8 text".to_owned())?;
    let name = name.trim();
    let count = name.chars().count();
    if count == 0 || count > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(format!("its name must be 1 to {MAX_NAME_CHARS} characters of plain text"));
    }
    Ok(LookInfo { id: String::from_utf8_lossy(id).into_owned(), name: name.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_ids_and_names() {
        let info = check(b"lava-lamp-2", " Lava Lamp ".as_bytes()).unwrap();
        assert_eq!(info, LookInfo { id: "lava-lamp-2".into(), name: "Lava Lamp".into() });
        assert!(check(b"ok", "Ünïcödé ✓".as_bytes()).is_ok());
    }

    #[test]
    fn refuses_odd_ids_and_names() {
        for id in [&b""[..], b"Upper", b"has space", b"dots.no", b"../escape", &[b'a'; MAX_ID_BYTES + 1]] {
            assert!(check(id, b"Name").is_err(), "{id:?}");
        }
        for name in [&b""[..], b"   ", b"tab\there", &[0xff, 0xfe], "x".repeat(MAX_NAME_CHARS + 1).as_bytes()] {
            assert!(check(b"id", name).is_err(), "{name:?}");
        }
    }
}
