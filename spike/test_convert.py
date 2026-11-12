#!/usr/bin/env python3
"""Тесты спайк-конвертера (S1-T3). Только стандартная библиотека.

Запуск из корня репозитория:
    python3 -m unittest discover -s spike -p 'test_*.py'
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import convert

ROOT = Path(__file__).resolve().parent
FIXTURE = ROOT / "fixtures" / "subscription.json"
CONFIGS = ROOT / "configs"


def fixture_profiles() -> list[dict]:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def convert_one(profile: dict, options: convert.Options | None = None):
    return convert.convert_profile(profile, 1, options or convert.Options())


class ConvertProfilesTest(unittest.TestCase):
    def test_both_profiles_convert(self) -> None:
        results = [convert_one(profile) for profile in fixture_profiles()]
        self.assertTrue(all(result is not None for result in results))

    def test_profile_without_proxy_is_skipped(self) -> None:
        profile = fixture_profiles()[0]
        profile["outbounds"] = [
            outbound for outbound in profile["outbounds"] if outbound["tag"] != "proxy"
        ]
        self.assertIsNone(convert_one(profile))

    def test_unsupported_transport_is_skipped(self) -> None:
        profile = fixture_profiles()[0]
        proxy = next(o for o in profile["outbounds"] if o["tag"] == "proxy")
        proxy["streamSettings"]["network"] = "ws"
        self.assertIsNone(convert_one(profile))

    def test_unsupported_protocol_is_skipped(self) -> None:
        profile = fixture_profiles()[0]
        proxy = next(o for o in profile["outbounds"] if o["tag"] == "proxy")
        proxy["protocol"] = "vmess"
        self.assertIsNone(convert_one(profile))


class OutboundMappingTest(unittest.TestCase):
    def test_vless_reality_fields_match_source(self) -> None:
        for profile in fixture_profiles():
            remarks, config = convert_one(profile)
            proxy = next(o for o in profile["outbounds"] if o["tag"] == "proxy")
            vnext = proxy["settings"]["vnext"][0]
            user = vnext["users"][0]
            reality = proxy["streamSettings"]["realitySettings"]
            outbound = next(o for o in config["outbounds"] if o["tag"] == "proxy")

            self.assertEqual(outbound["type"], "vless")
            self.assertEqual(outbound["server"], vnext["address"])
            self.assertEqual(outbound["server_port"], vnext["port"])
            self.assertEqual(outbound["uuid"], user["id"])
            self.assertEqual(outbound["flow"], user["flow"])
            self.assertEqual(outbound["tls"]["server_name"], reality["serverName"])
            self.assertEqual(
                outbound["tls"]["reality"]["public_key"], reality["publicKey"]
            )
            self.assertEqual(
                outbound["tls"]["reality"]["short_id"], reality["shortId"]
            )
            self.assertEqual(
                outbound["tls"]["utls"]["fingerprint"], reality["fingerprint"]
            )
            self.assertTrue(outbound["tls"]["enabled"])
            self.assertTrue(outbound["tls"]["reality"]["enabled"])
            # TCP — транспорт по умолчанию: блок transport не нужен.
            self.assertNotIn("transport", outbound)
            self.assertTrue(remarks)


class ConfigShapeTest(unittest.TestCase):
    def test_route_and_dns_policy(self) -> None:
        _, config = convert_one(fixture_profiles()[0])
        tun = next(i for i in config["inbounds"] if i["type"] == "tun")
        self.assertTrue(tun["auto_route"])
        self.assertTrue(tun["strict_route"])
        self.assertEqual(config["route"]["final"], "direct")
        self.assertTrue(config["route"]["auto_detect_interface"])
        self.assertEqual(config["route"]["default_domain_resolver"], {"server": "local"})

        self.assertEqual(config["dns"]["final"], "local")
        remote = next(s for s in config["dns"]["servers"] if s["tag"] == "remote")
        self.assertEqual(remote["detour"], "proxy")

        allowed = [r for r in config["route"]["rules"] if r.get("outbound") == "proxy"]
        self.assertTrue(any("process_name" in rule for rule in allowed))
        self.assertTrue(any("process_path_regex" in rule for rule in allowed))


class GoldenTest(unittest.TestCase):
    def test_configs_are_reproducible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)
            source = str(FIXTURE)
            for number, profile in enumerate(fixture_profiles(), start=1):
                remarks, config = convert_one(profile)
                text = json.dumps(config, ensure_ascii=False, indent=2) + "\n"
                expected = CONFIGS / f"profile-{number}-{convert.slugify(remarks)}.json"
                self.assertEqual(text, expected.read_text(encoding="utf-8"), source)


class InputValidationTest(unittest.TestCase):
    def test_empty_array_rejected(self) -> None:
        with self.assertRaises(convert.ConversionError):
            convert.load_profiles(b"[]", Path("empty.json"))

    def test_non_array_rejected(self) -> None:
        with self.assertRaises(convert.ConversionError):
            convert.load_profiles(b'{"a": 1}', Path("object.json"))

    def test_broken_json_rejected(self) -> None:
        with self.assertRaises(convert.ConversionError):
            convert.load_profiles(b"{not json", Path("broken.json"))


if __name__ == "__main__":
    unittest.main()
