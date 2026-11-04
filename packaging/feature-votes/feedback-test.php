<?php
// Run in an isolated database only, never on public reports.
require __DIR__.'/admin-core.php';
function cf(bool $ok,string $message): void {if(!$ok)throw new RuntimeException($message);}
function df(callable $f,int $code): void {try{$f();}catch(DomainException $e){cf($e->getCode()===$code,'wrong denial');return;}throw new RuntimeException('missing denial');}
$dir=sys_get_temp_dir().'/lemon-feedback-test-'.bin2hex(random_bytes(12));
try{
 $db=database($dir);$t=bin2hex(random_bytes(32));$u=hash('sha256',$t);$t2=bin2hex(random_bytes(32));$u2=hash('sha256',$t2);
 foreach([$u,$u2] as $user)query($db,'INSERT INTO users(id,created) VALUES(?,?)',[$user,time()]);
 $call=fn($in)=>handle($db,$in,'192.0.2.18',$t)['feedback'];$other=fn($in)=>handle($db,$in,'192.0.2.19',$t2)['feedback'];
 df(fn()=>handle($db,['action'=>'feedback_create'],'192.0.2.21',''),401);
 $d=$call(['action'=>'feedback_create','title'=>'Silent recording','body'=>'A short incomplete report','environment'=>'PRIVATE DEVICE','file'=>['name'=>'test.log','data'=>base64_encode('PRIVATE LOG')]]);$id=$d['reports'][0]['id'];
 cf($d['reports'][0]['status']==='New','initial status');cf($d['reports'][0]['environment']==='PRIVATE DEVICE','author diagnostic access');
 $public=handle($db,['action'=>'feedback_list','id'=>$id],'192.0.2.22','')['feedback'];cf(!str_contains(json_encode($public),'PRIVATE'),'private data leaked');cf(!isset($public['reports'][0]['author']),'identity leaked');cf($public['reports'][0]['files']===[],'file metadata leaked');
 for($i=0;$i<3;$i++)$d=$other(['action'=>'feedback_affected','id'=>$id,'value'=>true,'environment'=>'SECOND DEVICE']);cf($d['reports'][0]['affected']===2,'confirmation not unique');
 $owner=feedback_owner($db,['action'=>'feedback_review','id'=>$id]);cf($owner['reports'][0]['environment']==='PRIVATE DEVICE','owner diagnostics');$file=feedback_owner($db,['action'=>'feedback_file','id'=>(int)$owner['reports'][0]['files'][0]['id']]);cf(base64_decode($file['data'])==='PRIVATE LOG','private file download');
 $other(['action'=>'feedback_seen','id'=>$id]);$cursor=$owner['cursor'];
 feedback_owner($db,['action'=>'feedback_update','id'=>$id,'status'=>'Ready to test','summary'=>'Please test the new build','private_note'=>'PRIVATE INVESTIGATION','fix_build'=>'test-build']);
 $d=$other(['action'=>'feedback_list','mine'=>true]);cf($d['unread']===1,'follower notification');cf(!str_contains(json_encode($d),'PRIVATE'),'private notes leaked');
 cf(feedback_owner($db,['action'=>'feedback_review','after'=>$cursor])['total']===1,'incremental triage');
 feedback_owner($db,['action'=>'feedback_owner_reply','id'=>$id,'body'=>'Please send reproduction steps']);
 $d=$call(['action'=>'feedback_reply','id'=>$id,'body'=>'Fixed for me']);cf(count($d['reports'][0]['messages'])===2,'follow-up messages');
 cf(feedback_owner($db,['action'=>'feedback_review','unanswered'=>true])['total']===0,'unanswered filter');
 query($db,'UPDATE users SET blocked=1 WHERE id=?',[$u2]);df(fn()=>$other(['action'=>'feedback_reply','id'=>$id,'body'=>'blocked']),403);cf($call(['action'=>'feedback_list','id'=>$id])['reports'][0]['affected']===1,'blocked confirmation excluded');
 feedback_owner($db,['action'=>'feedback_update','id'=>$id,'status'=>'Confirmed','hidden'=>true]);cf($call(['action'=>'feedback_list'])['total']===0,'hidden report leaked');df(fn()=>$call(['action'=>'feedback_reply','id'=>$id,'body'=>'hidden']),404);
 df(fn()=>$call(['action'=>'feedback_create','title'=>'bad attachment','body'=>'test','file'=>['name'=>'evil.php','data'=>base64_encode('test')]]),400);
 cf((int)query($db,'SELECT COUNT(*) FROM feedback_reports')->fetchColumn()===1,'failed attachment did not roll back report');
 df(fn()=>handle($db,['action'=>'feedback_update','id'=>$id,'status'=>'Resolved'],'192.0.2.18',$t),404);
 $forum=handle($db,['action'=>'forum_create','title'=>'Existing discussion','body'=>'Original issue'],'192.0.2.18',$t)['forum'];$post=$forum['posts'][0]['id'];
 $import=feedback_owner($db,['action'=>'feedback_import','post'=>$post,'title'=>'Imported issue']);$repeat=feedback_owner($db,['action'=>'feedback_import','post'=>$post,'title'=>'Imported issue']);cf($import['reports'][0]['id']===$repeat['reports'][0]['id'],'duplicate forum import');cf((int)query($db,'SELECT COUNT(*) FROM forum_posts WHERE id=?',[$post])->fetchColumn()===1,'original forum post changed');
 echo json_encode(['ok'=>true,'checks'=>['incomplete report','private diagnostics and file isolation','unique confirmations','owner updates','incremental export','notifications','follow-up replies','blocked identities','hidden reports','attachment rollback','public moderation denied','idempotent forum import']]);
}finally{$db=null;foreach(glob($dir.'/*') as $p)unlink($p);if(is_dir($dir))rmdir($dir);}
