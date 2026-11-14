//! Правила kill switch: тексты для pf (macOS) и nftables (Linux).
//!
//! Правила статические и не содержат данных от клиента: разрешены loopback,
//! локальные сети и трафик root (ядра). Всё остальное блокируется, поэтому при
//! падении ядра выбранные приложения не выходят напрямую. Тексты собраны здесь,
//! чтобы их можно было проверить тестами на любой ОС.

/// Имя таблицы nftables: отдельная таблица не задевает чужие правила.
pub const NFT_TABLE: &str = "splitroute_killswitch";

/// Anchor pf. `com.apple/*` уже присутствует в `/etc/pf.conf` macOS, поэтому
/// дочерний anchor применяется без правки системного конфига.
pub const PF_ANCHOR: &str = "com.apple/splitroute.killswitch";

/// Правила pf: последним идёт блок, быстрые `pass` исключают нужное.
pub const PF_RULES: &str = "\
# SplitRoute kill switch: fail-closed, правила ставит helper.
pass out quick on lo0 all
pass out quick inet from any to { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, 224.0.0.0/4 } keep state
pass out quick inet6 from any to { fe80::/10, fc00::/7, ff00::/8 } keep state
pass out quick user root all keep state
block drop out all
";

/// Правила nftables: отдельная таблица с цепочкой output.
pub const NFT_RULES: &str = "\
add table inet splitroute_killswitch
add chain inet splitroute_killswitch output { type filter hook output priority 0; policy accept; }
add rule inet splitroute_killswitch output oifname \"lo\" accept
add rule inet splitroute_killswitch output ip daddr { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, 224.0.0.0/4 } accept
add rule inet splitroute_killswitch output ip6 daddr { fe80::/10, fc00::/7, ff00::/8 } accept
add rule inet splitroute_killswitch output meta skuid 0 accept
add rule inet splitroute_killswitch output drop
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pf_rules_allow_loopback_lan_and_root_before_block() {
        assert!(PF_RULES.contains("pass out quick on lo0 all"));
        assert!(PF_RULES.contains("pass out quick user root all"));
        assert!(PF_RULES.contains("192.168.0.0/16"));
        let block = PF_RULES
            .find("block drop out all")
            .expect("блокирующее правило");
        let root = PF_RULES.find("user root").expect("исключение для ядра");
        assert!(root < block, "исключения должны идти раньше блока");
    }

    #[test]
    fn pf_rules_are_static_and_contain_no_client_data() {
        // Никаких подстановок из запроса: текст одинаков на каждой машине.
        assert!(!PF_RULES.contains('$'));
        assert!(!PF_RULES.contains('`'));
    }

    #[test]
    fn nft_rules_use_dedicated_table_and_block_last() {
        assert!(NFT_RULES.contains("add table inet splitroute_killswitch"));
        assert!(NFT_RULES.contains("meta skuid 0 accept"));
        let drop = NFT_RULES.rfind("drop").expect("блокирующее правило");
        let root = NFT_RULES.find("meta skuid 0").expect("исключение для ядра");
        assert!(root < drop, "исключения должны идти раньше drop");
    }

    #[test]
    fn nft_rules_allow_loopback_and_lan() {
        assert!(NFT_RULES.contains("oifname \"lo\" accept"));
        assert!(NFT_RULES.contains("10.0.0.0/8"));
        assert!(NFT_RULES.contains("fe80::/10"));
    }
}
