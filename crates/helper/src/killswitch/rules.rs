//! Правила kill switch: тексты для pf (macOS), nftables (Linux) и статический
//! план фильтров WFP (Windows).
//!
//! Правила статические и не содержат данных от клиента: разрешены loopback,
//! локальные сети, трафик root/SYSTEM (ядра) и трафик, уходящий в TUN. Всё
//! остальное блокируется, поэтому при падении ядра выбранные приложения не
//! выходят напрямую. Тексты и план собраны здесь, чтобы их можно было проверить
//! тестами на любой ОС.

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

// --- Windows: статический план фильтров WFP ---------------------------------
//
// Windows не умеет «разрешить root»: эквивалент — точный AppId процесса ядра
// (`sing-box.exe`, путь передаёт helper, не клиент). Трафик приложений,
// завёрнутый в туннель, определяется по локальному адресу соединения: маршрут в
// TUN назначает источником адрес TUN. Когда ядро падает, адрес исчезает и
// разрешение перестаёт совпадать — правила остаются и работают fail-closed.

/// Версия плана. Меняется вместе с набором правил, чтобы изменение было явным.
pub const WINDOWS_PLAN_VERSION: u16 = 1;

/// Локальный адрес TUN IPv4; совпадает с `TUN_ADDRESS_V4` генератора конфига.
pub const WINDOWS_TUN_V4: [u8; 4] = [172, 19, 0, 1];
/// Локальный адрес TUN IPv6; совпадает с `TUN_ADDRESS_V6` генератора конфига.
pub const WINDOWS_TUN_V6: [u8; 16] = [
    0xfd, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
];

/// Слой WFP, на котором применяется фильтр.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinLayer {
    AuthConnectV4,
    AuthConnectV6,
}

/// Действие фильтра WFP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinAction {
    Permit,
    Block,
}

/// Условие фильтра. Все значения фиксированы в коде, кроме `CoreAppId`, куда
/// helper подставляет путь к своему ядру.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinCondition {
    /// Трафик loopback (`FWP_CONDITION_FLAG_IS_LOOPBACK`).
    Loopback,
    /// Локальный адрес соединения — адрес TUN: трафик приложений в туннель.
    TunLocalV4([u8; 4]),
    TunLocalV6([u8; 16]),
    /// Удалённая подсеть (локальные сети, link-local, multicast).
    RemoteV4 {
        addr: [u8; 4],
        prefix: u8,
    },
    RemoteV6 {
        addr: [u8; 16],
        prefix: u8,
    },
    /// Точный удалённый адрес (broadcast DHCP).
    RemoteExactV4([u8; 4]),
    /// UDP: единственный протокол, который разрешается сверх адресов.
    Udp,
    LocalPort(u16),
    RemotePort(u16),
    /// AppId процесса ядра — эквивалент исключения root.
    CoreAppId,
}

/// Один фильтр WFP. Больший `weight` приоритетнее: разрешения обязаны быть выше
/// блока, иначе блок перекрыл бы их.
#[derive(Debug)]
pub struct WinFilter {
    pub name: &'static str,
    pub layer: WinLayer,
    pub action: WinAction,
    pub weight: u16,
    pub conditions: &'static [WinCondition],
}

/// Вес блока: ниже всех разрешений.
pub const WINDOWS_BLOCK_WEIGHT: u16 = 1;

/// План фильтров Windows. Порядок в массиве не важен — приоритет задаёт вес.
pub const WINDOWS_FILTERS: &[WinFilter] = &[
    WinFilter {
        name: "core-v4",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 110,
        conditions: &[WinCondition::CoreAppId],
    },
    WinFilter {
        name: "core-v6",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Permit,
        weight: 110,
        conditions: &[WinCondition::CoreAppId],
    },
    WinFilter {
        name: "tun-v4",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 109,
        conditions: &[WinCondition::TunLocalV4(WINDOWS_TUN_V4)],
    },
    WinFilter {
        name: "tun-v6",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Permit,
        weight: 109,
        conditions: &[WinCondition::TunLocalV6(WINDOWS_TUN_V6)],
    },
    WinFilter {
        name: "loopback-v4",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 108,
        conditions: &[WinCondition::Loopback],
    },
    WinFilter {
        name: "loopback-v6",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Permit,
        weight: 108,
        conditions: &[WinCondition::Loopback],
    },
    WinFilter {
        name: "lan-10-8",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV4 {
            addr: [10, 0, 0, 0],
            prefix: 8,
        }],
    },
    WinFilter {
        name: "lan-172-16-12",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV4 {
            addr: [172, 16, 0, 0],
            prefix: 12,
        }],
    },
    WinFilter {
        name: "lan-192-168-16",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV4 {
            addr: [192, 168, 0, 0],
            prefix: 16,
        }],
    },
    WinFilter {
        name: "lan-169-254-16",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV4 {
            addr: [169, 254, 0, 0],
            prefix: 16,
        }],
    },
    WinFilter {
        name: "multicast-v4",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV4 {
            addr: [224, 0, 0, 0],
            prefix: 4,
        }],
    },
    WinFilter {
        name: "lan-fe80-10",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV6 {
            addr: [0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            prefix: 10,
        }],
    },
    WinFilter {
        name: "lan-fc00-7",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV6 {
            addr: [0xfc, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            prefix: 7,
        }],
    },
    WinFilter {
        name: "multicast-v6",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Permit,
        weight: 107,
        conditions: &[WinCondition::RemoteV6 {
            addr: [0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            prefix: 8,
        }],
    },
    WinFilter {
        name: "dhcp-v4",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Permit,
        weight: 106,
        conditions: &[
            WinCondition::Udp,
            WinCondition::LocalPort(68),
            WinCondition::RemotePort(67),
            WinCondition::RemoteExactV4([255, 255, 255, 255]),
        ],
    },
    WinFilter {
        name: "block-v4",
        layer: WinLayer::AuthConnectV4,
        action: WinAction::Block,
        weight: WINDOWS_BLOCK_WEIGHT,
        conditions: &[],
    },
    WinFilter {
        name: "block-v6",
        layer: WinLayer::AuthConnectV6,
        action: WinAction::Block,
        weight: WINDOWS_BLOCK_WEIGHT,
        conditions: &[],
    },
];

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

    #[test]
    fn windows_plan_has_one_block_per_layer_below_every_permit() {
        for layer in [WinLayer::AuthConnectV4, WinLayer::AuthConnectV6] {
            let blocks: Vec<_> = WINDOWS_FILTERS
                .iter()
                .filter(|filter| filter.layer == layer && filter.action == WinAction::Block)
                .collect();
            assert_eq!(blocks.len(), 1, "на каждом слое ровно один блок");
            assert!(blocks[0].conditions.is_empty(), "блок без условий");
            assert_eq!(blocks[0].weight, WINDOWS_BLOCK_WEIGHT);

            for permit in WINDOWS_FILTERS
                .iter()
                .filter(|filter| filter.layer == layer && filter.action == WinAction::Permit)
            {
                assert!(
                    permit.weight > blocks[0].weight,
                    "{}: разрешение должно быть выше блока",
                    permit.name
                );
            }
        }
    }

    #[test]
    fn windows_plan_permits_loopback_tun_and_core_on_both_layers() {
        for layer in [WinLayer::AuthConnectV4, WinLayer::AuthConnectV6] {
            let conditions: Vec<_> = WINDOWS_FILTERS
                .iter()
                .filter(|filter| filter.layer == layer && filter.action == WinAction::Permit)
                .flat_map(|filter| filter.conditions.iter())
                .collect();
            assert!(conditions.contains(&&WinCondition::Loopback));
            assert!(conditions.contains(&&WinCondition::CoreAppId));
            let tun = conditions.iter().any(|condition| {
                matches!(
                    condition,
                    WinCondition::TunLocalV4(_) | WinCondition::TunLocalV6(_)
                )
            });
            assert!(tun, "трафик приложений в TUN должен разрешаться");
        }
    }

    #[test]
    fn windows_plan_allows_the_same_local_networks_as_pf_and_nft() {
        let v4: Vec<_> = WINDOWS_FILTERS
            .iter()
            .flat_map(|filter| filter.conditions.iter())
            .filter_map(|condition| match condition {
                WinCondition::RemoteV4 { addr, prefix } => Some((*addr, *prefix)),
                _ => None,
            })
            .collect();
        for expected in [
            ([10, 0, 0, 0], 8),
            ([172, 16, 0, 0], 12),
            ([192, 168, 0, 0], 16),
            ([169, 254, 0, 0], 16),
            ([224, 0, 0, 0], 4),
        ] {
            assert!(v4.contains(&expected), "нет подсети {expected:?}");
        }

        let v6: Vec<_> = WINDOWS_FILTERS
            .iter()
            .flat_map(|filter| filter.conditions.iter())
            .filter_map(|condition| match condition {
                WinCondition::RemoteV6 { prefix, .. } => Some(*prefix),
                _ => None,
            })
            .collect();
        for prefix in [10, 7, 8] {
            assert!(
                v6.contains(&prefix),
                "нет IPv6-подсети с префиксом {prefix}"
            );
        }
    }

    #[test]
    fn windows_plan_is_static_and_contains_no_client_data() {
        // Весь план — константы: нет путей, портов и адресов из запроса. AppId
        // ядра — единственное значение, которое helper подставляет сам.
        for filter in WINDOWS_FILTERS {
            assert!(!filter.name.is_empty());
            assert!(filter.name.is_ascii());
            for condition in filter.conditions {
                match condition {
                    WinCondition::TunLocalV4(addr) => assert_eq!(*addr, WINDOWS_TUN_V4),
                    WinCondition::TunLocalV6(addr) => assert_eq!(*addr, WINDOWS_TUN_V6),
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn windows_plan_has_single_dhcp_exception() {
        let dhcp: Vec<_> = WINDOWS_FILTERS
            .iter()
            .filter(|filter| filter.name == "dhcp-v4")
            .collect();
        assert_eq!(dhcp.len(), 1);
        assert_eq!(
            dhcp[0].conditions,
            &[
                WinCondition::Udp,
                WinCondition::LocalPort(68),
                WinCondition::RemotePort(67),
                WinCondition::RemoteExactV4([255, 255, 255, 255]),
            ]
        );
    }

    #[test]
    fn windows_tun_addresses_are_the_documented_fixed_values() {
        // Значения повторяют `TUN_ADDRESS_V4`/`TUN_ADDRESS_V6` генератора
        // конфига: helper не получает их от клиента и не должен угадывать.
        assert_eq!(WINDOWS_TUN_V4, [172, 19, 0, 1]);
        assert_eq!(
            WINDOWS_TUN_V6,
            [0xfd, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
        );
        assert_eq!(WINDOWS_PLAN_VERSION, 1);
    }
}
