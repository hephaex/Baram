//! Internationalization (i18n) support for Baram
//!
//! This module provides multi-language support for CLI messages, errors,
//! and user-facing text. Supported languages: Korean (ko), English (en), Chinese (zh).
//!
//! # Environment Variables
//!
//! - `BARAM_LANG`: Set the preferred language (ko, en, zh). Defaults to English.
//!
//! # Usage
//!
//! ```rust,ignore
//! use baram::i18n::{t, set_locale};
//!
//! // Set language from environment or default
//! set_locale("ko");
//!
//! // Translate a message
//! let msg = t("cli.crawl.starting");
//! ```

use std::sync::OnceLock;

// Note: rust_i18n::i18n! macro is declared in lib.rs (crate root)

static CURRENT_LOCALE: OnceLock<String> = OnceLock::new();

/// Set the current locale for translations
///
/// # Arguments
///
/// * `locale` - Language code (ko, en, zh)
///
/// # Examples
///
/// ```rust,ignore
/// use baram::i18n::set_locale;
///
/// set_locale("ko");
/// ```
pub fn set_locale(locale: &str) {
    let normalized = normalize_locale(locale);
    rust_i18n::set_locale(&normalized);
    CURRENT_LOCALE.get_or_init(|| normalized.clone());
}

/// Get the current locale
///
/// Returns the currently active locale or the default fallback.
pub fn current_locale() -> &'static str {
    CURRENT_LOCALE.get().map(|s| s.as_str()).unwrap_or("en")
}

/// Initialize i18n from environment variables
///
/// Reads `BARAM_LANG` environment variable to set the locale.
/// Falls back to English if not set or invalid.
///
/// # Examples
///
/// ```rust,ignore
/// use baram::i18n::init_from_env;
///
/// init_from_env();
/// ```
pub fn init_from_env() {
    let locale = std::env::var("BARAM_LANG").unwrap_or_else(|_| "en".to_string());
    set_locale(&locale);
}

/// Normalize locale code to supported format
///
/// Converts various locale formats to our standard format:
/// - ko-KR, ko_KR, korean -> ko
/// - en-US, en_US, english -> en
/// - zh-CN, zh_CN, chinese -> zh
fn normalize_locale(locale: &str) -> String {
    let lower = locale.to_lowercase();

    if lower.starts_with("ko") || lower == "korean" {
        "ko".to_string()
    } else if lower.starts_with("zh") || lower == "chinese" {
        "zh".to_string()
    } else {
        "en".to_string()
    }
}

/// Translate a key with optional parameters
///
/// This is a re-export of rust_i18n::t! for convenience.
///
/// # Arguments
///
/// * `key` - Translation key (e.g., "cli.crawl.starting")
/// * `args` - Optional named arguments for interpolation
///
/// # Examples
///
/// ```rust,ignore
/// use baram::i18n::t;
///
/// let msg = t!("cli.crawl.starting");
/// let msg_with_args = t!("cli.crawl.progress", count = 10, total = 100);
/// ```
#[doc(inline)]
pub use rust_i18n::t;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_locale() {
        assert_eq!(normalize_locale("ko"), "ko");
        assert_eq!(normalize_locale("ko-KR"), "ko");
        assert_eq!(normalize_locale("ko_KR"), "ko");
        assert_eq!(normalize_locale("korean"), "ko");

        assert_eq!(normalize_locale("en"), "en");
        assert_eq!(normalize_locale("en-US"), "en");
        assert_eq!(normalize_locale("english"), "en");

        assert_eq!(normalize_locale("zh"), "zh");
        assert_eq!(normalize_locale("zh-CN"), "zh");
        assert_eq!(normalize_locale("chinese"), "zh");

        assert_eq!(normalize_locale("unknown"), "en");
    }

    #[test]
    fn test_set_and_get_locale() {
        set_locale("ko");
        assert_eq!(current_locale(), "ko");

        set_locale("en-US");
        assert_eq!(current_locale(), "en");
    }
}
