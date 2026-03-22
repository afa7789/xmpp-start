use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

pub struct I18n {
    bundle: FluentBundle<FluentResource>,
    locale: String,
}

impl I18n {
    /// Load a bundle from FTL source text for the given locale.
    pub fn from_ftl(locale: &str, ftl_source: &str) -> Result<Self, String> {
        let langid: LanguageIdentifier = locale
            .parse()
            .map_err(|e| format!("invalid locale identifier '{}': {}", locale, e))?;

        let mut bundle = FluentBundle::new(vec![langid]);

        let resource = FluentResource::try_new(ftl_source.to_owned())
            .map_err(|(_, errs)| format!("FTL parse errors: {:?}", errs))?;

        bundle
            .add_resource(resource)
            .map_err(|errs| format!("FTL resource errors: {:?}", errs))?;

        Ok(Self {
            bundle,
            locale: locale.to_owned(),
        })
    }

    /// Get a message by key. Returns the key itself if not found.
    pub fn get(&self, key: &str) -> String {
        let msg = match self.bundle.get_message(key) {
            Some(m) => m,
            None => return key.to_owned(),
        };
        let pattern = match msg.value() {
            Some(p) => p,
            None => return key.to_owned(),
        };
        let mut errors = vec![];
        self.bundle
            .format_pattern(pattern, None, &mut errors)
            .to_string()
    }

    /// Get a message with variables substituted.
    /// `vars` is a list of (name, value) pairs.
    pub fn get_with_args(&self, key: &str, vars: &[(&str, &str)]) -> String {
        let msg = match self.bundle.get_message(key) {
            Some(m) => m,
            None => return key.to_owned(),
        };
        let pattern = match msg.value() {
            Some(p) => p,
            None => return key.to_owned(),
        };

        let mut args = FluentArgs::new();
        for (name, value) in vars {
            args.set(*name, FluentValue::from(*value));
        }

        let mut errors = vec![];
        self.bundle
            .format_pattern(pattern, Some(&args), &mut errors)
            .to_string()
    }

    /// Current locale string (e.g. "en-US").
    pub fn locale(&self) -> &str {
        &self.locale
    }
}

/// Build a default English bundle from the embedded FTL file.
pub fn default_bundle() -> I18n {
    I18n::from_ftl("en-US", include_str!("../../locales/en-US.ftl"))
        .expect("built-in en-US.ftl must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en_bundle() -> I18n {
        default_bundle()
    }

    #[test]
    fn get_returns_message_for_known_key() {
        let i18n = en_bundle();
        assert_eq!(i18n.get("login-title"), "Sign In");
    }

    #[test]
    fn get_returns_key_for_unknown_key() {
        let i18n = en_bundle();
        assert_eq!(i18n.get("does-not-exist"), "does-not-exist");
    }

    #[test]
    fn get_with_args_substitutes_variable() {
        let i18n = en_bundle();
        // Fluent wraps interpolated values in FSI/PDI isolation marks.
        let result = i18n.get_with_args("error-connection-failed", &[("reason", "timeout")]);
        assert!(
            result.contains("timeout"),
            "expected 'timeout' in result, got: {:?}",
            result
        );
        assert!(
            result.starts_with("Connection failed:"),
            "expected result to start with 'Connection failed:', got: {:?}",
            result
        );
    }

    #[test]
    fn login_connected_substitutes_jid() {
        let i18n = en_bundle();
        let result = i18n.get_with_args("login-connected", &[("jid", "alice@example.com")]);
        assert!(
            result.contains("alice@example.com"),
            "expected JID in result, got: {:?}",
            result
        );
        assert!(
            result.starts_with("Connected as"),
            "expected result to start with 'Connected as', got: {:?}",
            result
        );
    }

    #[test]
    fn default_bundle_loads_successfully() {
        let i18n = default_bundle();
        // Spot-check a few keys to confirm the whole file loaded.
        assert_eq!(i18n.get("app-name"), "XMPP Messenger");
        assert_eq!(i18n.get("chat-send-button"), "Send");
        assert_eq!(i18n.get("error-auth-failed"), "Authentication failed");
    }

    #[test]
    fn locale_returns_correct_string() {
        let i18n = en_bundle();
        assert_eq!(i18n.locale(), "en-US");

        let pt = I18n::from_ftl("pt-BR", include_str!("../../locales/pt-BR.ftl"))
            .expect("pt-BR.ftl must be valid");
        assert_eq!(pt.locale(), "pt-BR");
    }
}
