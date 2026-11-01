import io, tempfile, unittest, zipfile
from pathlib import Path
from private_build import MARKER, require_public_binary, require_public_artifact, private_source
class PublicationBoundary(unittest.TestCase):
    def test_private_and_public_binary(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'lemonstudio.exe';p.write_bytes(b'MZpublic');require_public_binary(p)
            p.write_bytes(b'MZ'+MARKER)
            with self.assertRaises(SystemExit):require_public_binary(p)
    def test_nested_update_cannot_hide_private_binary(self):
        nested=io.BytesIO()
        with zipfile.ZipFile(nested,'w') as z:z.writestr('lemonstudio.exe',MARKER)
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'update.zip'
            with zipfile.ZipFile(p,'w') as z:z.writestr('payload.zip',nested.getvalue())
            with self.assertRaises(SystemExit):require_public_artifact(p)
    def test_private_source_excluded_public_stub_kept(self):
        self.assertTrue(private_source('plugins/lemon-master-assistant/Source/PluginProcessor.cpp'))
        self.assertTrue(private_source('crates/os-app/src/master_assistant.rs'))
        self.assertFalse(private_source('crates/os-app/src/master_assistant_disabled.rs'))
    def test_publish_guard_runs_before_credentials_or_network(self):
        import importlib.util,json
        from unittest.mock import patch
        spec=importlib.util.spec_from_file_location('release',Path(__file__).parent/'workflow/release.py')
        release=importlib.util.module_from_spec(spec);spec.loader.exec_module(release)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);binary=root/'lemonstudio.exe';binary.write_bytes(b'MZ'+MARKER)
            plan=root/'publish-plan.json';plan.write_text(json.dumps({'scope':'windows-linux','files':[{'path':'updates/2026110105/lemonstudio.exe','local':'lemonstudio.exe','sha256':release.digest(binary)}]}))
            with patch.object(release,'read_config',side_effect=AssertionError('credentials/network must not be reached')):
                with self.assertRaises(SystemExit):release.publish(plan)
    def test_configure_does_not_reference_a_publication_plan(self):
        import importlib.util,sys,types
        from unittest.mock import patch
        spec=importlib.util.spec_from_file_location('release',Path(__file__).parent/'workflow/release.py')
        release=importlib.util.module_from_spec(spec);spec.loader.exec_module(release)
        with patch.dict(sys.modules,{'keyring':types.ModuleType('keyring')}),patch.object(release,'read_config',return_value={'publish_via_ssh':'test'}),patch.object(release,'check_connection') as check:
            release.configure();check.assert_called_once()
if __name__=='__main__':unittest.main()
