<?php
// SPDX-License-Identifier: GPL-3.0-or-later
// Structured feedback. Additive tables; private diagnostics/files never enter public responses.
declare(strict_types=1);
const FEEDBACK_STATUSES=['New','Needs information','Confirmed','Fix in progress','Ready to test','Resolved'];
function feedback_schema(PDO $db): void {
 $db->exec("CREATE TABLE IF NOT EXISTS feedback_reports(id INTEGER PRIMARY KEY,author TEXT NOT NULL,title TEXT NOT NULL,kind TEXT NOT NULL,body TEXT NOT NULL,steps TEXT NOT NULL,expected TEXT NOT NULL,environment TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'New',summary TEXT NOT NULL DEFAULT '',priority TEXT NOT NULL DEFAULT 'Normal',private_note TEXT NOT NULL DEFAULT '',fix_build TEXT NOT NULL DEFAULT '',duplicate_of INTEGER,source_post INTEGER UNIQUE,hidden INTEGER NOT NULL DEFAULT 0,created INTEGER NOT NULL,updated INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS feedback_followers(report INTEGER NOT NULL REFERENCES feedback_reports(id),user TEXT NOT NULL,affected INTEGER NOT NULL DEFAULT 0,environment TEXT NOT NULL DEFAULT '',seen INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(report,user));
 CREATE TABLE IF NOT EXISTS feedback_messages(id INTEGER PRIMARY KEY,report INTEGER NOT NULL REFERENCES feedback_reports(id),author TEXT NOT NULL,body TEXT NOT NULL,created INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS feedback_events(id INTEGER PRIMARY KEY,report INTEGER NOT NULL REFERENCES feedback_reports(id),kind TEXT NOT NULL,created INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS feedback_files(id INTEGER PRIMARY KEY,report INTEGER NOT NULL REFERENCES feedback_reports(id),author TEXT NOT NULL,name TEXT NOT NULL,bytes INTEGER NOT NULL,data BLOB NOT NULL,created INTEGER NOT NULL);
 CREATE INDEX IF NOT EXISTS feedback_events_report ON feedback_events(report,id);
 CREATE INDEX IF NOT EXISTS feedback_messages_report ON feedback_messages(report,id);");
}
function feedback_text(mixed $value,int $max,bool $required=false): string {
 if(!is_string($value)||strlen($value)>$max*4||!preg_match('//u',$value)){throw new DomainException('Invalid feedback text.',400);}
 $value=trim(str_replace(["\r\n","\r"],"\n",$value));
 if(preg_match_all('/./us',$value)>$max||($required&&$value==='')||preg_match('/[\p{Cc}]/u',str_replace(["\n","\t"],'',$value))){throw new DomainException('Check the length of your feedback.',400);}
 return $value;
}
function feedback_event(PDO $db,int $id,string $kind): void {query($db,'INSERT INTO feedback_events(report,kind,created) VALUES(?,?,?)',[$id,$kind,time()]);query($db,'UPDATE feedback_reports SET updated=? WHERE id=?',[time(),$id]);}
function feedback_id(array $in,string $key='id'): int {$id=$in[$key]??0;if(!is_int($id)||$id<1){throw new DomainException('Choose a report.',400);}return $id;}
function feedback_report(PDO $db,int $id,bool $owner=false): array {
 $r=query($db,'SELECT * FROM feedback_reports WHERE id=?'.($owner?'':' AND hidden=0'),[$id])->fetch(PDO::FETCH_ASSOC);
 if(!$r){throw new DomainException('Report unavailable.',404);}return $r;
}
function feedback_page(PDO $db,array $in,string $user,bool $owner=false): array {
 $offset=$in['offset']??0;$id=$in['id']??0;$after=$in['after']??0;
 if(!is_int($offset)||$offset<0||$offset>100000||!is_int($id)||$id<0||!is_int($after)||$after<0){throw new DomainException('Invalid page.',400);}
 $where=$owner?'1=1':'r.hidden=0';$args=[];
 if($id){$where.=' AND r.id=?';$args[]=$id;}
 foreach(['status'=>FEEDBACK_STATUSES,'kind'=>['Bug','Feature request','Question']] as $key=>$allowed){$v=$in[$key]??'';if($v!==''){if(!in_array($v,$allowed,true)){throw new DomainException('Invalid filter.',400);}$where.=" AND r.$key=?";$args[]=$v;}}
 if(($in['mine']??false)===true){$where.=' AND EXISTS(SELECT 1 FROM feedback_followers f WHERE f.report=r.id AND f.user=?)';$args[]=$user;}
 if(($in['unanswered']??false)===true){$where.=" AND NOT EXISTS(SELECT 1 FROM feedback_messages m WHERE m.report=r.id AND m.author='owner:community')";}
 $search=feedback_text($in['search']??'',100);if($search!==''){$where.=' AND (r.title LIKE ? OR r.body LIKE ? OR CAST(r.id AS TEXT)=?)';$args[]='%'.$search.'%';$args[]='%'.$search.'%';$args[]=ltrim($search,'#');}
 if($after){$where.=' AND EXISTS(SELECT 1 FROM feedback_events e WHERE e.report=r.id AND e.id>?)';$args[]=$after;}
 $total=(int)query($db,'SELECT COUNT(*) FROM feedback_reports r WHERE '.$where,$args)->fetchColumn();
 $order=match($in['sort']??'updated'){'affected'=>'affected DESC,r.id DESC','id'=>'r.id ASC',default=>'r.updated DESC,r.id DESC'};
 $rows=query($db,"SELECT r.*,(SELECT COUNT(*) FROM feedback_followers f JOIN users u ON u.id=f.user WHERE f.report=r.id AND f.affected=1 AND u.blocked=0) affected,(SELECT COALESCE(MAX(e.id),0) FROM feedback_events e WHERE e.report=r.id) revision FROM feedback_reports r WHERE $where ORDER BY $order LIMIT 30 OFFSET $offset",$args)->fetchAll(PDO::FETCH_ASSOC);
 foreach($rows as &$r){$rid=(int)$r['id'];$r['id']=$rid;$r['affected']=(int)$r['affected'];$r['revision']=(int)$r['revision'];
  $follow=query($db,'SELECT affected,seen FROM feedback_followers WHERE report=? AND user=?',[$rid,$user])->fetch(PDO::FETCH_ASSOC);$r['following']=(bool)$follow;$r['mine_affected']=(bool)($follow['affected']??false);$r['unread']=$follow&&$r['revision']>(int)$follow['seen'];$r['is_author']=$user===$r['author'];
  $r['messages']=[];$r['files']=[];
  if($id){$messages=query($db,'SELECT m.id,m.body,m.created,m.author FROM feedback_messages m WHERE report=? ORDER BY id DESC LIMIT 100',[$rid])->fetchAll(PDO::FETCH_ASSOC);foreach($messages as &$m){$m['owner']=$m['author']===FORUM_OWNER;unset($m['author']);}unset($m);$r['messages']=array_reverse($messages);
   if($owner||$r['is_author']){$r['files']=query($db,'SELECT id,name,bytes FROM feedback_files WHERE report=?'.($owner?'':' AND author=?'),$owner?[$rid]:[$rid,$user])->fetchAll(PDO::FETCH_ASSOC);}
  }
  if($owner){$r['environments']=query($db,'SELECT environment FROM feedback_followers WHERE report=? AND affected=1 AND environment<>\'\' LIMIT 100',[$rid])->fetchAll(PDO::FETCH_COLUMN);}
  else{unset($r['author'],$r['private_note'],$r['priority']);if(!$r['is_author'])unset($r['environment']);}
 }
 unset($r);
 $unread=(int)query($db,'SELECT COUNT(*) FROM feedback_followers f JOIN feedback_reports r ON r.id=f.report WHERE f.user=? AND r.hidden=0 AND EXISTS(SELECT 1 FROM feedback_events e WHERE e.report=f.report AND e.id>f.seen)',[$user])->fetchColumn();
 return ['reports'=>$rows,'total'=>$total,'offset'=>$offset,'unread'=>$unread,'cursor'=>(int)query($db,'SELECT COALESCE(MAX(id),0) FROM feedback_events')->fetchColumn(),'statuses'=>FEEDBACK_STATUSES];
}
function feedback_attachment(PDO $db,array $in,string $user,int $id): void {
 $file=$in['file']??null;if($file===null)return;
 if(!is_array($file)){throw new DomainException('Invalid attachment.',400);}
 $name=feedback_text($file['name']??'',120,true);$name=basename(str_replace('\\','/',$name));
 if(!preg_match('/\.(txt|log|json|png|jpg|jpeg|wav|flp|lmn)$/i',$name)){throw new DomainException('Use a log, text, JSON, picture, WAV or project file.',400);}
 $encoded=$file['data']??'';if(!is_string($encoded)||strlen($encoded)>2800000){throw new DomainException('Attachments must be at most 2 MB.',413);}
 $data=base64_decode($encoded,true);if($data===false||strlen($data)>2097152||strlen($data)===0){throw new DomainException('Invalid attachment or file too large.',400);}
 if((int)query($db,'SELECT COUNT(*) FROM feedback_files WHERE report=?',[$id])->fetchColumn()>=3||(int)query($db,'SELECT COALESCE(SUM(bytes),0) FROM feedback_files')->fetchColumn()+strlen($data)>209715200){throw new DomainException('Attachment storage limit reached. Send text details instead.',429);}
 query($db,'INSERT INTO feedback_files(report,author,name,bytes,data,created) VALUES(?,?,?,?,?,?)',[$id,$user,$name,strlen($data),$data,time()]);
}
function feedback_handle(PDO $db,array $in,string $user,string $peer): array {
 $action=$in['action'];if($action==='feedback_list')return feedback_page($db,$in,$user);
 if(!$user){throw new DomainException('Connect before sending feedback.',401);}
 if(query($db,'SELECT blocked FROM users WHERE id=?',[$user])->fetchColumn()){throw new DomainException('This identity is blocked.',403);}
 if($action==='feedback_create'){
  $title=feedback_text($in['title']??'',120,true);$body=feedback_text($in['body']??'',4000,true);$kind=$in['kind']??'Bug';
  if(!in_array($kind,['Bug','Feature request','Question'],true)){throw new DomainException('Choose a feedback type.',400);}
  limit_request($db,'feedback-create:'.$user,5,86400);limit_request($db,'feedback-create-ip:'.$peer,15,86400);
  if((int)query($db,'SELECT COUNT(*) FROM feedback_reports')->fetchColumn()>=10000){throw new DomainException('Feedback inbox is full.',429);}
  query($db,'INSERT INTO feedback_reports(author,title,kind,body,steps,expected,environment,created,updated) VALUES(?,?,?,?,?,?,?,?,?)',[$user,$title,$kind,$body,feedback_text($in['steps']??'',4000),feedback_text($in['expected']??'',2000),feedback_text($in['environment']??'',4000),time(),time()]);$id=(int)$db->lastInsertId();
  query($db,'INSERT INTO feedback_followers(report,user,affected) VALUES(?,?,?)',[$id,$user,(int)($kind==='Bug')]);feedback_attachment($db,$in,$user,$id);feedback_event($db,$id,'created');
 }else{
  $id=feedback_id($in);$report=feedback_report($db,$id);
  if($action==='feedback_follow'||$action==='feedback_affected'){
   limit_request($db,'feedback-follow:'.$user,30,60);$on=$in['value']??null;if(!is_bool($on)){throw new DomainException('Invalid setting.',400);}
   if($action==='feedback_follow'&&!$on){query($db,'DELETE FROM feedback_followers WHERE report=? AND user=?',[$id,$user]);}
   else {query($db,'INSERT OR IGNORE INTO feedback_followers(report,user) VALUES(?,?)',[$id,$user]);if($action==='feedback_affected'){query($db,'UPDATE feedback_followers SET affected=?,environment=? WHERE report=? AND user=?',[(int)$on,feedback_text($in['environment']??'',4000),$id,$user]);feedback_event($db,$id,'affected');}}
  }elseif($action==='feedback_reply'){
   limit_request($db,'feedback-reply:'.$user,6,60);limit_request($db,'feedback-reply-day:'.$user,60,86400);
   if((int)query($db,'SELECT COUNT(*) FROM feedback_messages WHERE report=?',[$id])->fetchColumn()>=100){throw new DomainException('This report has reached its reply limit.',429);}
   query($db,'INSERT INTO feedback_messages(report,author,body,created) VALUES(?,?,?,?)',[$id,$user,feedback_text($in['body']??'',4000,true),time()]);query($db,'INSERT OR IGNORE INTO feedback_followers(report,user) VALUES(?,?)',[$id,$user]);feedback_attachment($db,$in,$user,$id);feedback_event($db,$id,'reply');
  }elseif($action==='feedback_seen'){query($db,'UPDATE feedback_followers SET seen=COALESCE((SELECT MAX(id) FROM feedback_events WHERE report=?),0) WHERE report=? AND user=?',[$id,$id,$user]);}
  else{throw new DomainException('Invalid feedback action.',400);}
 }
 return feedback_page($db,['id'=>$id],$user);
}
function feedback_owner(PDO $db,array $in): array {
 $action=$in['action'];if($action==='feedback_review')return feedback_page($db,$in,FORUM_OWNER,true);
 if($action==='feedback_file'){$f=query($db,'SELECT name,data FROM feedback_files WHERE id=?',[feedback_id($in)])->fetch(PDO::FETCH_ASSOC);if(!$f)throw new DomainException('File unavailable.',404);return ['name'=>$f['name'],'data'=>base64_encode($f['data'])];}
 $db->exec('BEGIN IMMEDIATE');try{
  if($action==='feedback_import'){
   $post=feedback_id($in,'post');$p=query($db,'SELECT p.* FROM forum_posts p JOIN forum_threads t ON t.id=p.thread WHERE p.id=? AND p.hidden=0 AND t.hidden=0',[$post])->fetch(PDO::FETCH_ASSOC);if(!$p)throw new DomainException('Forum post unavailable.',404);
   $id=(int)query($db,'SELECT id FROM feedback_reports WHERE source_post=?',[$post])->fetchColumn();
   if(!$id){query($db,'INSERT INTO feedback_reports(author,title,kind,body,steps,expected,environment,source_post,created,updated) VALUES(?,?,?,?,?,?,?,?,?,?)',[$p['author'],feedback_text($in['title']??'',120,true),'Bug',$p['body'],'','',feedback_text($in['environment']??'',4000),$post,$p['created'],time()]);$id=(int)$db->lastInsertId();feedback_event($db,$id,'imported');}
  }else{
   $id=feedback_id($in);feedback_report($db,$id,true);
   if($action==='feedback_update'){
    $status=$in['status']??'';$priority=$in['priority']??'Normal';if(!in_array($status,FEEDBACK_STATUSES,true)||!in_array($priority,['Critical','High','Normal','Low'],true))throw new DomainException('Invalid status or priority.',400);
    $dup=$in['duplicate_of']??null;if($dup!==null){if(!is_int($dup)||$dup===$id)throw new DomainException('Invalid duplicate.',400);feedback_report($db,$dup,true);}
    $hidden=$in['hidden']??false;if(!is_bool($hidden))throw new DomainException('Invalid visibility.',400);
    query($db,'UPDATE feedback_reports SET status=?,summary=?,priority=?,private_note=?,fix_build=?,duplicate_of=?,hidden=? WHERE id=?',[$status,feedback_text($in['summary']??'',2000),$priority,feedback_text($in['private_note']??'',4000),feedback_text($in['fix_build']??'',80),$dup,(int)$hidden,$id]);feedback_event($db,$id,'status');
   }elseif($action==='feedback_owner_reply'){
    if((int)query($db,'SELECT COUNT(*) FROM feedback_messages WHERE report=?',[$id])->fetchColumn()>=100)throw new DomainException('Reply limit reached.',429);
    query($db,'INSERT INTO feedback_messages(report,author,body,created) VALUES(?,?,?,?)',[$id,FORUM_OWNER,feedback_text($in['body']??'',4000,true),time()]);feedback_event($db,$id,'owner_reply');
   }else{throw new DomainException('Invalid owner feedback action.',400);}
  }
  query($db,'INSERT INTO moderation_log(created,action,target,detail) VALUES(?,?,?,?)',[time(),$action,(string)$id,'Feedback workflow update']);$db->exec('COMMIT');return feedback_page($db,['id'=>$id],FORUM_OWNER,true);
 }catch(Throwable $e){$db->exec('ROLLBACK');throw $e;}
}
