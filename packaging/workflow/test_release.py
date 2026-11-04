import hashlib, importlib.util, json, tempfile, unittest, zipfile
from pathlib import Path
from unittest.mock import patch
import ftplib

spec=importlib.util.spec_from_file_location('release',Path(__file__).with_name('release.py'))
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)

class FTP:
    def __init__(self):self.files={};self.dirs=set();self.events=[]
    def mkd(self,p):
        if p in self.dirs:raise ftplib.error_perm('550 exists')
        self.dirs.add(p)
    def rmd(self,p):self.dirs.remove(p)
    def pwd(self):return '/'
    def cwd(self,p):pass
    def quit(self):pass
    def delete(self,p):self.files.pop(p,None)
    def retrbinary(self,cmd,callback):
        p=cmd[5:]
        if p not in self.files:raise ftplib.error_perm('550 absent')
        callback(self.files[p])
    def storbinary(self,cmd,f,size):self.files[cmd[5:]]=f.read()
    def rename(self,a,b):self.files[b]=self.files.pop(a);self.events.append(b)

class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.private=tempfile.TemporaryDirectory();self.override=patch.object(r,'PRIVATE',Path(self.private.name));self.override.start()
    def tearDown(self):self.override.stop();self.private.cleanup()

    def test_relay_copies_only_planned_files_and_returns_receipt(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan_path=self.fixture(root);plan=json.loads(plan_path.read_text())
            (root/'do-not-copy-private.txt').write_text('fixture not for relay')
            share=root/'shared';share.mkdir()
            cfg={'mac_share':'/mounted share/Lemon Studio'}
            def remote(cfg,action,path):
                self.assertEqual(action,'publish')
                relay=share/'relay'/Path(str(path)).parent.name
                self.assertFalse((relay/'do-not-copy-private.txt').exists())
                for e in plan['files']:self.assertEqual(r.digest(relay/e['local']),e['sha256'])
                (relay/'PUBLISHED.json').write_text('{"verified":true}')
            with patch.object(r,'shared_folder',return_value=share),patch.object(r,'remote_call',side_effect=remote):r.relay_publish(cfg,plan_path,plan)
            self.assertTrue(json.loads((root/'PUBLISHED.json').read_text())['verified'])
    def test_credential_file_host_mismatch_never_connects(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'hosting.env';p.write_text('FTP_HOST=unrelated.invalid\nFTP_USER=fixture\nFTP_PASS=fixture\n');p.chmod(0o600)
            with patch.object(r,'open_secure_connection') as connection:
                with self.assertRaisesRegex(RuntimeError,'different host'):r.connect({'ftp_credentials_file':str(p),'ftp_host':'web12.webprojaggt.de'})
                connection.assert_not_called()
    def test_unreachable_server_never_asks_for_password(self):
        with patch.object(r,'read_config',return_value={}),patch.object(r.ftplib,'FTP_TLS') as factory,patch.object(r.getpass,'getpass') as password:
            factory.return_value.connect.side_effect=TimeoutError('timeout')
            with self.assertRaisesRegex(RuntimeError,'No username or password was sent'):r.configure()
            password.assert_not_called();factory.return_value.close.assert_called_once()
    def test_tls_failure_never_sends_login(self):
        with patch.object(r.ftplib,'FTP_TLS') as factory:
            factory.return_value.auth.side_effect=r.ssl.SSLError('certificate verification failed')
            with self.assertRaisesRegex(RuntimeError,'encrypted connection failed'):r.open_secure_connection('example.invalid')
            factory.return_value.login.assert_not_called();factory.return_value.close.assert_called_once()
    def test_bad_password_is_distinguished_from_network_timeout(self):
        with patch.object(r.ftplib,'FTP_TLS') as factory:
            ftp=factory.return_value;ftp.login.side_effect=ftplib.error_perm('530 Login failed')
            with self.assertRaisesRegex(RuntimeError,'rejected the login'):r.login_connection(ftp,'user','fixture-only','/')
            ftp.close.assert_called_once()
    def test_scope_rejects_cross_platform_and_traversal(self):
        self.assertTrue(r.permitted('mac/123/file.dmg','mac-arm64'))
        for p in ('updates/stable.json','platforms/windows.json','platforms/mac-x86_64.json','mac/../index.html','/mac/123/a','mac\\123\\a'):
            self.assertFalse(r.permitted(p,'mac-arm64'))
    def test_unix_feeds_have_isolated_scopes(self):
        for scope,prefix in [('windows-linux','updates/linux-x64'),('mac-arm64','mac/updates/arm64'),('mac-x86_64','mac/updates/x86_64')]:
            self.assertTrue(r.permitted(prefix+'/stable.json',scope))
        self.assertTrue(r.permitted('mac/updates/arm64/123/Contents/MacOS/lemonstudio','mac-arm64'))
        self.assertFalse(r.permitted('mac/updates/x86_64/stable.json','mac-arm64'))
        self.assertFalse(r.permitted('updates/linux-x64/stable.json','mac-arm64'))
        self.assertFalse(r.permitted('mac/updates/arm64/123/Contents/../escape','mac-arm64'))

    def fixture(self,root):
        source=root/'source.zip'
        with zipfile.ZipFile(source,'w') as archive:archive.writestr('source.txt','corresponding source')
        dmg=root/'app.dmg';dmg.write_bytes(b'app')
        manifest=root/'mac-arm64.json';manifest.write_text(json.dumps({'version':123}))
        entries=[{'path':rel,'local':p.name,'sha256':r.digest(p)} for rel,p in [('platforms/mac-arm64.json',manifest),('mac/123/app.dmg',dmg),('mac/123/source.zip',source)]]
        plan=root/'publish-plan.json';plan.write_text(json.dumps({'scope':'mac-arm64','version':123,'commit':'fixture','files':entries}))
        return plan
    def test_artifacts_verified_before_manifest(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan=self.fixture(root);ftp=FTP();verified=[]
            def verify(rel,p):
                self.assertEqual(ftp.files[rel],p.read_bytes());verified.append(rel)
                if rel.startswith('platforms/'):self.assertEqual(len(verified),3)
            with patch.object(r,'connect',return_value=ftp),patch.object(r,'read_config',return_value={}),patch.object(r,'shared_folder',return_value=root/'shared'),patch.object(r,'verify_remote',side_effect=verify):r.publish(plan)
            self.assertEqual(ftp.events[-1],'platforms/mac-arm64.json');self.assertFalse(ftp.dirs & {'.publish-mac-arm64.lock'})
    def test_interrupted_own_upload_lock_can_resume(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan=self.fixture(root);ftp=FTP();lock='.publish-mac-arm64.lock';ftp.dirs.add(lock)
            r.PRIVATE.mkdir(exist_ok=True);(r.PRIVATE/'publisher-id.txt').write_text('this-computer')
            ftp.files[lock+'/owner.json']=json.dumps({'publisher':'this-computer','scope':'mac-arm64'}).encode()
            with patch.object(r,'connect',return_value=ftp),patch.object(r,'read_config',return_value={}),patch.object(r,'shared_folder',return_value=root/'shared'),patch.object(r,'verify_remote'):
                r.publish(plan)
            self.assertNotIn(lock,ftp.dirs);self.assertTrue((root/'PUBLISHED.json').exists())
    def test_other_computers_upload_lock_is_preserved(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan=self.fixture(root);ftp=FTP();lock='.publish-mac-arm64.lock';ftp.dirs.add(lock)
            ftp.files[lock+'/owner.json']=json.dumps({'publisher':'another-computer','scope':'mac-arm64'}).encode()
            with patch.object(r,'connect',return_value=ftp),patch.object(r,'read_config',return_value={}):
                with self.assertRaisesRegex(RuntimeError,'Another computer'):r.publish(plan)
            self.assertIn(lock,ftp.dirs);self.assertFalse(ftp.events)
    def test_changed_staging_rejected_before_connection(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan=self.fixture(root);(root/'app.dmg').write_bytes(b'changed')
            with patch.object(r,'connect') as connection:
                with self.assertRaises(RuntimeError):r.publish(plan)
                connection.assert_not_called()
    def test_published_artifact_collision_never_replaced(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan=self.fixture(root);ftp=FTP();ftp.files['mac/123/app.dmg']=b'prior release'
            with patch.object(r,'connect',return_value=ftp),patch.object(r,'read_config',return_value={}):
                with self.assertRaises(RuntimeError):r.publish(plan)
            self.assertEqual(ftp.files['mac/123/app.dmg'],b'prior release');self.assertNotIn('platforms/mac-arm64.json',ftp.files)
    def test_newer_manifest_cannot_be_downgraded(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);plan=self.fixture(root);ftp=FTP();ftp.files['platforms/mac-arm64.json']=b'{"version":124}'
            with patch.object(r,'connect',return_value=ftp),patch.object(r,'read_config',return_value={}),patch.object(r,'verify_remote'):
                with self.assertRaises(RuntimeError):r.publish(plan)
            self.assertEqual(ftp.files['platforms/mac-arm64.json'],b'{"version":124}')

if __name__=='__main__':unittest.main()
