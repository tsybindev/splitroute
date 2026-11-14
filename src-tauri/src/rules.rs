//! Проекция правил allowlist для UI: признак «путь не найден» (PRD §6.3).
//!
//! Проверка — локальный `stat` при чтении списка: без сети, привилегий и
//! исполнения строк из правила. Признак ставится только там, где ответ
//! однозначен: `process_name` и произвольные regex не проверяются, чтобы не
//! показывать ложное «неактивно».

use std::path::Path;

use splitroute_protocol::{AppRule, AppRuleView};

/// Строит правило с признаком отсутствующего пути.
pub fn view(rule: &AppRule) -> AppRuleView {
    AppRuleView {
        rule: rule.clone(),
        missing_paths: missing_paths(rule, |path| Path::new(path).exists()),
    }
}

/// Проекция списка правил для команд `rules_list`/`rules_upsert`/`rules_delete`.
pub fn views(rules: Vec<AppRule>) -> Vec<AppRuleView> {
    rules.iter().map(view).collect()
}

/// Возвращает абсолютные пути правила, которых нет на диске.
///
/// Проверяются только абсолютные `process_path` и `path_regex`, которые целиком
/// сводятся к литеральному пути (так выглядит regex каталога:
/// `^/Applications/App.app/`). Если хотя бы один проверяемый путь существует,
/// правило считается активным: пустой ответ означает «всё в порядке или
/// проверить нельзя».
pub fn missing_paths(rule: &AppRule, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    for path in &rule.process_path {
        if Path::new(path).is_absolute() && !candidates.contains(path) {
            candidates.push(path.clone());
        }
    }
    for pattern in &rule.path_regex {
        let Some(literal) = literal_path(pattern) else {
            continue;
        };
        if !candidates.contains(&literal) {
            candidates.push(literal);
        }
    }
    if candidates.is_empty() || candidates.iter().any(|path| exists(path)) {
        return Vec::new();
    }
    candidates
}

/// Сводит regex к литеральному пути, если он состоит только из `^`, литералов
/// и экранированных спецсимволов. Всё остальное (`.`, `.*`, `[a-z]`, `$`)
/// не проверяется: по такому шаблону нельзя сказать, существует ли путь.
fn literal_path(pattern: &str) -> Option<String> {
    let rest = pattern.strip_prefix('^')?;
    if !rest.ends_with('/') {
        return None;
    }
    let mut literal = String::with_capacity(rest.len());
    let mut chars = rest.chars();
    while let Some(character) = chars.next() {
        match character {
            '\\' => literal.push(unescape(chars.next()?)?),
            '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '.' => {
                return None;
            }
            _ => literal.push(character),
        }
    }
    Path::new(&literal).is_absolute().then_some(literal)
}

/// Символы, которые экранирует генератор правил каталога (`catalog::escape_regex`).
fn unescape(character: char) -> Option<char> {
    matches!(
        character,
        '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
    )
    .then_some(character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use splitroute_protocol::Platform;

    fn rule(process_path: &[&str], path_regex: &[&str], process_name: &[&str]) -> AppRule {
        AppRule {
            id: "rule-1".to_string(),
            name: "Тестовое правило".to_string(),
            platform: Platform::Linux,
            process_name: process_name.iter().map(|value| value.to_string()).collect(),
            process_path: process_path.iter().map(|value| value.to_string()).collect(),
            path_regex: path_regex.iter().map(|value| value.to_string()).collect(),
            enabled: true,
        }
    }

    /// Существование подменяется предикатом: тесты не зависят от файловой системы.
    fn missing_with(rule: &AppRule, existing: &[&str]) -> Vec<String> {
        missing_paths(rule, |path| existing.contains(&path))
    }

    #[test]
    fn existing_absolute_path_is_active() {
        let rule = rule(&["/opt/app/bin/app"], &[], &[]);
        assert!(missing_with(&rule, &["/opt/app/bin/app"]).is_empty());
    }

    #[test]
    fn missing_absolute_path_is_reported() {
        let rule = rule(&["/opt/gone/bin/app"], &[], &[]);
        assert_eq!(
            missing_with(&rule, &[]),
            vec!["/opt/gone/bin/app".to_string()]
        );
    }

    #[test]
    fn relative_path_is_not_checked() {
        // Проверить относительный путь нельзя — правило не помечается.
        let rule = rule(&["bin/app"], &[], &[]);
        assert!(missing_with(&rule, &[]).is_empty());
    }

    #[test]
    fn process_name_only_rule_is_not_checked() {
        let rule = rule(&[], &[], &["curl"]);
        assert!(missing_with(&rule, &[]).is_empty());
    }

    #[test]
    fn literal_regex_from_catalog_is_checked() {
        // Так выглядит правило, которое создаёт каталог на macOS.
        let rule = rule(&[], &[r"^/Applications/Google Chrome\.app/"], &[]);
        assert_eq!(
            missing_with(&rule, &[]),
            vec!["/Applications/Google Chrome.app/".to_string()]
        );
        assert!(missing_with(&rule, &["/Applications/Google Chrome.app/"]).is_empty());
    }

    #[test]
    fn regex_with_metacharacters_is_not_checked() {
        for pattern in [
            r"^/Applications/.*\.app/",
            r"^/opt/.*/bin/",
            r"/Applications/App.app/",
            r"^/Applications/App.app$",
            r"^/Applications/App\.(app|bak)/",
            r"^/Applications/[A-Z]pp\.app/",
        ] {
            let rule = rule(&[], &[pattern], &[]);
            assert!(
                missing_with(&rule, &[]).is_empty(),
                "ложное «неактивно» для {pattern}"
            );
        }
    }

    #[test]
    fn one_existing_path_keeps_rule_active() {
        // Правило матчится, если существует хотя бы один из путей.
        let rule = rule(&["/opt/gone/app", "/opt/live/app"], &[], &[]);
        assert!(missing_with(&rule, &["/opt/live/app"]).is_empty());
    }

    #[test]
    fn multiple_missing_paths_are_all_reported() {
        let rule = rule(&["/opt/gone/app", "/opt/also-gone/app"], &[], &[]);
        assert_eq!(
            missing_with(&rule, &[]),
            vec![
                "/opt/gone/app".to_string(),
                "/opt/also-gone/app".to_string()
            ]
        );
    }

    #[test]
    fn empty_rule_is_active() {
        let rule = rule(&[], &[], &[]);
        assert!(missing_with(&rule, &[]).is_empty());
    }

    #[test]
    fn view_reads_real_file_system() {
        let dir = std::env::temp_dir().join(format!("splitroute-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join("app").to_string_lossy().to_string();
        std::fs::write(&existing, b"").unwrap();

        let live = view(&rule(&[&existing], &[], &[]));
        assert!(live.missing_paths.is_empty());
        assert_eq!(live.rule.process_path, vec![existing.clone()]);

        drop(std::fs::remove_file(&existing));
        let gone = view(&rule(&[&existing], &[], &[]));
        assert_eq!(gone.missing_paths, vec![existing]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
