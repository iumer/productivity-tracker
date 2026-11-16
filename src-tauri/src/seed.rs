use crate::db::{CategoryRule, TaskTemplate};
use uuid::Uuid;

pub fn default_category_rules() -> Vec<CategoryRule> {
    let browsers = vec![
        "Google Chrome".into(),
        "Chrome".into(),
        "Brave Browser".into(),
        "Brave".into(),
        "Safari".into(),
        "Firefox".into(),
    ];

    vec![
        // Domain / URL rules (match host, ignore path except wp-admin)
        CategoryRule {
            id: "support-site".into(),
            label: "Company support site".into(),
            display: "Company support site".into(),
            match_apps: browsers.clone(),
            title_contains: vec![
                "customer lookup".into(),
                "new customer".into(),
                "prospect".into(),
            ],
            url_contains: vec![
                "support.example.com".into(),
                "helpdesk.example.com".into(),
            ],
            priority: 260,
            enabled: true,
        },
        CategoryRule {
            id: "knowledge-base".into(),
            label: "Knowledge base".into(),
            display: "Knowledge base".into(),
            match_apps: browsers.clone(),
            title_contains: vec![],
            url_contains: vec!["example.atlassian.net".into()],
            priority: 255,
            enabled: true,
        },
        CategoryRule {
            id: "company-wp-admin".into(),
            label: "Company WP Admin".into(),
            display: "Company WP Admin".into(),
            match_apps: browsers.clone(),
            title_contains: vec![],
            url_contains: vec!["example.com/wp-admin".into()],
            priority: 250,
            enabled: true,
        },
        CategoryRule {
            id: "google-docs".into(),
            label: "Google Docs / Sheets".into(),
            display: "Google Docs / Sheets".into(),
            match_apps: browsers.clone(),
            title_contains: vec![],
            url_contains: vec!["docs.google.com".into()],
            priority: 245,
            enabled: true,
        },
        CategoryRule {
            id: "company-site".into(),
            label: "Company website".into(),
            display: "Company website".into(),
            match_apps: browsers.clone(),
            title_contains: vec![],
            url_contains: vec!["example.com".into()],
            priority: 230,
            enabled: true,
        },
        CategoryRule {
            id: "zendesk".into(),
            label: "Zendesk".into(),
            display: "Working on Zendesk".into(),
            match_apps: browsers.clone(),
            title_contains: vec!["zendesk".into()],
            url_contains: vec!["zendesk.com".into()],
            priority: 240,
            enabled: true,
        },
        CategoryRule {
            id: "netflix".into(),
            label: "Netflix".into(),
            display: "Watching Netflix".into(),
            match_apps: browsers.clone(),
            title_contains: vec!["netflix".into()],
            url_contains: vec!["netflix.com".into()],
            priority: 240,
            enabled: true,
        },
        CategoryRule {
            id: "youtube".into(),
            label: "YouTube".into(),
            display: "Watching YouTube".into(),
            match_apps: browsers.clone(),
            title_contains: vec!["youtube".into()],
            url_contains: vec!["youtube.com".into(), "youtu.be".into()],
            priority: 235,
            enabled: true,
        },
        CategoryRule {
            id: "browser-other".into(),
            label: "Browser".into(),
            display: "Browsing".into(),
            match_apps: browsers,
            title_contains: vec![],
            url_contains: vec![],
            priority: 50,
            enabled: true,
        },
        app_rule("whatsapp", "WhatsApp", "WhatsApp", &["WhatsApp"], 100),
        app_rule(
            "windows-app",
            "Windows App",
            "Windows App / Remote Desktop",
            &["Windows App", "Microsoft Remote Desktop", "Remote Desktop"],
            100,
        ),
        app_rule("notes", "Notes", "Notes", &["Notes"], 100),
        app_rule("chatgpt", "ChatGPT", "ChatGPT", &["ChatGPT"], 100),
        app_rule("cursor", "Cursor", "Cursor", &["Cursor"], 100),
        app_rule("parsec", "Parsec", "Parsec", &["Parsec"], 100),
        app_rule("anydesk", "AnyDesk", "AnyDesk", &["AnyDesk"], 100),
        app_rule(
            "iphone-mirroring",
            "iPhone Mirroring",
            "iPhone Mirroring",
            &["iPhone Mirroring"],
            100,
        ),
        app_rule("calendar", "Calendar", "Calendar", &["Calendar"], 100),
        app_rule("calculator", "Calculator", "Calculator", &["Calculator"], 100),
    ]
}

fn app_rule(
    id: &str,
    label: &str,
    display: &str,
    apps: &[&str],
    priority: i32,
) -> CategoryRule {
    CategoryRule {
        id: id.into(),
        label: label.into(),
        display: display.into(),
        match_apps: apps.iter().map(|s| (*s).to_string()).collect(),
        title_contains: vec![],
        url_contains: vec![],
        priority,
        enabled: true,
    }
}

fn tmpl(
    weekday: i32,
    summary: &str,
    sort_order: i32,
    reminder: Option<(u32, u32, &str)>,
) -> TaskTemplate {
    let (reminder_enabled, reminder_hour, reminder_minute, reminder_tz) = match reminder {
        Some((h, m, tz)) => (true, Some(h), Some(m), tz.to_string()),
        None => (false, None, None, "America/Chicago".into()),
    };
    TaskTemplate {
        id: Uuid::new_v4().to_string(),
        weekday,
        summary: summary.to_string(),
        sort_order,
        reminder_enabled,
        reminder_hour,
        reminder_minute,
        reminder_tz,
    }
}

/// weekday: 0=Sun … 6=Sat
pub fn default_task_templates() -> Vec<TaskTemplate> {
    vec![
        tmpl(1, "Pause scheduled jobs", 0, Some((10, 0, "America/Chicago"))),
        tmpl(1, "Check server status", 1, None),
        tmpl(1, "Check voicemail", 2, None),
        tmpl(2, "Check server status", 0, None),
        tmpl(2, "Check voicemail", 1, None),
        tmpl(2, "Backup servers", 2, Some((10, 50, "America/Chicago"))),
        tmpl(2, "Weekly plugin updates", 3, None),
        tmpl(3, "Check server status", 0, None),
        tmpl(3, "Check voicemail", 1, None),
        tmpl(3, "Check failed emails", 2, None),
        tmpl(4, "Check server status", 0, None),
        tmpl(4, "Check voicemail", 1, None),
        tmpl(5, "Check server status", 0, None),
        tmpl(5, "Check voicemail", 1, None),
        tmpl(5, "Backup servers", 2, Some((10, 50, "America/Chicago"))),
        tmpl(5, "Resume scheduled jobs", 3, Some((18, 0, "America/Chicago"))),
    ]
}
