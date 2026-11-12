//! Разбор санитизированной фикстуры подписки из `spike/fixtures/`.

use std::path::Path;

use splitroute_lib::subscription::parse_subscription;
use splitroute_protocol::{Protocol, TlsKind, Transport};

fn fixture() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../spike/fixtures/subscription.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("не удалось прочитать {}: {error}", path.display()))
}

#[test]
fn parses_both_fixture_profiles() {
    let profiles = parse_subscription(&fixture()).expect("фикстура должна разбираться");
    assert_eq!(
        profiles.len(),
        2,
        "в фикстуре два профиля VLESS/Reality/TCP"
    );
    for profile in &profiles {
        assert_eq!(profile.tls.kind, TlsKind::Reality);
        assert!(profile.port > 0);
        assert_eq!(profile.protocol, Protocol::Vless);
        assert_eq!(profile.transport, Transport::Tcp);
        assert!(profile.name.contains("Тестовая локация"));
        assert!(profile.tls.public_key.is_some());
        assert!(profile.id.starts_with("profile-"));
    }
    assert_ne!(profiles[0].id, profiles[1].id);
}
