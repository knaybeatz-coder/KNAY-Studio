import importlib.util,json,os,sys,tempfile,unittest,zipfile
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).parent))
import checkpoints as c
import pipeline as p
import builders as b

class PipelineTests(unittest.TestCase):
    def test_resume_skips_success_and_retries_only_failed_step(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);artifact=root/'app.exe';artifact.write_bytes(b'compiled');calls=[]
            j=c.Journal(root/'state.json',{'version':7,'commit':'abc'})
            def build(n):calls.append(('build',n));return str(artifact),[artifact]
            j.step('build',build)
            with self.assertRaises(RuntimeError):j.step('upload',lambda n:(_ for _ in ()).throw(RuntimeError('connection lost')))
            j=c.Journal(root/'state.json',{'version':7,'commit':'abc'})
            j.step('build',build)
            j.step('upload',lambda n:(calls.append(('upload',n)) or 'done',[artifact]))
            self.assertEqual(calls,[('build',1),('upload',2)])
            self.assertIn('upload: complete',(root/'Progress.txt').read_text())
    def test_revision_and_artifact_changes_stop_without_rerunning(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);file=root/'artifact';file.write_bytes(b'original');j=c.Journal(root/'state.json',{'commit':'a'})
            j.step('compile',lambda n:('ready',[file]));file.write_bytes(b'changed')
            with self.assertRaisesRegex(RuntimeError,'changed'):j.step('compile',lambda n:self.fail('must not rebuild'))
            with self.assertRaisesRegex(RuntimeError,'identity'):c.Journal(root/'state.json',{'commit':'b'})
    def test_process_lock_rejects_second_publisher_and_releases(self):
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'run.lock'
            with c.exclusive(path):
                with self.assertRaisesRegex(RuntimeError,'publisher'): 
                    with c.exclusive(path):pass
            with c.exclusive(path):pass
    def test_staging_deduplicates_and_excludes_unplanned_files(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'a.zip').write_bytes(b'archive');(root/'private.txt').write_text('do not copy')
            plan=root/'plan.json';c.atomic_json(plan,dict(version=7,commit='abc',scope='windows-linux',files=[dict(path='updates/7/a.zip',local='a.zip',sha256=c.digest(root/'a.zip')),dict(path='updates/linux-x64/7/a.zip',local='a.zip',sha256=c.digest(root/'a.zip'))]))
            staged=c.stage_plan(plan,root/'shared');data=json.loads(staged.read_text());self.assertEqual(data['files'][0]['local'],data['files'][1]['local'])
            self.assertFalse((staged.parent/'private.txt').exists());c.stage_plan(plan,root/'shared')
            (staged.parent/data['files'][0]['local']).write_bytes(b'tampered')
            with self.assertRaisesRegex(RuntimeError,'changed'):c.stage_plan(plan,root/'shared')
    def test_public_build_overrides_personal_environment_and_isolates_caches(self):
        cfg={'cache':'public-cache','juce_source':'juce','engines':{x:{'bundle':x+'.zip'} for x in p.PLATFORMS}}
        with patch.dict(os.environ,{'LEMON_WAVE_LIBRARY':'personal.lwpack','CARGO_TARGET_DIR':'personal-cache'}),patch.object(b.r,'read_config',return_value={'llvm':'llvm'}):
            environments=[b.environment(cfg,x) for x in p.PLATFORMS]
        self.assertTrue(all(e['LEMON_WAVE_LIBRARY']=='' and e['LEMON_RELEASE']=='1' for e in environments))
        self.assertEqual(len({e['CARGO_TARGET_DIR'] for e in environments}),4)
    def test_changed_engine_rejected_before_native_build(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'modules/juce_core').mkdir(parents=True);(root/'modules/juce_core/juce_core.h').touch()
            cfg={'juce_source':str(root),'engines':{'windows':{'source':'old','bundle':'missing.zip'}}}
            with patch.object(p,'config',return_value=cfg),patch.object(p,'engine_id',return_value='new'):
                with self.assertRaisesRegex(RuntimeError,'source changed'):p.probe()
    def test_stage_rejects_path_escape(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);folder=root/'release';folder.mkdir();secret=root/'private';secret.write_bytes(b'no')
            plan=folder/'plan.json';c.atomic_json(plan,dict(files=[dict(local='../private',sha256=c.digest(secret))]))
            with self.assertRaisesRegex(RuntimeError,'Invalid'):c.plan_files(plan)
    def test_complete_step_without_expected_artifact_is_not_recorded(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);j=c.Journal(root/'state.json',{'version':1})
            with self.assertRaises(FileNotFoundError):j.step('build',lambda n:('done',[root/'missing']))
            self.assertEqual(j.data['steps']['build']['status'],'failed')

    def test_full_coordinator_restarts_failed_publish_without_rebuilding(self):
        from contextlib import ExitStack
        import threading
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);out=root/'Desktop';share=root/'share';share.mkdir();out.mkdir();source=root/'source';(source/'packaging').mkdir(parents=True);(source/'packaging/download-page.html').write_text('page')
            calls=[];barrier=threading.Barrier(4,timeout=10);failed=[False]
            def makeplan(folder,scope):
                folder=Path(folder);folder.mkdir(parents=True,exist_ok=True);file=folder/'test.bin';file.write_bytes(scope.encode())
                dest=folder/'publish-plan.json';c.atomic_json(dest,dict(version=7,commit='abc',scope=scope,files=[dict(path='mac/7/'+scope+'.bin',local='test.bin',sha256=c.digest(file))]));return dest
            def win(cfg,folder,log):calls.append('windows');barrier.wait();return makeplan(folder,'windows-linux')
            def lin(cfg,log):calls.append('linux');barrier.wait();f=out/'linux-binary';f.write_bytes(b'linux');return f
            def remote(args,log,sync=False):
                if args[0]=='worker':
                    calls.append(args[1]);barrier.wait();makeplan(share/('mac-'+args[1]),'mac-'+args[1])
                elif args[0]=='publish-worker':
                    calls.append('publish-'+args[1])
                    if args[1]=='mac-arm64' and not failed[0]:failed[0]=True;raise RuntimeError('simulated lost connection')
                    c.atomic_json(share/args[1]/'PUBLISHED.json',{'verified':True})
            def stage(plan,name):dest=c.stage_plan(plan,share/name);return str(dest),c.plan_files(dest)
            def verify(plans,folder):f=folder/'github-release.json';c.atomic_json(f,{'tag':'fixture'});return str(f),[f]
            def ghverify(plan,folder,commit=None):f=folder/'GITHUB-VERIFIED.json';c.atomic_json(f,{'url':'fixture'});return 'fixture',[f]
            def backup(*args,**kw):
                f=share/'snapshot.zip';f.write_bytes(b'snapshot');c.atomic_json(share/'LATEST.json',{'source':f.name,'sha256':c.digest(f)})
            with ExitStack() as stack:
                changes=[(p.platform,'system',lambda:'Windows'),(p,'config',lambda:{'source_sync':'fixture','juce_source':'fixture'}),(p,'probe',lambda:None),(p,'remote',remote),(p,'prepare',lambda n:None),(p,'release_notes',lambda:{}),(p,'identity',lambda:{'version':7,'commit':'abc'}),(p,'output_root',lambda:out),(p,'shared_run',lambda:share),(p,'stage',stage),(p,'verify_live',verify),(p,'github_stage',lambda *a:({'commit':'abc','tag':'fixture'},[out/'github-release.json'])),(p,'github_verify',ghverify),(p.r,'ROOT',source),(p.r,'PRIVATE',root/'private'),(p.r,'git',lambda *a:'' if a[0]=='status' else 'abc'),(p.r,'run',lambda *a:None),(p.r,'shared_folder',lambda:share),(b,'windows',win),(b,'linux_compile',lin),(b,'linux_package',lambda cfg,binary,wp,folder,log:makeplan(folder,'linux-package')),(b,'merged_plan',lambda plans,folder:makeplan(folder/'combined','windows-linux')),(b,'command',backup),(p.r,'write_plan',lambda site,folder,scope:makeplan(folder,scope))]
                for obj,name,value in changes:stack.enter_context(patch.object(obj,name,value))
                with self.assertRaisesRegex(RuntimeError,'lost connection'):p.coordinator()
                p.coordinator()
            for name in ['windows','linux','arm64','x86_64']:self.assertEqual(calls.count(name),1,name)
            self.assertEqual(calls.count('publish-mac-arm64'),2)
            self.assertEqual(calls.count('publish-mac-x86_64'),1)
            self.assertEqual(json.loads((out/'state.json').read_text())['steps']['Back up source']['status'],'complete')

if __name__=='__main__':unittest.main()
