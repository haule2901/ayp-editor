//! Secrets in the macOS Keychain (generic passwords, safe `security-framework` API).
//! Elsewhere there is no store: reads find nothing and writes say so.

/// The secret saved under `service` / `account`, or `None` when absent or unreadable.
pub fn secret_get(service: &str, account: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let bytes = security_framework::passwords::get_generic_password(service, account).ok()?;
        String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (service, account);
        None
    }
}

/// Save `value` under `service` / `account`; an empty value deletes the item.
pub fn secret_set(service: &str, account: &str, value: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use security_framework::passwords::{delete_generic_password, set_generic_password};
        if value.is_empty() {
            // Deleting what is not there is fine.
            let _ = delete_generic_password(service, account);
            return Ok(());
        }
        set_generic_password(service, account, value.as_bytes()).map_err(|e| format!("Keychain: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (service, account, value);
        Err("Keychain chỉ có trên macOS".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_round_trip() {
        const S: &str = "vn.ayp.editor.test";
        const A: &str = "round-trip";
        // Delete at the end even when an assertion fails.
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = secret_set(S, A, "");
            }
        }
        let _cleanup = Cleanup;
        let _ = secret_set(S, A, "");
        assert_eq!(secret_get(S, A), None);
        assert_eq!(secret_set(S, A, "sk-test-value"), Ok(()));
        assert_eq!(secret_get(S, A).as_deref(), Some("sk-test-value"));
        assert_eq!(secret_set(S, A, "second"), Ok(()));
        assert_eq!(secret_get(S, A).as_deref(), Some("second"));
        assert_eq!(secret_set(S, A, ""), Ok(()));
        assert_eq!(secret_get(S, A), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn keychain_is_macos_only() {
        assert_eq!(secret_get("s", "a"), None);
        assert!(secret_set("s", "a", "v").is_err());
    }
}
