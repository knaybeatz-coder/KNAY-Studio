<?php
// Run with PHP CLI or an ephemeral authenticated deployment wrapper; isolated temporary DB only.
define('LEMON_VOTES_TEST',true);
require __DIR__.'/api.php';
function check($value,$message){if(!$value){throw new RuntimeException($message);}}
function rejected(callable $f,int $status){try{$f();}catch(DomainException $e){check($e->getCode()===$status,'wrong rejection');return;}throw new RuntimeException('request was accepted');}
$dir=sys_get_temp_dir().'/lemon-votes-test-'.bin2hex(random_bytes(12));
try {
 $db=database($dir);$ip='192.0.2.1';$token=bin2hex(random_bytes(32));query($db,'INSERT INTO users(id,created) VALUES(?,?)',[hash('sha256',$token),time()]);
 for($i=0;$i<20;$i++){handle($db,['action'=>'vote','id'=>1,'value'=>1],$ip,$token);}
 handle($db,['action'=>'vote','id'=>2,'value'=>-1],$ip,$token);
 $rows=board($db,hash('sha256',$token))['items'];$rows=array_column($rows,null,'id');
 check($rows[1]['up']===1 && $rows[2]['down']===1,'independent single votes');
 handle($db,['action'=>'vote','id'=>1,'value'=>-1],$ip,$token);
 $rows=array_column(board($db,hash('sha256',$token))['items'],null,'id');check($rows[1]['up']===0 && $rows[1]['down']===1,'switch replaces vote');
 rejected(fn()=>handle($db,['action'=>'vote','id'=>1,'value'=>2],$ip,$token),400);
 rejected(fn()=>handle($db,['action'=>'vote','id'=>999,'value'=>1],$ip,$token),404);
 rejected(fn()=>handle($db,['action'=>'vote','id'=>1,'value'=>1],$ip,''),401);
 rejected(fn()=>handle($db,['action'=>'vote','id'=>1,'value'=>1],$ip,str_repeat('a',64)),401);
 rejected(fn()=>handle($db,['action'=>'suggest','title'=>'<script>alert(1)</script>'],$ip,$token),400);
 rejected(fn()=>handle($db,['action'=>'suggest','title'=>str_repeat('x',121)],$ip,$token),400);
 rejected(fn()=>handle($db,['action'=>'suggest','title'=>"Hidden \u{0080} control"],$ip,$token),400);
 handle($db,['action'=>'suggest','title'=>'A new test feature'],$ip,$token);
 rejected(fn()=>handle($db,['action'=>'suggest','title'=>'a NEW test feature'],$ip,$token),409);
 handle($db,['action'=>'suggest','title'=>'Another test feature'],$ip,$token);
 handle($db,['action'=>'suggest','title'=>'Third test feature'],$ip,$token);
 rejected(fn()=>handle($db,['action'=>'suggest','title'=>'Fourth test feature'],$ip,$token),429);
 $challenge=handle($db,['action'=>'challenge'],$ip,'')['challenge'];$n=0;while(!str_starts_with(hash('sha256',$challenge.':'.$n),'0000')){$n++;}
 $new=handle($db,['action'=>'register','challenge'=>$challenge,'nonce'=>(string)$n],$ip,'');check(strlen($new['token'])===64,'secure identity');
 rejected(fn()=>handle($db,['action'=>'register','challenge'=>$challenge,'nonce'=>(string)$n],$ip,''),429);
 rejected(fn()=>handle($db,['action'=>'register','challenge'=>$challenge,'nonce'=>'bad'],'192.0.2.2',''),400);
 for($i=0;$i<3;$i++){$tok=bin2hex(random_bytes(32));query($db,'INSERT INTO users(id,created) VALUES(?,?)',[hash('sha256',$tok),time()]);handle($db,['action'=>'vote','id'=>3,'value'=>1],$ip,$tok);}
 $tok=bin2hex(random_bytes(32));query($db,'INSERT INTO users(id,created) VALUES(?,?)',[hash('sha256',$tok),time()]);rejected(fn()=>handle($db,['action'=>'vote','id'=>3,'value'=>1],$ip,$tok),429);
 echo json_encode(['ok'=>true,'checks'=>['repeated clicks','separate suggestions','direction change','invalid votes','authentication','input bounds','markup rejection','duplicates','submission limits','proof of work','challenge replay','IP identity-vote limit']]);
}finally{$db=null;foreach(glob($dir.'/*') as $file){unlink($file);}rmdir($dir);}
