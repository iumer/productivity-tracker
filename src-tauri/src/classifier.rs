use crate::db::CategoryRule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    pub category_id: String,
    pub category_label: String,
    pub display: String,
}

fn rule_matches_app(rule: &CategoryRule, app_l: &str) -> bool {
    rule.match_apps.is_empty()
        || rule
            .match_apps
            .iter()
            .any(|a| app_l.contains(&a.to_lowercase()))
}

fn rule_content_ok(rule: &CategoryRule, title_l: &str, url_l: &str) -> bool {
    let title_ok = rule.title_contains.is_empty()
        || rule
            .title_contains
            .iter()
            .any(|t| title_l.contains(&t.to_lowercase()));

    let url_ok = rule.url_contains.is_empty()
        || rule
            .url_contains
            .iter()
            .any(|u| url_l.contains(&u.to_lowercase()));

    if !rule.url_contains.is_empty() && !rule.title_contains.is_empty() {
        title_ok || url_ok
    } else if !rule.url_contains.is_empty() {
        url_ok
    } else if !rule.title_contains.is_empty() {
        title_ok
    } else {
        true
    }
}

/// Stable id for an auto-discovered app, e.g. `app-msteams`.
pub fn auto_app_id(app_name: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in app_name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "app-unknown".into()
    } else {
        format!("app-{slug}")
    }
}

pub fn friendly_app_label(app_name: &str) -> String {
    let lower = app_name.to_lowercase();
    match lower.as_str() {
        "msteams" | "microsoft teams" | "teams" => "Teams".into(),
        "microsoft excel" | "excel" => "Excel".into(),
        "microsoft word" | "word" => "Word".into(),
        "microsoft powerpoint" | "powerpoint" => "PowerPoint".into(),
        "microsoft outlook" | "outlook" => "Outlook".into(),
        "google chrome" | "chrome" => "Chrome".into(),
        "spotify" => "Spotify".into(),
        "finder" => "Finder".into(),
        "activity monitor" => "Activity Monitor".into(),
        "system settings" | "system preferences" => "System Settings".into(),
        "passwords" => "Passwords".into(),
        _ => app_name.to_string(),
    }
}

/// Transient / helper processes — keep under Other, never auto-name them.
pub fn is_noise_app(app_name: &str) -> bool {
    let lower = app_name.trim().to_lowercase();
    if lower.is_empty() {
        return true;
    }
    const EXACT: &[&str] = &[
        "app_mode_loader",
        "microsoft error reporting",
        "werfault",
        "problem reporter",
        "reportcrash",
        "legacycrashreporter",
        "coreaudiod",
        "coreservicesuiagent",
        "usernotificationcenter",
        "notificationcenter",
        "controlcenter",
        "loginwindow",
        "windowserver",
        "dock",
        "systemuiserver",
        "universalcontrol",
        "airplayuiagent",
        "softwareupdated",
        "storeuid",
        "backgroundtaskmanagementagent",
    ];
    if EXACT.iter().any(|n| *n == lower) {
        return true;
    }
    lower.contains("error reporting")
        || lower.contains("crash reporter")
        || lower.contains("crashreporter")
        || lower.ends_with("_loader")
        || lower.contains("helper") && lower.contains("microsoft")
}

/// Build a low-priority rule for an app we can identify by name.
pub fn auto_app_rule(app_name: &str) -> CategoryRule {
    let label = friendly_app_label(app_name);
    CategoryRule {
        id: auto_app_id(app_name),
        label: label.clone(),
        display: label,
        match_apps: vec![app_name.to_string()],
        title_contains: vec![],
        url_contains: vec![],
        priority: 5,
        enabled: true,
    }
}

pub fn classify(
    app_name: &str,
    window_title: &str,
    page_url: &str,
    rules: &[CategoryRule],
) -> Classified {
    let app_l = app_name.to_lowercase();
    let title_l = window_title.to_lowercase();
    let url_l = page_url.to_lowercase();

    let mut matched: Option<&CategoryRule> = None;
    for rule in rules.iter().filter(|r| r.enabled) {
        if !rule_matches_app(rule, &app_l) {
            continue;
        }
        if !rule_content_ok(rule, &title_l, &url_l) {
            continue;
        }
        matched = Some(match matched {
            Some(prev) if prev.priority >= rule.priority => prev,
            _ => rule,
        });
    }

    if let Some(rule) = matched {
        return Classified {
            category_id: rule.id.clone(),
            category_label: rule.label.clone(),
            display: rule.display.clone(),
        };
    }

    // User disabled a rule for this app — treat as unreadable/Other.
    for rule in rules.iter().filter(|r| !r.enabled) {
        if rule_matches_app(rule, &app_l) && rule_content_ok(rule, &title_l, &url_l) {
            return Classified {
                category_id: "other".into(),
                category_label: "Other".into(),
                display: "Other (category off)".into(),
            };
        }
    }

    // Helper / crash / loader processes stay in Other (not named bars).
    if is_noise_app(app_name) {
        return Classified {
            category_id: "other".into(),
            category_label: "Other".into(),
            display: "Other".into(),
        };
    }

    // Identifiable app with no curated rule → named entry (not Other).
    if !app_name.is_empty() {
        let label = friendly_app_label(app_name);
        return Classified {
            category_id: auto_app_id(app_name),
            category_label: label.clone(),
            display: label,
        };
    }

    // Could not read which app is focused.
    Classified {
        category_id: "other".into(),
        category_label: "Other".into(),
        display: "Idle / unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browsers() -> Vec<String> {
        vec!["Chrome".into(), "Brave".into()]
    }

    #[test]
    fn support_domain_not_path() {
        let rules = vec![CategoryRule {
            id: "support-site".into(),
            label: "Company support site".into(),
            display: "Company support site".into(),
            match_apps: browsers(),
            title_contains: vec![],
            url_contains: vec![
                "support.example.com".into(),
                "helpdesk.example.com".into(),
            ],
            priority: 220,
            enabled: true,
        }];
        let c = classify(
            "Google Chrome",
            "CustomerInfo",
            "https://support.example.com/customers/view?id=12345",
            &rules,
        );
        assert_eq!(c.category_id, "support-site");
    }

    #[test]
    fn wp_admin_beats_site() {
        let rules = vec![
            CategoryRule {
                id: "company-wp-admin".into(),
                label: "WP Admin".into(),
                display: "Company WP Admin".into(),
                match_apps: browsers(),
                title_contains: vec![],
                url_contains: vec!["example.com/wp-admin".into()],
                priority: 240,
                enabled: true,
            },
            CategoryRule {
                id: "company-site".into(),
                label: "Site".into(),
                display: "Company website".into(),
                match_apps: browsers(),
                title_contains: vec![],
                url_contains: vec!["example.com".into()],
                priority: 210,
                enabled: true,
            },
        ];
        let c = classify(
            "Google Chrome",
            "Dashboard",
            "https://example.com/wp-admin/index.php",
            &rules,
        );
        assert_eq!(c.category_id, "company-wp-admin");
    }

    #[test]
    fn zendesk_in_chrome() {
        let rules = vec![CategoryRule {
            id: "zendesk".into(),
            label: "Zendesk".into(),
            display: "Working on Zendesk".into(),
            match_apps: browsers(),
            title_contains: vec!["zendesk".into()],
            url_contains: vec![],
            priority: 100,
            enabled: true,
        }];
        let c = classify("Google Chrome", "Ticket #1 – Zendesk", "", &rules);
        assert_eq!(c.category_id, "zendesk");
    }

    #[test]
    fn known_app_gets_named_entry_not_other() {
        let c = classify("MSTeams", "Chat", "", &[]);
        assert_eq!(c.category_id, "app-msteams");
        assert_eq!(c.category_label, "Teams");
    }

    #[test]
    fn noise_apps_stay_in_other() {
        let c = classify("app_mode_loader", "", "", &[]);
        assert_eq!(c.category_id, "other");
        let c2 = classify("Microsoft Error Reporting", "", "", &[]);
        assert_eq!(c2.category_id, "other");
    }

    #[test]
    fn empty_app_is_other() {
        let c = classify("", "", "", &[]);
        assert_eq!(c.category_id, "other");
    }
}
