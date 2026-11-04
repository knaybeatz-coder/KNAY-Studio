<?php
declare(strict_types=1);
require __DIR__.'/admin-core.php';
function ensure($ok,$why){if(!$ok){throw new RuntimeException($why);}}
function denied(callable $f,int $code){try{$f();}catch(DomainException $e){ensure($e->getCode()===$code,'wrong error');return;}throw new RuntimeException('accepted unsafe action');}
$dir=sys_get_temp_dir().'/lemon-admin-test-'.bin2hex(random_bytes(12));
try{
 // Start with the old schema/data, then prove migration preserves it.
 mkdir($dir,0700);$old=new PDO('sqlite:'.$dir.'/votes.sqlite');
 $old->exec('CREATE TABLE users(id TEXT PRIMARY KEY,created INTEGER NOT NULL); CREATE TABLE features(id INTEGER PRIMARY KEY,title TEXT NOT NULL,normalized TEXT UNIQUE NOT NULL,created INTEGER NOT NULL,author TEXT,hidden INTEGER NOT NULL DEFAULT 0);');
 $token=bin2hex(random_bytes(32));$user=hash('sha256',$token);query($old,'INSERT INTO users VALUES(?,?)',[$user,time()]);query($old,'INSERT INTO features VALUES(10,?,?,?, ?,0)',['Existing suggestion','existing suggestion',time(),$user]);$old=null;
 $db=database($dir);ensure(query($db,'SELECT title FROM features WHERE id=10')->fetchColumn()==='Existing suggestion','migration data');
 handle($db,['action'=>'vote','id'=>10,'value'=>1],'192.0.2.80',$token);
 admin_change($db,['action'=>'visibility','id'=>10,'value'=>true]);ensure(!in_array(10,array_column(board($db,$user)['items'],'id')),'hide');
 denied(fn()=>handle($db,['action'=>'vote','id'=>10,'value'=>-1],'192.0.2.80',$token),404);
 admin_change($db,['action'=>'visibility','id'=>10,'value'=>false]);
 admin_change($db,['action'=>'lock','id'=>10,'value'=>true]);denied(fn()=>handle($db,['action'=>'vote','id'=>10,'value'=>-1],'192.0.2.80',$token),403);
 admin_change($db,['action'=>'lock','id'=>10,'value'=>false]);
 admin_change($db,['action'=>'status','id'=>10,'value'=>'Completed']);ensure(query($db,'SELECT status FROM features WHERE id=10')->fetchColumn()==='Completed','status');
 denied(fn()=>admin_change($db,['action'=>'status','id'=>10,'value'=>'bad']),400);
 denied(fn()=>admin_change($db,['action'=>'block','id'=>10,'user'=>str_repeat('a',64),'value'=>true]),404);
 admin_change($db,['action'=>'block','id'=>10,'user'=>$user,'value'=>true]);
 denied(fn()=>handle($db,['action'=>'suggest','title'=>'Blocked submission'],'192.0.2.80',$token),403);
 denied(fn()=>handle($db,['action'=>'vote','id'=>1,'value'=>1],'192.0.2.80',$token),403);
 $row=array_column(board($db,$user)['items'],null,'id')[10];ensure($row['up']===0,'blocked votes excluded');
 admin_change($db,['action'=>'block','id'=>10,'user'=>$user,'value'=>false]);$row=array_column(board($db,$user)['items'],null,'id')[10];ensure($row['up']===1,'votes restored');
 ensure(count(admin_board($db)['log'])===7,'audit log');ensure(admin_voters($db,10,0)['total']===1,'voter review');
 // Public metadata/comment lifecycle and isolation from voter-only access.
 foreach(['Open','Planned','In progress','Completed','Declined'] as $status){admin_change($db,['action'=>'status','id'=>10,'value'=>$status]);ensure(array_column(board($db,'' )['items'],null,'id')[10]['status']===$status,'public status');}
 $added=admin_change($db,['action'=>'comment_add','id'=>10,'body'=>"Work in progress.\n<script>alert(1)</script>"]);
 $comment=array_column($added['items'],null,'id')[10]['comments'][0];
 ensure(array_column(board($db,'' )['items'],null,'id')[10]['comments'][0]['body']===$comment['body'],'public plain text comment');
 denied(fn()=>handle($db,['action'=>'comment_add','id'=>10,'body'=>'Unauthorized'],'192.0.2.80',$token),400);
 denied(fn()=>admin_change($db,['action'=>'comment_edit','id'=>1,'comment'=>$comment['id'],'body'=>'Wrong idea']),404);
 denied(fn()=>admin_change($db,['action'=>'comment_add','id'=>10,'body'=>'   ']),400);
 denied(fn()=>admin_change($db,['action'=>'comment_add','id'=>10,'body'=>str_repeat('x',601)]),400);
 denied(fn()=>admin_change($db,['action'=>'comment_add','id'=>10,'body'=>"bad\x00value"]),400);
 admin_change($db,['action'=>'comment_edit','id'=>10,'comment'=>$comment['id'],'body'=>'Ready to test']);
 ensure(array_column(board($db,'' )['items'],null,'id')[10]['comments'][0]['body']==='Ready to test','edit public comment');
 admin_change($db,['action'=>'visibility','id'=>10,'value'=>true]);ensure(!in_array(10,array_column(board($db,'' )['items'],'id')),'hidden comments');
 admin_change($db,['action'=>'visibility','id'=>10,'value'=>false]);
 admin_change($db,['action'=>'comment_delete','id'=>10,'comment'=>$comment['id']]);ensure(array_column(board($db,'' )['items'],null,'id')[10]['comments']===[],'remove public comment');
 for($i=0;$i<10;$i++){admin_change($db,['action'=>'comment_add','id'=>10,'body'=>'Update '.$i]);}
 denied(fn()=>admin_change($db,['action'=>'comment_add','id'=>10,'body'=>'Too many']),409);
 // Drafts never leak through the public API; owner publish/edit/unpublish/delete.
 $draft=admin_change($db,['action'=>'announcement_save','title'=>'News','body'=>"Plain <script> text\nSecond line",'published'=>false]);
 $aid=$draft['announcements'][0]['id'];ensure(board($db,'')['announcements']===[],'draft private');
 denied(fn()=>handle($db,['action'=>'announcement_save','title'=>'No','body'=>'No','published'=>true],'192.0.2.80',$token),400);
 admin_change($db,['action'=>'announcement_save','id'=>$aid,'title'=>'Audio update','body'=>'Now available','published'=>true]);
 ensure(board($db,'')['announcements'][0]['body']==='Now available','published');
 denied(fn()=>admin_change($db,['action'=>'announcement_save','title'=>'News','body'=>str_repeat('x',2001),'published'=>true]),400);
 denied(fn()=>admin_change($db,['action'=>'announcement_save','title'=>"Bad\nTitle",'body'=>'News','published'=>true]),400);
 denied(fn()=>admin_change($db,['action'=>'announcement_save','title'=>'News','body'=>'News','published'=>1]),400);
 admin_change($db,['action'=>'announcement_save','id'=>$aid,'title'=>'Audio update','body'=>'Revised','published'=>false]);ensure(board($db,'')['announcements']===[],'unpublished');
 admin_change($db,['action'=>'announcement_delete','id'=>$aid]);ensure(admin_board($db)['announcements']===[],'deleted');
 $_SESSION=['csrf'=>'test'];denied(fn()=>admin_csrf('bad'),403);admin_csrf('test');ensure(!admin_authenticated(),'no login');$_SESSION['owner_until']=time()-1;ensure(!admin_authenticated(),'expiry');
 query($db,'INSERT INTO settings VALUES(?,?)',['owner_password',password_hash('test-owner',PASSWORD_DEFAULT)]);
 for($i=0;$i<8;$i++){denied(fn()=>admin_login($db,'wrong','192.0.2.100'),401);}denied(fn()=>admin_login($db,'wrong','192.0.2.100'),429);
 echo json_encode(['ok'=>true,'checks'=>['old-schema preservation','hide and restore','lock and reopen','statuses','blocked identities','vote exclusion and restoration','audit log','voter review','CSRF','session expiry','login throttle','public statuses','public comment create/edit/remove','hidden comment isolation','comment bounds','public write denial']]);
}finally{$db=null;$old=null;foreach(glob($dir.'/*') as $file){unlink($file);}rmdir($dir);}
