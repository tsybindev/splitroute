//! Linux: правила kill switch в отдельной таблице nftables.
//!
//! Используется собственная таблица `inet splitroute_killswitch`, поэтому
//! правила не пересекаются с чужими. Перед загрузкой таблица пересоздаётся;
//! отсутствие таблицы при снятии — не ошибка.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::rules::{NFT_RULES, NFT_TABLE};
use super::{Firewall, FirewallError};

const RULES_FILE: &str = "killswitch.nft";

/// Платформенный backend nftables.
pub struct SystemFirewall {
    runtime_dir: PathBuf,
}

impl SystemFirewall {
    pub fn new(runtime_dir: &Path) -> Self {
        Self {
            runtime_dir: runtime_dir.to_path_buf(),
        }
    }

    fn rules_path(&self) -> PathBuf {
        self.runtime_dir.join(RULES_FILE)
    }

    fn write_rules(&self) -> Result<(), FirewallError> {
        fs::create_dir_all(&self.runtime_dir)
            .map_err(|error| format!("не удалось создать runtime-каталог: {error}"))?;
        let mut file = crate::platform::open_private(&self.rules_path())
            .map_err(|error| format!("не удалось записать правила nftables: {error}"))?;
        file.write_all(NFT_RULES.as_bytes())
            .map_err(|error| format!("не удалось записать правила nftables: {error}"))?;
        file.flush()
            .map_err(|error| format!("не удалось записать правила nftables: {error}"))?;
        crate::platform::set_file_mode(&self.rules_path())
            .map_err(|error| format!("не удалось выставить права на правила nftables: {error}"))
    }

    fn delete_table(&self) -> Result<(), FirewallError> {
        let output = Command::new(nft_program())
            .arg("delete")
            .arg("table")
            .arg("inet")
            .arg(NFT_TABLE)
            .output()
            .map_err(|error| format!("не удалось запустить nft: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if is_missing_table(&stderr) {
            return Ok(());
        }
        Err(format!("nft delete table: {stderr}"))
    }
}

impl Firewall for SystemFirewall {
    fn enable(&self) -> Result<(), FirewallError> {
        self.write_rules()?;
        // Отдельная таблица пересоздаётся целиком: повторное включение безопасно.
        self.delete_table()?;
        let output = Command::new(nft_program())
            .arg("-f")
            .arg(self.rules_path())
            .output()
            .map_err(|error| format!("не удалось запустить nft: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(format!("nft -f: {stderr}"))
        }
    }

    fn disable(&self) -> Result<(), FirewallError> {
        self.delete_table()?;
        let _ = fs::remove_file(self.rules_path());
        Ok(())
    }
}

fn nft_program() -> &'static str {
    if Path::new("/usr/bin/nft").exists() {
        "/usr/bin/nft"
    } else {
        "nft"
    }
}

/// Таблицы нет — снятие идемпотентно.
fn is_missing_table(error: &str) -> bool {
    let error = error.to_lowercase();
    error.contains("no such file or directory")
        || error.contains("does not exist")
        || error.contains("no such table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_table_errors_are_recognised() {
        assert!(is_missing_table(
            "Error: Could not process rule: No such file or directory"
        ));
        assert!(is_missing_table("Error: table does not exist"));
        assert!(!is_missing_table("Operation not permitted"));
    }

    #[test]
    fn rules_use_dedicated_table() {
        assert!(NFT_RULES.contains(NFT_TABLE));
        assert!(!NFT_RULES.contains("flush ruleset"));
    }
}
