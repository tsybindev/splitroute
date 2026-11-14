//! macOS: правила kill switch в pf anchor.
//!
//! `/etc/pf.conf` уже содержит `anchor "com.apple/*"`, поэтому дочерний anchor
//! применяется без правки системного файла. Отдельный `pfctl -E` включает pf и
//! возвращает токен; токен сохраняется в runtime-каталоге (`0600`), чтобы
//! снять ссылку при остановке и при очистке после падения helper'а.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::rules::{PF_ANCHOR, PF_RULES};
use super::{Firewall, FirewallError};

const PFCTL: &str = "/sbin/pfctl";
const RULES_FILE: &str = "killswitch.pf";
const TOKEN_FILE: &str = "killswitch.token";

/// Платформенный backend pf.
pub struct SystemFirewall {
    runtime_dir: PathBuf,
}

impl SystemFirewall {
    pub fn new(runtime_dir: &Path, _core: Option<&Path>) -> Self {
        Self {
            runtime_dir: runtime_dir.to_path_buf(),
        }
    }

    fn rules_path(&self) -> PathBuf {
        self.runtime_dir.join(RULES_FILE)
    }

    fn token_path(&self) -> PathBuf {
        self.runtime_dir.join(TOKEN_FILE)
    }

    fn write_rules(&self) -> Result<(), FirewallError> {
        fs::create_dir_all(&self.runtime_dir)
            .map_err(|error| format!("не удалось создать runtime-каталог: {error}"))?;
        let mut file = crate::platform::open_private(&self.rules_path())
            .map_err(|error| format!("не удалось записать правила pf: {error}"))?;
        file.write_all(PF_RULES.as_bytes())
            .map_err(|error| format!("не удалось записать правила pf: {error}"))?;
        file.flush()
            .map_err(|error| format!("не удалось записать правила pf: {error}"))?;
        crate::platform::set_file_mode(&self.rules_path())
            .map_err(|error| format!("не удалось выставить права на правила pf: {error}"))
    }

    fn read_token(&self) -> Option<String> {
        fs::read_to_string(self.token_path())
            .ok()
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
    }

    fn write_token(&self, token: &str) -> Result<(), FirewallError> {
        let mut file = crate::platform::open_private(&self.token_path())
            .map_err(|error| format!("не удалось сохранить токен pf: {error}"))?;
        file.write_all(token.as_bytes())
            .map_err(|error| format!("не удалось сохранить токен pf: {error}"))?;
        crate::platform::set_file_mode(&self.token_path())
            .map_err(|error| format!("не удалось выставить права на токен pf: {error}"))
    }

    /// Включает pf один раз и возвращает токен ссылки.
    fn ensure_pf_enabled(&self) -> Result<String, FirewallError> {
        if let Some(token) = self.read_token() {
            return Ok(token);
        }
        let output = Command::new(PFCTL)
            .arg("-E")
            .output()
            .map_err(|error| format!("не удалось запустить pfctl: {error}"))?;
        if !output.status.success() {
            return Err(command_error("pfctl -E", &output.stderr));
        }
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let token = parse_enable_token(&combined)
            .ok_or_else(|| "pfctl не вернул токен включения pf".to_string())?;
        if let Err(error) = self.write_token(&token) {
            let _ = release_token(&token);
            return Err(error);
        }
        Ok(token)
    }

    fn load_anchor(&self) -> Result<(), FirewallError> {
        let output = Command::new(PFCTL)
            .arg("-a")
            .arg(PF_ANCHOR)
            .arg("-f")
            .arg(self.rules_path())
            .output()
            .map_err(|error| format!("не удалось запустить pfctl: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("pfctl -f", &output.stderr))
        }
    }

    fn flush_anchor(&self) -> Result<(), FirewallError> {
        let output = Command::new(PFCTL)
            .arg("-a")
            .arg(PF_ANCHOR)
            .arg("-F")
            .arg("all")
            .output()
            .map_err(|error| format!("не удалось запустить pfctl: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("pfctl -F", &output.stderr))
        }
    }
}

impl Firewall for SystemFirewall {
    fn enable(&self) -> Result<(), FirewallError> {
        self.write_rules()?;
        let token = self.ensure_pf_enabled()?;
        if let Err(error) = self.load_anchor() {
            // Не оставляем включённую ссылку, если правила не применились.
            let _ = release_token(&token);
            let _ = fs::remove_file(self.token_path());
            return Err(error);
        }
        Ok(())
    }

    fn disable(&self) -> Result<(), FirewallError> {
        let mut errors = Vec::new();

        if let Err(error) = self.flush_anchor() {
            if !is_missing_anchor(&error) {
                errors.push(error);
            }
        }
        if let Some(token) = self.read_token() {
            if let Err(error) = release_token(&token) {
                if !is_stale_token(&error) {
                    errors.push(error);
                }
            }
        }
        let _ = fs::remove_file(self.token_path());
        let _ = fs::remove_file(self.rules_path());

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn command_error(what: &str, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{what} завершился с ошибкой")
    } else {
        format!("{what}: {stderr}")
    }
}

/// Достаёт токен из вывода `pfctl -E` (`Token : 123456`).
pub(crate) fn parse_enable_token(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        if !line.starts_with("Token") {
            return None;
        }
        let (_, value) = line.split_once(':')?;
        let value = value.trim();
        if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
            Some(value.to_string())
        } else {
            None
        }
    })
}

/// Anchor уже отсутствует — снятие идемпотентно.
fn is_missing_anchor(error: &str) -> bool {
    let error = error.to_lowercase();
    error.contains("not found") || error.contains("no such") || error.contains("does not exist")
}

/// Токен уже освобождён — повторное снятие не считается ошибкой.
fn is_stale_token(error: &str) -> bool {
    error.to_lowercase().contains("not found")
}

fn release_token(token: &str) -> Result<(), FirewallError> {
    let output = Command::new(PFCTL)
        .arg("-X")
        .arg(token)
        .output()
        .map_err(|error| format!("не удалось запустить pfctl: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error("pfctl -X", &output.stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_from_stderr() {
        let output = "No ALTQ support in kernel\nToken : 123456\n";
        assert_eq!(parse_enable_token(output).as_deref(), Some("123456"));
    }

    #[test]
    fn parses_token_from_stdout() {
        assert_eq!(parse_enable_token("Token : 42\n").as_deref(), Some("42"));
    }

    #[test]
    fn ignores_lines_without_token() {
        assert_eq!(parse_enable_token("pf enabled\nToken : abc\n"), None);
        assert_eq!(parse_enable_token(""), None);
    }

    #[test]
    fn missing_anchor_errors_are_recognised() {
        assert!(is_missing_anchor(
            "pfctl -F: pfctl: Anchor 'com.apple/splitroute.killswitch' not found."
        ));
        assert!(is_missing_anchor("no such anchor"));
        assert!(!is_missing_anchor("Permission denied"));
    }

    #[test]
    fn stale_token_is_not_an_error() {
        assert!(is_stale_token("pfctl -X: token 123 not found"));
        assert!(!is_stale_token("Operation not permitted"));
    }
}
