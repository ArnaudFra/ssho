//! Interface language. English is the default; French is the other shipped
//! language. A text is written once, next to its translation, at the place it
//! is displayed: `t!("Save", "Enregistrer")`, `tf!("{n} hosts", "{n} hôtes")`.
use serde::{Deserialize, Deserializer, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Fr,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::En, Self::Fr];

    /// Value stored in the preferences.
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }

    /// The language named in itself, so it can be found whatever is displayed.
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Fr => "Français",
        }
    }

    /// Lenient: `fr`, `FR`, `fr_FR.UTF-8` or `français` select French; anything
    /// else, including an unknown language, keeps the English default.
    pub fn parse(text: &str) -> Self {
        let text = text.trim().to_lowercase();
        if text.starts_with("fr") {
            Self::Fr
        } else {
            Self::En
        }
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A null or unknown value never prevents the preferences from loading.
        Ok(Option::<String>::deserialize(deserializer)?
            .map(|text| Self::parse(&text))
            .unwrap_or_default())
    }
}

thread_local! {
    // The interface runs on one thread; each test thread starts in English.
    static CURRENT: Cell<Language> = const { Cell::new(Language::En) };
}

pub fn set(language: Language) {
    CURRENT.with(|current| current.set(language));
}

pub fn current() -> Language {
    CURRENT.with(Cell::get)
}

pub fn french() -> bool {
    current() == Language::Fr
}

/// One text in the current language: `t!("English", "Français")`.
#[macro_export]
macro_rules! t {
    ($en:expr, $fr:expr $(,)?) => {
        if $crate::i18n::french() {
            $fr
        } else {
            $en
        }
    };
}

/// A formatted text in the current language. Both templates take the same
/// arguments: `tf!("{n} hosts", "{n} hôtes")`, `tf!("File: {}", "Fichier : {}", path)`.
#[macro_export]
macro_rules! tf {
    ($en:literal, $fr:literal $(, $($arg:tt)*)?) => {
        if $crate::i18n::french() {
            format!($fr $(, $($arg)*)?)
        } else {
            format!($en $(, $($arg)*)?)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default_and_parsing_is_lenient() {
        assert_eq!(current(), Language::En);
        assert_eq!(t!("Save", "Enregistrer"), "Save");
        for french in ["fr", "FR", " fr_FR.UTF-8 ", "français", "French"] {
            assert_eq!(Language::parse(french), Language::Fr, "{french}");
        }
        for english in ["en", "EN_us", "", "de", "klingon"] {
            assert_eq!(Language::parse(english), Language::En, "{english}");
        }
        set(Language::Fr);
        assert_eq!(t!("Save", "Enregistrer"), "Enregistrer");
        let n = 3;
        assert_eq!(tf!("{n} hosts", "{n} hôtes"), "3 hôtes");
        assert_eq!(tf!("File: {}", "Fichier : {}", "x"), "Fichier : x");
        set(Language::En);
        assert_eq!(tf!("File: {}", "Fichier : {}", "x"), "File: x");
    }

    #[test]
    fn preference_is_read_leniently_and_written_as_a_code() {
        let read = |json: &str| serde_json::from_str::<Language>(json).unwrap();
        assert_eq!(read("\"fr\""), Language::Fr);
        assert_eq!(read("\"Français\""), Language::Fr);
        assert_eq!(read("null"), Language::En);
        assert_eq!(read("\"xx\""), Language::En);
        assert_eq!(serde_json::to_string(&Language::Fr).unwrap(), "\"fr\"");
        assert_eq!(Language::ALL.map(Language::code), ["en", "fr"]);
    }
}
