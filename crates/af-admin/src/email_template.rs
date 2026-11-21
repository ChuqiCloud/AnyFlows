use std::collections::BTreeMap;

use zeroize::Zeroizing;

use crate::AdminEmailSettingsError;

const MAX_TEMPLATE_BYTES: usize = 16_384;
const MAX_VARIABLES: usize = 64;
const MAX_HTML_BYTES: usize = 256 * 1_024;

#[derive(Clone, Copy)]
pub(super) enum EmailTemplateKind {
    SmtpTest,
    Registration,
    EmailBinding,
    PasswordReset,
    BalanceAlert,
    SubscriptionBalanceAlert,
}

impl EmailTemplateKind {
    const ALL: [Self; 6] = [
        Self::SmtpTest,
        Self::Registration,
        Self::EmailBinding,
        Self::PasswordReset,
        Self::BalanceAlert,
        Self::SubscriptionBalanceAlert,
    ];

    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::SmtpTest => "smtp_test",
            Self::Registration => "registration",
            Self::EmailBinding => "email_binding",
            Self::PasswordReset => "password_reset",
            Self::BalanceAlert => "balance_alert",
            Self::SubscriptionBalanceAlert => "subscription_balance_alert",
        }
    }

    const fn variables(self) -> &'static [&'static str] {
        match self {
            Self::SmtpTest => &[],
            Self::Registration | Self::EmailBinding => &["code", "expires_minutes"],
            Self::PasswordReset => &["site_name", "reset_url", "expires_minutes"],
            Self::BalanceAlert => &["site_name", "username", "balance", "threshold"],
            Self::SubscriptionBalanceAlert => &[
                "site_name",
                "username",
                "plan_name",
                "remaining",
                "quota",
                "threshold_percent",
                "window_end",
            ],
        }
    }

    const fn required(self) -> Option<&'static str> {
        match self {
            Self::Registration | Self::EmailBinding => Some("code"),
            Self::PasswordReset => Some("reset_url"),
            _ => None,
        }
    }
}

/// 空字符串清除对应模板。保存前校验变量和登录相关邮件的必要内容。
pub(super) fn validate_templates(
    templates: &mut BTreeMap<String, String>,
) -> Result<(), AdminEmailSettingsError> {
    if templates.len() > EmailTemplateKind::ALL.len() {
        return Err(AdminEmailSettingsError::InvalidInput);
    }
    for (key, source) in templates {
        let kind = EmailTemplateKind::ALL
            .into_iter()
            .find(|kind| kind.key() == key)
            .ok_or(AdminEmailSettingsError::InvalidInput)?;
        if source.len() > MAX_TEMPLATE_BYTES
            || source
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
        {
            return Err(AdminEmailSettingsError::InvalidInput);
        }
        if source.trim().is_empty() {
            source.clear();
            continue;
        }
        let mut required_found = kind.required().is_none();
        let valid = visit_template(source, |_, name| {
            if name == Some("body") || name.is_some_and(|name| Some(name) == kind.required()) {
                required_found = true;
            }
            name.is_none_or(|name| {
                matches!(name, "subject" | "body" | "recipient") || kind.variables().contains(&name)
            })
        });
        if !valid || !required_found {
            return Err(AdminEmailSettingsError::InvalidInput);
        }
    }
    Ok(())
}

pub(super) fn render(
    source: &str,
    subject: &str,
    body: &str,
    recipient: &str,
    values: &[(&str, &str)],
) -> Option<Zeroizing<String>> {
    if source.len() > MAX_TEMPLATE_BYTES || source.trim().is_empty() {
        return None;
    }
    let mut html = Zeroizing::new(String::with_capacity(source.len()));
    let rendered = visit_template(source, |literal, name| {
        html.push_str(literal);
        if let Some(name) = name {
            let value = match name {
                "subject" => subject,
                "body" => body,
                "recipient" => recipient,
                _ => {
                    let Some((_, value)) = values.iter().find(|(key, _)| *key == name) else {
                        return false;
                    };
                    value
                }
            };
            push_escaped(&mut html, value);
        }
        html.len() <= MAX_HTML_BYTES
    });
    rendered.then_some(html)
}

fn visit_template(mut source: &str, mut visit: impl FnMut(&str, Option<&str>) -> bool) -> bool {
    for _ in 0..MAX_VARIABLES {
        let Some((literal, remainder)) = source.split_once("{{") else {
            return visit(source, None);
        };
        let Some((name, rest)) = remainder.split_once("}}") else {
            return false;
        };
        if !visit(literal, Some(name.trim())) {
            return false;
        }
        source = rest;
    }
    !source.contains("{{") && visit(source, None)
}

fn push_escaped(html: &mut String, value: &str) {
    for character in value.chars() {
        html.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#39;",
            '`' => "&#96;",
            '=' => "&#61;",
            ' ' => "&#32;",
            '\t' => "&#9;",
            '\n' => "&#10;",
            '\r' => "&#13;",
            _ => {
                html.push(character);
                continue;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_scoped_variables_and_required_authentication_content() {
        for (kind, source, accepted) in [
            ("registration", "<b>{{ code }}</b>", true),
            ("email_binding", "<p>{{body}}</p>", true),
            ("password_reset", "<a href=\"{{reset_url}}\">重置</a>", true),
            ("password_reset", "<p>{{subject}}</p>", false),
            ("balance_alert", "<p>{{reset_url}}</p>", false),
            ("smtp_test", "<p>{{body</p>", false),
            ("other", "", false),
            ("registration", "  ", true),
        ] {
            let mut templates = BTreeMap::from([(kind.to_owned(), source.to_owned())]);
            assert_eq!(
                validate_templates(&mut templates).is_ok(),
                accepted,
                "{kind}: {source}"
            );
        }
    }

    #[test]
    fn escapes_values_once_and_preserves_email_markup() {
        let html = render(
            "<p>{{ username }}</p><a href=\"{{reset_url}}\">{{subject}}</a>",
            "{{body}} & hello",
            "plain text",
            "member@example.com",
            &[
                ("username", r#"<img src=x onerror="boom">"#),
                ("reset_url", "https://example.com/reset?token=a&other=b"),
            ],
        )
        .unwrap();
        assert_eq!(
            html.as_str(),
            "<p>&lt;img&#32;src&#61;x&#32;onerror&#61;&quot;boom&quot;&gt;</p><a href=\"https://example.com/reset?token&#61;a&amp;other&#61;b\">{{body}}&#32;&amp;&#32;hello</a>"
        );
        assert!(render("{{missing}}", "", "", "", &[]).is_none());
    }
}
