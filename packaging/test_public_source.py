"""Focused regression checks for vendor-example credential sanitization."""
import json, unittest
from private_build import public_source_bytes

class PublicSourceTests(unittest.TestCase):
    def test_vendor_demo_keys_removed_without_altering_project_configuration(self):
        # Synthetic pattern, not a credential; assembled to avoid storing a token.
        key = "AI" + "za" + "X" * 35
        original = {"project_info": {"project_id": "example"}, "client": [{"api_key": [{"current_key": key}], "services": {"example": True}}]}
        name = "plugins/lemon-master-assistant/JUCE/examples/Assets/google-services.json"
        result = public_source_bytes(name, json.dumps(original).encode())
        clean = json.loads(result)
        self.assertEqual(clean["client"][0]["api_key"], [])
        self.assertEqual(clean["client"][0]["services"], original["client"][0]["services"])
        self.assertEqual(clean["project_info"], original["project_info"])
        self.assertEqual(result, public_source_bytes(name, result))
        with self.assertRaises(ValueError):
            public_source_bytes("unexpected-config.json", json.dumps(original).encode())
        self.assertEqual(public_source_bytes("source.cpp", b"int x = 1;"), b"int x = 1;")

if __name__ == "__main__": unittest.main()
