//! Проекция правил allowlist для UI: признак «путь не найден» (PRD §6.3) и
//! валидация правила до записи в store (S10-T1).
//!
//! Проверка путей — локальный `stat` при чтении списка: без сети, привилегий
//! и исполнения строк из правила. Признак ставится только там, где ответ
//! однозначен: `process_name` и произвольные regex не проверяются, чтобы не
//! показывать ложное «неактивно».

use std::path::Path;

use regex::Regex;
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

/// Лимиты валидации правила: rule-set перечитывается работающим ядром, и
/// мусор в нём означал бы, что новые правила просто не применятся до
/// следующей правки (sing-box при ошибке парсинга оставляет старые). Проверка
/// в Rust закрывает это до записи в store.
const MAX_NAME_CHARS: usize = 200;
const MAX_PATTERNS: usize = 20;
const MAX_PATTERN_CHARS: usize = 1024;

/// Проверяет правило до записи в store (S10-T1).
///
/// `path_regex` компилируется крейтом `regex`: его синтаксис и синтаксис
/// Go-regexp в sing-box происходят из RE2, поэтому для шаблонов правил
/// (пути, экранирование, якоря, символьные классы) компиляция эквивалентна.
/// Пустое имя или правило без селекторов в rule-set не попадает и смысла не
/// имеет — такие тоже отклоняются.
pub fn validate_rule(rule: &AppRule) -> Result<(), String> {
    let name = rule.name.trim();
    if name.is_empty() {
        return Err("название правила не заполнено".to_string());
    }
    if rule.name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("название длиннее {MAX_NAME_CHARS} символов"));
    }

    let fields: [(&str, &Vec<String>); 3] = [
        ("process_name", &rule.process_name),
        ("process_path", &rule.process_path),
        ("path_regex", &rule.path_regex),
    ];
    if fields.iter().all(|(_, values)| values.is_empty()) {
        return Err("правило без селекторов: заполните имя процесса, путь или шаблон".to_string());
    }

    for (field, values) in fields {
        if values.len() > MAX_PATTERNS {
            return Err(format!("поле {field}: больше {MAX_PATTERNS} значений"));
        }
        for value in values {
            if value.trim().is_empty() {
                return Err(format!("поле {field}: есть пустое значение"));
            }
            if value.chars().count() > MAX_PATTERN_CHARS {
                return Err(format!(
                    "поле {field}: значение длиннее {MAX_PATTERN_CHARS} символов"
                ));
            }
        }
    }

    for pattern in &rule.path_regex {
        if let Err(error) = Regex::new(pattern) {
            return Err(format!("шаблон не компилируется: {error}"));
        }
    }
    Ok(())
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

    fn validation_error(rule: &AppRule) -> String {
        validate_rule(rule).unwrap_err()
    }

    #[test]
    fn valid_rules_pass_validation() {
        for rule in [
            rule(&[], &[], &["curl"]),
            rule(&["/opt/app/bin/app"], &[], &[]),
            rule(
                &[],
                &[r"^C:\\Users\\Имя\\Discord\\app-[0-9.]+\\Discord\.exe$"],
                &[],
            ),
            rule(&[], &[r"^/opt/.*\.app/"], &[]),
        ] {
            assert!(validate_rule(&rule).is_ok(), "{}", rule.name);
        }
    }

    #[test]
    fn empty_name_and_blank_selectors_are_rejected() {
        let mut no_name = rule(&[], &[], &["curl"]);
        no_name.name = "   ".to_string();
        assert_eq!(validation_error(&no_name), "название правила не заполнено");

        let blank = rule(&[], &[], &[]);
        assert!(validation_error(&blank).contains("без селекторов"));

        let mut empty_value = rule(&[], &[], &[]);
        empty_value.process_name = vec!["   ".to_string()];
        assert!(validation_error(&empty_value).contains("пустое значение"));
    }

    #[test]
    fn broken_regex_is_rejected_before_store() {
        let rule = rule(&[], &[r"^/opt/[unclosed"], &[]);
        assert!(validation_error(&rule).contains("не компилируется"));
    }

    #[test]
    fn oversized_values_are_rejected() {
        let mut long_name = rule(&[], &[], &["curl"]);
        long_name.name = "x".repeat(201);
        assert!(validation_error(&long_name).contains("название длиннее"));

        let long_pattern = rule(&[], &[&"a".repeat(1025)], &[]);
        assert!(validation_error(&long_pattern).contains("длиннее"));

        let mut many = rule(&[], &[], &[]);
        many.process_name = (0..21).map(|i| format!("app-{i}")).collect();
        assert!(validation_error(&many).contains("больше"));
    }

    /// Несуществующий абсолютный путь, корректный на текущей платформе.
    fn missing_absolute(tag: &str) -> String {
        std::env::temp_dir()
            .join(format!("splitroute-missing-{tag}-{}", std::process::id()))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn existing_absolute_path_is_active() {
        let rule = rule(&["/opt/app/bin/app"], &[], &[]);
        assert!(missing_with(&rule, &["/opt/app/bin/app"]).is_empty());
    }

    #[test]
    fn missing_absolute_path_is_reported() {
        let path = missing_absolute("single");
        let rule = rule(&[path.as_str()], &[], &[]);
        assert_eq!(missing_with(&rule, &[]), vec![path]);
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

    // Так правило выглядит на macOS и Linux; на Windows другие разделители.
    #[cfg(unix)]
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
        let first = missing_absolute("first");
        let second = missing_absolute("second");
        let rule = rule(&[first.as_str(), second.as_str()], &[], &[]);
        assert_eq!(missing_with(&rule, &[]), vec![first, second]);
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
