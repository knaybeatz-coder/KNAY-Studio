<?php
// Isolated forum/profile tests; never create production posts or identities.
require __DIR__.'/admin-core.php';
function checkf($v,$message){if(!$v){throw new RuntimeException($message);}}
function denyf(callable $f,int $code){try{$f();}catch(DomainException $e){checkf($e->getCode()===$code,'unexpected denial '.$e->getCode());return;}throw new RuntimeException('accepted forbidden operation');}
$dir=sys_get_temp_dir().'/lemon-forum-test-'.bin2hex(random_bytes(12));
try{
 $db=database($dir);$token=bin2hex(random_bytes(32));$user=hash('sha256',$token);$ip='192.0.2.99';query($db,'INSERT INTO users(id,created) VALUES(?,?)',[$user,time()]);
 $call=fn($in)=>handle($db,$in,$ip,$token)['forum'];
 checkf($call(['action'=>'forum_list'])['total']===0,'empty forum');
 denyf(fn()=>handle($db,['action'=>'forum_create','title'=>'Test topic','body'=>'Hello'],$ip,''),401);
 denyf(fn()=>$call(['action'=>'forum_profile','name'=>'<script>','avatar'=>'']),400);
 denyf(fn()=>$call(['action'=>'forum_profile','name'=>'Test Musician','avatar'=>'bad']),400);
 $avatar=str_repeat('abc123',1024);$profile=$call(['action'=>'forum_profile','name'=>'Test Musician','avatar'=>$avatar]);checkf($profile['profile']['avatar']===$avatar,'profile avatar roundtrip');
 $page=$call(['action'=>'forum_create','title'=>'Making a first track','body'=>'How do you start?']);$tid=$page['topic']['id'];$pid=$page['posts'][0]['id'];
 checkf($page['posts'][0]['author']==='Test Musician'&&$page['posts'][0]['avatar']===$avatar,'public identity');checkf(str_starts_with($page['posts'][0]['member'],'Member '),'stable identifier');
 $reply=$call(['action'=>'forum_reply','thread'=>$tid,'parent'=>$pid,'body'=>'Start with a rhythm.']);checkf($reply['posts'][1]['parent']===$pid,'reply association');
 for($i=0;$i<3;$i++){$page=$call(['action'=>'forum_react','thread'=>$tid,'post'=>$pid,'value'=>1]);}checkf($page['posts'][0]['up']===1,'one reaction');
 $page=$call(['action'=>'forum_react','thread'=>$tid,'post'=>$pid,'value'=>-1]);checkf($page['posts'][0]['up']===0&&$page['posts'][0]['down']===1,'reaction replacement');
 $page=$call(['action'=>'forum_react','thread'=>$tid,'post'=>$pid,'value'=>0]);checkf($page['posts'][0]['down']===0,'remove reaction');
 $other=$call(['action'=>'forum_create','title'=>'Second discussion','body'=>'Another topic']);
 denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$other['topic']['id'],'parent'=>$pid,'body'=>'Wrong thread']),400);
 denyf(fn()=>$call(['action'=>'forum_react','thread'=>$other['topic']['id'],'post'=>$pid,'value'=>1]),404);
 denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$tid,'body'=>str_repeat('x',801)]),400);
 denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$tid,'body'=>'https://spam.invalid']),400);
 forum_moderate($db,['action'=>'forum_lock','id'=>$tid,'value'=>true]);denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$tid,'body'=>'No']),403);checkf($call(['action'=>'forum_list','thread'=>$tid])['total']===2,'closed readable');
 forum_moderate($db,['action'=>'forum_lock','id'=>$tid,'value'=>false]);
 forum_moderate($db,['action'=>'forum_hide_post','id'=>$pid,'value'=>true]);checkf($call(['action'=>'forum_list','thread'=>$tid])['total']===1,'hidden post excluded');denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$tid,'parent'=>$pid,'body'=>'No']),400);
 forum_moderate($db,['action'=>'forum_hide_post','id'=>$pid,'value'=>false]);
 forum_moderate($db,['action'=>'forum_clear_profile','id'=>$pid,'value'=>true]);$page=$call(['action'=>'forum_list','thread'=>$tid]);checkf($page['posts'][0]['avatar']===''&&str_starts_with($page['posts'][0]['author'],'Member '),'moderated profile');
 forum_moderate($db,['action'=>'forum_block','id'=>$pid,'value'=>true]);denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$tid,'body'=>'Blocked']),403);denyf(fn()=>$call(['action'=>'forum_profile','name'=>'Changed name','avatar'=>'']),403);
 forum_moderate($db,['action'=>'forum_block','id'=>$pid,'value'=>false]);
 for($i=0;$i<24;$i++){query($db,'INSERT INTO forum_posts(thread,author,body,created) VALUES(?,?,?,?)',[$tid,$user,'Paging fixture '.$i,time()]);}
 $page=$call(['action'=>'forum_list','thread'=>$tid,'offset'=>20]);checkf(count($page['posts'])===6&&$page['total']===26,'bounded pagination');
 forum_moderate($db,['action'=>'forum_complete_post','id'=>$pid,'value'=>true]);checkf($call(['action'=>'forum_list','thread'=>$tid])['posts'][0]['completed']===1,'completion visible');
 denyf(fn()=>$call(['action'=>'forum_complete_post','thread'=>$tid,'id'=>$pid,'value'=>false]),400);
 // A late reply is placed next to its parent, not at the end of the chronological feed.
 query($db,'DELETE FROM limits WHERE k LIKE ?', ['forum-post-minute:%']);
 $late=$call(['action'=>'forum_reply','thread'=>$tid,'parent'=>$pid,'body'=>'Late reply to opening post']);
 checkf($late['offset']===0,'reply response opens parent page');
 checkf($late['posts'][2]['body']==='Late reply to opening post'&&$late['posts'][2]['depth']===1,'grouped replies');
 checkf($late['posts'][2]['parent_body']==='How do you start?'&&$late['posts'][2]['parent_author']!=='','quoted parent');
 $lateId=$late['posts'][2]['id'];
 $nested=$call(['action'=>'forum_reply','thread'=>$tid,'parent'=>$lateId,'body'=>'Nested follow up']);
 checkf($nested['posts'][3]['depth']===2&&$nested['posts'][3]['parent_body']==='Late reply to opening post','nested context');
 forum_moderate($db,['action'=>'forum_hide_post','id'=>$pid,'value'=>true]);
 $hidden=$call(['action'=>'forum_list','thread'=>$tid]);checkf($hidden['posts'][0]['parent_body']===''&&$hidden['posts'][0]['depth']===0,'hidden parent text never leaked');
 forum_moderate($db,['action'=>'forum_hide_post','id'=>$pid,'value'=>false]);
 forum_moderate($db,['action'=>'forum_hide_thread','id'=>$tid,'value'=>true]);denyf(fn()=>$call(['action'=>'forum_list','thread'=>$tid]),404);checkf(forum_page($db,'',$tid,0,true)['total']===28,'owner hidden review');
 forum_moderate($db,['action'=>'forum_hide_thread','id'=>$tid,'value'=>false]);
 query($db,'DELETE FROM limits WHERE k LIKE ?', ['forum-post-minute:%']);
 for($i=0;$i<5;$i++){$call(['action'=>'forum_reply','thread'=>$tid,'body'=>'Rate test '.$i]);}
 $call(['action'=>'forum_reply','thread'=>$tid,'body'=>'Rate test 6']);denyf(fn()=>$call(['action'=>'forum_reply','thread'=>$tid,'body'=>'Rate test 7']),429);
 // Owner-only dashboard participation uses a distinct actor, never a public bearer token.
 $profile=owner_forum_action($db,['action'=>'owner_forum_profile_read']);checkf($profile['name']==='','empty owner profile');
 owner_forum_action($db,['action'=>'owner_forum_profile','name'=>'Michael Lane','avatar'=>$avatar]);
 $owner=owner_forum_action($db,['action'=>'owner_forum_create','title'=>'Owner welcome thread','body'=>'Welcome everyone']);$ot=$owner['topic']['id'];$op=$owner['posts'][0]['id'];
 checkf($owner['posts'][0]['author']==='Michael Lane'&&$owner['posts'][0]['member']==='Administrator'&&$owner['posts'][0]['avatar']===$avatar,'owner identity visible');
 owner_forum_action($db,['action'=>'owner_forum_reply','thread'=>$ot,'parent'=>$op,'body'=>'Administrator reply']);
 $react=owner_forum_action($db,['action'=>'owner_forum_react','thread'=>$ot,'post'=>$op,'value'=>1]);checkf($react['posts'][0]['mine']===1,'owner reaction');
 owner_forum_action($db,['action'=>'owner_forum_profile','name'=>'Michael','avatar'=>'']);
 $public=forum_page($db,'',$ot);checkf($public['posts'][0]['author']==='Michael'&&$public['posts'][0]['avatar']===''&&$public['posts'][1]['member']==='Administrator','historical owner profile update');
 denyf(fn()=>$call(['action'=>'owner_forum_profile','name'=>'Impersonator','avatar'=>'']),400);
 $spoof=$call(['action'=>'forum_create','title'=>'Ordinary member identity','body'=>'No admin privilege','user'=>FORUM_OWNER]);checkf($spoof['posts'][0]['member']!=='Administrator','client cannot choose owner actor');
 // Category migration preserves nested replies, authors and reactions; public callers cannot moderate.
 forum_moderate($db,['action'=>'forum_rename_thread','id'=>$ot,'title'=>'Bugs, Fixes, Issues','value'=>true]);
 forum_moderate($db,['action'=>'forum_organize_post','id'=>$pid,'thread'=>$ot,'category'=>1,'value'=>true]);
 $organized=forum_page($db,$user,$ot,0,false,1);checkf($organized['total']===4,'whole reply tree moved');
 checkf($organized['posts'][0]['id']===$pid&&$organized['posts'][2]['id']===$lateId&&$organized['posts'][3]['depth']===2,'reply identity and nesting preserved');
 checkf(count($organized['categories'])===9&&$organized['category']===1,'category navigation');
 denyf(fn()=>$call(['action'=>'forum_organize_post','id'=>$pid,'thread'=>$ot,'category'=>2,'value'=>true]),400);
 denyf(fn()=>$call(['action'=>'forum_list','thread'=>$ot,'category'=>99]),400);
 query($db,'DELETE FROM limits');
 $catReply=$call(['action'=>'forum_reply','thread'=>$ot,'parent'=>$pid,'category'=>2,'body'=>'Category inherited from report']);checkf($catReply['category']===1&&$catReply['total']===5,'reply inherits category');
 query($db,'INSERT INTO features(title,normalized,created,author) VALUES(?,?,?,?)',['Playback fixture','playback fixture',time(),$user]);$fid=(int)$db->lastInsertId();
 query($db,'INSERT INTO votes VALUES(?,?,?)',[$user,$fid,1]);query($db,'INSERT INTO public_comments(feature,body,created,edited) VALUES(?,?,?,?)',[$fid,'Existing owner context',time(),time()]);
 for($i=0;$i<2;$i++){forum_moderate($db,['action'=>'forum_import_bug','id'=>$fid,'thread'=>$ot,'category'=>2,'value'=>true]);}
 $imported=forum_page($db,$user,$ot,0,false,2);checkf($imported['total']===2&&$imported['posts'][0]['up']===1&&$imported['posts'][1]['parent']===$imported['posts'][0]['id'],'idempotent import keeps votes and comments');
 checkf((int)query($db,'SELECT hidden FROM features WHERE id=?',[$fid])->fetchColumn()===1,'migrated voting duplicate hidden');
 $routed=$call(['action'=>'forum_create','title'=>'Playback glitch','body'=>'One bug area']);checkf($routed['topic']['id']===$ot&&$routed['category']===8,'bug discussions routed to central area');
 echo json_encode(['ok'=>true,'checks'=>['threads','replies','cross-thread isolation','reactions','profile and avatar','moderation','blocks','pagination','rate limits','bounds','owner profile and participation','owner impersonation rejected']]);
}finally{$db=null;foreach(glob($dir.'/*')?:[] as $p){unlink($p);}if(is_dir($dir)){rmdir($dir);}}
