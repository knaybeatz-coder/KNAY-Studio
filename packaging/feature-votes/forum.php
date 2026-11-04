<?php
// SPDX-License-Identifier: GPL-3.0-or-later
// Shared anonymous identities, plain text, bounded pages. Called within API transactions.
const FORUM_OWNER='owner:community';
const FORUM_CATEGORIES=[1=>'Playback & audio',2=>'Plugins & ARA',3=>'Recording',4=>'Interface & windows',5=>'Editing & automation',6=>'Updates',7=>'Lemon Chop',8=>'Other issues',9=>'General requests'];
function forum_category($value): int {if(!is_int($value)||($value!==0&&!isset(FORUM_CATEGORIES[$value]))){throw new DomainException('Invalid category.',400);}return $value;}
function forum_schema(PDO $db): void {
 $db->exec('CREATE TABLE IF NOT EXISTS forum_profiles(user TEXT PRIMARY KEY REFERENCES users(id),name TEXT NOT NULL,avatar TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS forum_threads(id INTEGER PRIMARY KEY,title TEXT NOT NULL,author TEXT NOT NULL REFERENCES users(id),created INTEGER NOT NULL,updated INTEGER NOT NULL,hidden INTEGER NOT NULL DEFAULT 0,locked INTEGER NOT NULL DEFAULT 0);
 CREATE TABLE IF NOT EXISTS forum_posts(id INTEGER PRIMARY KEY,thread INTEGER NOT NULL REFERENCES forum_threads(id),parent INTEGER REFERENCES forum_posts(id),author TEXT NOT NULL REFERENCES users(id),body TEXT NOT NULL,created INTEGER NOT NULL,hidden INTEGER NOT NULL DEFAULT 0);
 CREATE INDEX IF NOT EXISTS forum_thread_posts ON forum_posts(thread,id);
 CREATE TABLE IF NOT EXISTS forum_reactions(user TEXT NOT NULL REFERENCES users(id),post INTEGER NOT NULL REFERENCES forum_posts(id),value INTEGER NOT NULL CHECK(value IN(-1,1)),PRIMARY KEY(user,post));');
 $columns=$db->query('PRAGMA table_info(forum_posts)')->fetchAll(PDO::FETCH_ASSOC);if(!in_array('completed',array_column($columns,'name'),true)){$db->exec('ALTER TABLE forum_posts ADD COLUMN completed INTEGER NOT NULL DEFAULT 0');}
 foreach(['category'=>'INTEGER NOT NULL DEFAULT 0','source_feature'=>'INTEGER'] as $name=>$type){if(!in_array($name,array_column($columns,'name'),true)){$db->exec('ALTER TABLE forum_posts ADD COLUMN '.$name.' '.$type);}}
 $db->exec('CREATE UNIQUE INDEX IF NOT EXISTS forum_source_feature ON forum_posts(source_feature)');
}
function forum_text($value,int $min,int $max): string {
 if(!is_string($value)||strlen($value)>$max*4||!preg_match('//u',$value)){throw new DomainException('Invalid forum text.',400);}
 $value=trim(str_replace(["\r\n","\r"],"\n",$value));$n=preg_match_all('/./us',$value);
 if($n<$min||$n>$max||preg_match('/[\p{C}<>]|https?:|www\./iu',str_replace(["\n","\t"],'',$value))){throw new DomainException('Use '.$min.'-'.$max.' characters, without links or markup.',400);}
 return $value;
}
function forum_alias(PDO $db,string $user): string {if($user===FORUM_OWNER){return 'Administrator';}return 'Member '.strtoupper(substr(hash_hmac('sha256','forum:'.$user,query($db,'SELECT v FROM settings WHERE k=?',['secret'])->fetchColumn()),0,10));}
function forum_profile(PDO $db,string $user): array {return query($db,'SELECT name,avatar FROM forum_profiles WHERE user=?',[$user])->fetch(PDO::FETCH_ASSOC)?:['name'=>'','avatar'=>''];}
function forum_identity(PDO $db,string $user): array {$p=forum_profile($db,$user);return ['author'=>$p['name']?:forum_alias($db,$user),'member'=>forum_alias($db,$user),'avatar'=>$p['avatar']];}
// Stable depth-first reply ordering, with hidden/missing parents promoted safely.
function forum_order(PDO $db,int $thread,bool $owner=false,int $category=0): array {
 $rows=query($db,'SELECT id,parent FROM forum_posts WHERE thread=?'.($owner?'':' AND hidden=0').($category?' AND category='.(int)$category:'').' ORDER BY id',[$thread])->fetchAll(PDO::FETCH_ASSOC);
 $ids=array_fill_keys(array_column($rows,'id'),true);$children=[];
 foreach($rows as $row){$parent=(int)($row['parent']??0);if(!isset($ids[$parent])||$parent>=(int)$row['id']){$parent=0;}$children[$parent][]=(int)$row['id'];}
 $stack=[];foreach(array_reverse($children[0]??[]) as $id){$stack[]=[$id,0];}$order=[];
 while($stack){[$id,$depth]=array_pop($stack);$order[$id]=$depth;foreach(array_reverse($children[$id]??[]) as $child){$stack[]=[$child,$depth+1];}}
 return $order;
}
function forum_page(PDO $db,string $user,int $thread=0,int $offset=0,bool $owner=false,int $category=0): array {
 forum_category($category);
 if($thread<0||$offset<0||$offset>1000000){throw new DomainException('Invalid forum page.',400);}
 $data=['category'=>$category,'categories'=>[],'threads'=>[],'posts'=>[],'topic'=>null,'total'=>0,'offset'=>$offset,'profile'=>forum_profile($db,$user)];
 if(!$thread){
  $where=$owner?'':' WHERE t.hidden=0';
  $data['total']=(int)query($db,'SELECT COUNT(*) FROM forum_threads t'.$where)->fetchColumn();
  $rows=query($db,'SELECT t.*,(SELECT COUNT(*) FROM forum_posts p WHERE p.thread=t.id AND p.hidden=0) AS posts FROM forum_threads t'.$where.' ORDER BY t.updated DESC,t.id DESC LIMIT 20 OFFSET '.$offset)->fetchAll(PDO::FETCH_ASSOC);
  foreach($rows as &$row){$row=array_merge($row,forum_identity($db,$row['author']));foreach(['id','created','updated','hidden','locked','posts'] as $k){$row[$k]=(int)$row[$k];}}$data['threads']=$rows;
 } else {
  $topic=query($db,'SELECT t.*,(SELECT COUNT(*) FROM forum_posts p WHERE p.thread=t.id AND p.hidden=0) AS posts FROM forum_threads t WHERE t.id=?'.($owner?'':' AND t.hidden=0'),[$thread])->fetch(PDO::FETCH_ASSOC);
  if(!$topic){throw new DomainException('This thread is no longer available.',404);}
  $topic=array_merge($topic,forum_identity($db,$topic['author']));foreach(['id','created','updated','hidden','locked','posts'] as $k){$topic[$k]=(int)$topic[$k];}$data['topic']=$topic;
  $where=' WHERE p.thread=?'.($owner?'':' AND p.hidden=0').($category?' AND p.category='.(int)$category:'');
  foreach(FORUM_CATEGORIES as $key=>$label){$n=(int)query($db,'SELECT COUNT(*) FROM forum_posts WHERE thread=? AND category=?'.($owner?'':' AND hidden=0'),[$thread,$key])->fetchColumn();$data['categories'][]=['id'=>$key,'name'=>$label,'posts'=>$n];}
  $data['total']=(int)query($db,'SELECT COUNT(*) FROM forum_posts p'.$where,[$thread])->fetchColumn();
  $order=forum_order($db,$thread,$owner,$category);$pageIds=array_slice(array_keys($order),$offset,20);
  $where.=' AND p.id IN ('.implode(',', $pageIds?:[0]).')';
  $rows=query($db,'SELECT p.*,u.blocked,COALESCE((SELECT SUM(r.value=1) FROM forum_reactions r JOIN users v ON v.id=r.user WHERE r.post=p.id AND v.blocked=0),0) AS up,COALESCE((SELECT SUM(r.value=-1) FROM forum_reactions r JOIN users v ON v.id=r.user WHERE r.post=p.id AND v.blocked=0),0) AS down,COALESCE((SELECT value FROM forum_reactions WHERE post=p.id AND user=?),0) AS mine FROM forum_posts p JOIN users u ON u.id=p.author'.$where.' ORDER BY p.id',[$user,$thread])->fetchAll(PDO::FETCH_ASSOC);
  foreach($rows as &$row){$row=array_merge($row,forum_identity($db,$row['author']));foreach(['id','thread','created','hidden','blocked','completed','up','down','mine'] as $k){$row[$k]=(int)$row[$k];}$row['parent']=$row['parent']===null?null:(int)$row['parent'];}
  $byId=array_column($rows,null,'id');$rows=[];
  foreach($pageIds as $id){$row=$byId[$id];$row['depth']=$order[$id];$row['parent_author']='';$row['parent_body']='';
   if($row['parent']!==null){$parent=query($db,'SELECT author,body FROM forum_posts WHERE id=? AND thread=? AND hidden=0',[$row['parent'],$thread])->fetch(PDO::FETCH_ASSOC);if($parent){$row['parent_author']=forum_identity($db,$parent['author'])['author'];$row['parent_body']=$parent['body'];}}
   $rows[]=$row;
  }$data['posts']=$rows;
 }
 return $data;
}
function forum_handle(PDO $db,array $in,string $user,string $peer): array {
 $action=$in['action'];$thread=$in['thread']??0;$offset=$in['offset']??0;$category=forum_category($in['category']??0);
 if(!is_int($thread)||!is_int($offset)){throw new DomainException('Invalid forum page.',400);}
 if($action==='forum_list'){return forum_page($db,$user,$thread,$offset,false,$category);}
 if(!$user){throw new DomainException('Connect before posting.',401);}
 if($action==='forum_profile'){
  $name=forum_text($in['name']??'',2,32);$avatar=$in['avatar']??'';
  if(str_contains($name,"\n")||!is_string($avatar)||($avatar!==''&&!preg_match('/^[a-f0-9]{6144}$/D',$avatar))){throw new DomainException('Invalid profile name or picture.',400);}
  limit_request($db,'forum-profile:'.$user,10,3600);
  query($db,'INSERT INTO forum_profiles VALUES(?,?,?) ON CONFLICT(user) DO UPDATE SET name=excluded.name,avatar=excluded.avatar',[$user,$name,$avatar]);
  return forum_page($db,$user,$thread,$offset,false,$category);
 }
 if($action==='forum_create'){
  $title=forum_text($in['title']??'',4,100);$body=forum_text($in['body']??'',1,800);
  if(str_contains($title,"\n")){throw new DomainException('Use one line for the title.',400);}
  limit_request($db,'forum-thread-user:'.$user,5,86400);limit_request($db,'forum-thread-net:'.$peer,10,86400);
  if(query($db,'SELECT COUNT(*) FROM forum_threads')->fetchColumn()>=10000){throw new DomainException('Forum is full for now.',429);}
  $bugThread=(int)query($db,"SELECT id FROM forum_threads WHERE title='Bugs, Fixes, Issues' AND hidden=0 AND locked=0 LIMIT 1")->fetchColumn();
  if($bugThread&&($category!==0||preg_match('/\b(bugs?|crash(?:es|ing)?|glitch(?:es)?|issues?)\b/i',$title))){$thread=$bugThread;$category=$category?:8;
   query($db,'INSERT INTO forum_posts(thread,author,body,created,category) VALUES(?,?,?,?,?)',[$thread,$user,$body,time(),$category]);query($db,'UPDATE forum_threads SET updated=? WHERE id=?',[time(),$thread]);return forum_page($db,$user,$thread,0,false,$category);
  }
  query($db,'INSERT INTO forum_threads(title,author,created,updated) VALUES(?,?,?,?)',[$title,$user,time(),time()]);$thread=(int)$db->lastInsertId();
  query($db,'INSERT INTO forum_posts(thread,author,body,created) VALUES(?,?,?,?)',[$thread,$user,$body,time()]);
  return forum_page($db,$user,$thread);
 }
 $topic=query($db,'SELECT * FROM forum_threads WHERE id=? AND hidden=0',[$thread])->fetch(PDO::FETCH_ASSOC);
 if(!$topic){throw new DomainException('Thread unavailable.',404);}
 if($topic['locked']){throw new DomainException('This thread is closed. You can still read it.',403);}
 if($action==='forum_reply'){
  $body=forum_text($in['body']??'',1,800);$parent=$in['parent']??null;
  if($parent!==null && (!is_int($parent)||!query($db,'SELECT 1 FROM forum_posts WHERE id=? AND thread=? AND hidden=0',[$parent,$thread])->fetchColumn())){throw new DomainException('Reply target unavailable.',400);}
  if($parent!==null){$category=(int)query($db,'SELECT category FROM forum_posts WHERE id=?',[$parent])->fetchColumn();}
  if($category===0&&$topic['title']==='Bugs, Fixes, Issues'){$category=8;}
  limit_request($db,'forum-post-minute:'.$user,6,60);limit_request($db,'forum-post-day:'.$user,100,86400);limit_request($db,'forum-post-net:'.$peer,200,86400);
  if(query($db,'SELECT COUNT(*) FROM forum_posts WHERE thread=?',[$thread])->fetchColumn()>=1000){throw new DomainException('This thread is full. Start a new thread.',409);}
  query($db,'INSERT INTO forum_posts(thread,parent,author,body,created,category) VALUES(?,?,?,?,?,?)',[$thread,$parent,$user,$body,time(),$category]);$posted=(int)$db->lastInsertId();query($db,'UPDATE forum_threads SET updated=? WHERE id=?',[time(),$thread]);
  $count=(int)query($db,'SELECT COUNT(*) FROM forum_posts WHERE thread=? AND hidden=0',[$thread])->fetchColumn();$position=array_search($posted,array_keys(forum_order($db,$thread,false,$category)),true);$offset=intdiv($position===false?max(0,$count-1):$position,20)*20;
 } elseif($action==='forum_react'){
  $post=$in['post']??null;$value=$in['value']??null;
  if(!is_int($post)||!in_array($value,[-1,0,1],true)){throw new DomainException('Invalid reaction.',400);}
  if(!query($db,'SELECT 1 FROM forum_posts WHERE id=? AND thread=? AND hidden=0',[$post,$thread])->fetchColumn()){throw new DomainException('Post unavailable.',404);}
  limit_request($db,'forum-react:'.$user,40,60);
  if($value===0){query($db,'DELETE FROM forum_reactions WHERE user=? AND post=?',[$user,$post]);}
  else{query($db,'INSERT INTO forum_reactions VALUES(?,?,?) ON CONFLICT(user,post) DO UPDATE SET value=excluded.value',[$user,$post,$value]);}
 }else{throw new DomainException('Invalid forum action.',400);}
 return forum_page($db,$user,$thread,$offset,false,$category);
}
function forum_moderate(PDO $db,array $in): array {
 $action=$in['action'];$id=$in['id']??null;$value=$in['value']??null;
 if(!is_int($id)||$id<1||!is_bool($value)){throw new DomainException('Invalid forum moderation.',400);}
 $db->exec('BEGIN IMMEDIATE');
 try {
  if($action==='forum_organize_post'){
   $category=forum_category($in['category']??0);$target=$in['thread']??0;
   if(!is_int($target)||!query($db,'SELECT 1 FROM forum_threads WHERE id=? AND hidden=0',[$target])->fetchColumn()){throw new DomainException('Destination unavailable.',400);}
   $root=query($db,'SELECT thread,parent FROM forum_posts WHERE id=?',[$id])->fetch(PDO::FETCH_ASSOC);if(!$root){throw new DomainException('Post unavailable.',404);}
   if($root['parent']!==null){throw new DomainException('Move the original report and its replies together.',400);}
   $ids=[$id];for($i=0;$i<count($ids);$i++){foreach(query($db,'SELECT id FROM forum_posts WHERE parent=? AND thread=?',[$ids[$i],$root['thread']])->fetchAll(PDO::FETCH_COLUMN) as $child){if(!in_array((int)$child,$ids,true)){$ids[]=(int)$child;}}}
   foreach($ids as $post){query($db,'UPDATE forum_posts SET thread=?,category=? WHERE id=?',[$target,$category,$post]);}
  }elseif($action==='forum_rename_thread'){
   query($db,'UPDATE forum_threads SET title=? WHERE id=?',[forum_text($in['title']??'',4,100),$id]);
  }elseif($action==='forum_import_bug'){
   $target=$in['thread']??0;$category=forum_category($in['category']??0);
   if(!is_int($target)||!query($db,'SELECT 1 FROM forum_threads WHERE id=? AND hidden=0 AND locked=0',[$target])->fetchColumn()){throw new DomainException('Destination unavailable.',400);}
   $feature=query($db,'SELECT * FROM features WHERE id=?',[$id])->fetch(PDO::FETCH_ASSOC);if(!$feature){throw new DomainException('Suggestion unavailable.',404);}
   if(!query($db,'SELECT id FROM forum_posts WHERE source_feature=?',[$id])->fetchColumn()){
    query($db,'INSERT INTO forum_posts(thread,author,body,created,category,source_feature) VALUES(?,?,?,?,?,?)',[$target,$feature['author'],$feature['title'],$feature['created'],$category,$id]);$post=(int)$db->lastInsertId();
    query($db,'INSERT INTO forum_reactions(user,post,value) SELECT user,?,value FROM votes WHERE feature=?',[$post,$id]);
    foreach(query($db,'SELECT body,created FROM public_comments WHERE feature=? ORDER BY id',[$id])->fetchAll(PDO::FETCH_ASSOC) as $comment){query($db,'INSERT INTO forum_posts(thread,parent,author,body,created,category) VALUES(?,?,?,?,?,?)',[$target,$post,FORUM_OWNER,$comment['body'],$comment['created'],$category]);}
   }
   query($db,'UPDATE features SET hidden=1,locked=1 WHERE id=?',[$id]);
  }elseif(in_array($action,['forum_hide_thread','forum_lock'],true)){
   if(!query($db,'SELECT 1 FROM forum_threads WHERE id=?',[$id])->fetchColumn()){throw new DomainException('Thread not found.',404);}
   query($db,'UPDATE forum_threads SET '.($action==='forum_lock'?'locked':'hidden').'=? WHERE id=?',[(int)$value,$id]);
  }elseif(in_array($action,['forum_hide_post','forum_block','forum_clear_profile','forum_complete_post'],true)){
   $author=query($db,'SELECT author FROM forum_posts WHERE id=?',[$id])->fetchColumn();if(!$author){throw new DomainException('Post not found.',404);}
   if($action==='forum_complete_post'){query($db,'UPDATE forum_posts SET completed=? WHERE id=?',[(int)$value,$id]);}
   elseif($action==='forum_clear_profile'){query($db,'DELETE FROM forum_profiles WHERE user=?',[$author]);}
   elseif($action==='forum_block'){query($db,'UPDATE users SET blocked=? WHERE id=?',[(int)$value,$author]);}
   else{query($db,'UPDATE forum_posts SET hidden=? WHERE id=?',[(int)$value,$id]);}
  }else{throw new DomainException('Invalid forum moderation action.',400);}
  query($db,'INSERT INTO moderation_log(created,action,target,detail) VALUES(?,?,?,?)',[time(),$action,(string)$id,$value?'1':'0']);$db->exec('COMMIT');return admin_board($db);
 }catch(Throwable $e){$db->exec('ROLLBACK');throw $e;}
}
