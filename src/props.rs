//! Operator properties.
use anyhow::{Result, anyhow, bail};
use tercen_rs::PropertyReader;
use tercen_rs::context::ContextBase;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Two-sided confidence level for the CCC interval and the Bland–Altman limits.
    pub confidence_level: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            confidence_level: 0.95,
        }
    }
}

/// Read the properties off the task's operator settings.
///
/// Tercen serialises a numeric property as text (e.g. "0.95"), so every number is parsed as
/// f64; a value that does not parse is an error, never a silent default.
pub fn settings_from_ctx(ctx: &ContextBase) -> Result<Settings> {
    let pr = PropertyReader::from_operator_settings(ctx.operator_settings());
    parse(&pr.get_string("confidence_level", "0.95"))
}

/// The parsing and validation, separate from the context so it can be tested.
pub fn parse(confidence_level: &str) -> Result<Settings> {
    let raw = confidence_level.trim();
    let level: f64 = if raw.is_empty() {
        Settings::default().confidence_level
    } else {
        raw.parse()
            .map_err(|_| anyhow!("property 'confidence_level' is not a number: '{raw}'"))?
    };
    if !(level > 0.0 && level < 1.0) {
        bail!("property 'confidence_level' must be strictly between 0 and 1, got {level}");
    }
    Ok(Settings {
        confidence_level: level,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_parsing() {
        assert_eq!(parse("").unwrap().confidence_level, 0.95);
        assert_eq!(parse(" 0.99 ").unwrap().confidence_level, 0.99);
        assert!(parse("1").is_err());
        assert!(parse("0").is_err());
        assert!(parse("abc").is_err());
    }
}
