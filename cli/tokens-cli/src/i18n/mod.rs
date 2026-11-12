//! Terminal localization.
//!
//! Three languages, resolved once at startup from (`--lang`), then
//! `TOKENS_LANG`, then `settings.json`, then English. Translation is opt-in:
//! the POSIX locale (`LANG`, `LC_ALL`) is never read, so output people script
//! against does not change under them because of a system setting.
//! The catalogue is compiled in — `include_str!` per language and domain — so
//! a run costs no file reads and the binary cannot be started without it.
//!
//! Messages are keyed, not looked up by English text: a translated string may
//! be edited without touching a call site, and a missing key falls back to
//! English here rather than printing a key at a user.
//!
//! Domains are separate files (`cli`, `top`, `commands`) purely so parallel
//! work does not converge on one file. They are merged at load.
//!
//! Formatting stays with the caller:
//!
//! ```ignore
//! println!("  {}", t!("auth.requesting").bright_black());
//! println!("{}", tr!("auth.failed", error = message));
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// A language the terminal output can be rendered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    ZhCn,
    Ja,
}

impl Lang {
    /// Parse a BCP 47 tag or a plain language code, matching by prefix so
    /// `zh-Hans-CN` and `ja_JP.UTF-8` both land.
    ///
    /// `auto` is the flag value that means "keep going down the chain".
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
        let primary = normalized.split(['-', '.']).next()?;

        match primary {
            "en" => Some(Self::En),
            "zh" => Some(Self::ZhCn),
            "ja" => Some(Self::Ja),
            _ => None,
        }
    }

    pub const fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
            Self::Ja => "ja",
        }
    }

    const fn index(self) -> u8 {
        match self {
            Self::En => 0,
            Self::ZhCn => 1,
            Self::Ja => 2,
        }
    }

    const fn from_index(value: u8) -> Self {
        match value {
            1 => Self::ZhCn,
            2 => Self::Ja,
            _ => Self::En,
        }
    }
}

/// Domains, each a directory of `<lang>.json` files on disk.
///
/// Kept as arrays of `(lang, text)` rather than three named constants so the
/// loader cannot drift from the enum: the compiler catches a missing language
/// here, not a user on a plane.
const SOURCES: &[&[(&str, &str); 3]] = &[
    &[
        ("en", include_str!("../../locales/en/cli.json")),
        ("zh-CN", include_str!("../../locales/zh-CN/cli.json")),
        ("ja", include_str!("../../locales/ja/cli.json")),
    ],
    &[
        ("en", include_str!("../../locales/en/top.json")),
        ("zh-CN", include_str!("../../locales/zh-CN/top.json")),
        ("ja", include_str!("../../locales/ja/top.json")),
    ],
    &[
        ("en", include_str!("../../locales/en/commands.json")),
        ("zh-CN", include_str!("../../locales/zh-CN/commands.json")),
        ("ja", include_str!("../../locales/ja/commands.json")),
    ],
];

static CURRENT: AtomicU8 = AtomicU8::new(0);
static TABLES: OnceLock<[HashMap<String, String>; 3]> = OnceLock::new();

fn encode(lang: Lang) -> u8 {
    lang.index()
}

/// Set the active language. Called once from `main`; tests call it to switch.
pub fn set_lang(lang: Lang) {
    CURRENT.store(encode(lang), Ordering::Relaxed);
}

/// Resolve the startup language.
///
/// Order: the `--lang` flag, `TOKENS_LANG`, the stored setting, then English.
/// The flag wins so one run can be in another language without editing a
/// config. Every source is one the user set for this tool on purpose; the
/// POSIX locale is deliberately not one of them, so English stays the default
/// whatever `LANG` or `LC_ALL` say.
pub fn resolve(flag: Option<&str>, stored: Option<&str>) -> Lang {
    resolve_from(flag, std::env::var("TOKENS_LANG").ok().as_deref(), stored)
}

/// [`resolve`] with the environment passed in, so the chain can be tested
/// without mutating process state.
fn resolve_from(flag: Option<&str>, env: Option<&str>, stored: Option<&str>) -> Lang {
    flag.and_then(Lang::parse)
        .or_else(|| env.and_then(Lang::parse))
        .or_else(|| stored.and_then(Lang::parse))
        .unwrap_or_default()
}

pub fn current() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

fn tables() -> &'static [HashMap<String, String>; 3] {
    TABLES.get_or_init(|| {
        let mut tables: [HashMap<String, String>; 3] = Default::default();

        for (index, domain) in SOURCES.iter().enumerate() {
            for (tag, text) in domain.iter() {
                // A malformed catalogue is a build-time mistake, but a panic
                // in a CLI is a lost submission — degrade to the other
                // languages and let `cargo test` catch the file.
                let Some(slot) = Lang::parse(tag).map(|lang| encode(lang) as usize) else {
                    continue;
                };
                if index == 0 {
                    tables[slot] = HashMap::new();
                }
                match serde_json::from_str::<HashMap<String, String>>(text) {
                    Ok(entries) => tables[slot].extend(entries),
                    Err(error) => {
                        debug_assert!(
                            false,
                            "catalogue {tag}/{} is not JSON: {error}",
                            index_name(index)
                        );
                    }
                }
            }
        }

        tables
    })
}

const fn index_name(index: usize) -> &'static str {
    match index {
        0 => "cli",
        1 => "top",
        _ => "commands",
    }
}

/// Translate a key for the active language.
///
/// Falls back to English, then to the key itself. The key coming out is a
/// visible bug rather than a blank line, which is what a translator needs to
/// see.
pub fn t(key: &str) -> &'static str {
    let lang = current();
    let tables = tables();

    if let Some(text) = tables[encode(lang) as usize].get(key) {
        return text;
    }
    if let Some(text) = tables[encode(Lang::En) as usize].get(key) {
        return text;
    }

    // The map owns its keys, so a miss cannot hand back the borrowed key.
    // Leaking one short string per miss is cheaper than a per-call allocation.
    // The key is resolved *before* the assertion so a caller that survives the
    // panic (and every release build) still prints something a translator can
    // act on instead of a blank line.
    let key_out: &'static str = Box::leak(key.to_owned().into_boxed_str());
    debug_assert!(false, "missing translation for `{key}`");
    key_out
}

/// Translate without arguments.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::t($key)
    };
}

/// Translate and interpolate. Placeholders are `{name}` in the catalogue.
///
/// Substitution is a `str::replace` per named argument rather than `format!`,
/// because `format!` needs a literal template and the catalogue hands back a
/// `&'static str` chosen at runtime. The trade is that `{name:?}` debug
/// formatting is unavailable; every value goes through `Display`.
#[macro_export]
macro_rules! tr {
    ($key:literal) => {
        $crate::i18n::t($key).to_owned()
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let __template = $crate::i18n::t($key);
        $(let __template = __template.replace(concat!("{", stringify!($name), "}"), &$value.to_string());)+
        __template
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Mutex;

    /// These tests move the process-wide language, and the rest of the suite
    /// asserts on English text. They run under `#[serial]` — the same lock the
    /// CLI's output-asserting tests and its localized-help test take, so no two
    /// tests can switch the language under each other — and put English back;
    /// a panic still restores through `Drop`.
    static LOCK: Mutex<()> = Mutex::new(());

    struct EnGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnGuard {
        fn take() -> Self {
            let lock = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
            Self { _lock: lock }
        }
    }

    impl Drop for EnGuard {
        fn drop(&mut self) {
            set_lang(Lang::En);
        }
    }

    #[test]
    fn parses_tags_by_prefix() {
        assert_eq!(Lang::parse("zh-CN"), Some(Lang::ZhCn));
        assert_eq!(Lang::parse("zh_Hans"), Some(Lang::ZhCn));
        assert_eq!(Lang::parse("ja_JP.UTF-8"), Some(Lang::Ja));
        assert_eq!(Lang::parse("en-GB"), Some(Lang::En));
        assert_eq!(Lang::parse("de-DE"), None);
        assert_eq!(Lang::parse(""), None);
    }

    #[test]
    #[serial]
    fn english_is_the_default() {
        let _guard = EnGuard::take();
        assert_eq!(Lang::default(), Lang::En);
        assert_eq!(Lang::from_index(99), Lang::En);
    }

    #[test]
    fn only_explicit_sources_choose_a_language() {
        // Nothing set: English, whatever the system locale is.
        assert_eq!(resolve_from(None, None, None), Lang::En);
        // `auto` and junk fall through rather than picking something.
        assert_eq!(resolve_from(Some("auto"), None, None), Lang::En);
        assert_eq!(resolve_from(Some("de"), None, None), Lang::En);
        // Each explicit source works on its own...
        assert_eq!(resolve_from(Some("ja"), None, None), Lang::Ja);
        assert_eq!(resolve_from(None, Some("zh-CN"), None), Lang::ZhCn);
        assert_eq!(resolve_from(None, None, Some("ja")), Lang::Ja);
        // ...and the flag beats the env, which beats the setting.
        assert_eq!(resolve_from(Some("en"), Some("ja"), Some("zh-CN")), Lang::En);
        assert_eq!(resolve_from(None, Some("ja"), Some("zh-CN")), Lang::Ja);
    }

    /// In a debug build a miss trips the assertion, so it is loud in
    /// development and the suite pins that.
    #[test]
    #[serial]
    #[should_panic(expected = "missing translation")]
    fn a_missing_key_is_loud_in_development() {
        let _guard = EnGuard::take();
        set_lang(Lang::ZhCn);
        let _ = t("__definitely_absent__");
    }

    /// A release build (`debug_assert!` compiled out) has to keep going:
    /// a missing key prints the key rather than an empty line.
    #[test]
    #[serial]
    #[cfg(not(debug_assertions))]
    fn a_missing_key_falls_back_to_the_key() {
        let _guard = EnGuard::take();
        set_lang(Lang::ZhCn);
        assert_eq!(t("__definitely_absent__"), "__definitely_absent__");
        set_lang(Lang::En);
    }
}
