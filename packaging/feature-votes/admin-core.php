<?php
// SPDX-License-Identifier: GPL-3.0-or-later
// Included by the owner dashboard only. No credentials are distributed in source.
declare(strict_types=1);
define('LEMON_VOTES_TEST',true);
require __DIR__.'/api.php';
function admin_storage(): string {
    if(defined('LEMON_ADMIN_STORAGE')){return LEMON_ADMIN_STORAGE;}
    $root=realpath($_SERVER['DOCUMENT_ROOT']);if(!$root){throw new RuntimeException('storage');}
    return dirname($root).'/lemon-feature-votes-private';
}
function admin_authenticated(): bool {return isset($_SESSION['owner_until']) && $_SESSION['owner_until']>time();}
function admin_csrf(string $value): void {
    if(!isset($_SESSION['csrf']) || !hash_equals($_SESSION['csrf'],$value)){throw new DomainException('Please reload the dashboard and try again.',403);}
}
function admin_login(PDO $db,string $password,string $ip): void {
    if(strlen($password)>256){throw new DomainException('Sign-in failed.',401);}
    $secret=query($db,'SELECT v FROM settings WHERE k=?',['secret'])->fetchColumn();
    $db->exec('BEGIN IMMEDIATE');
    try {limit_request($db,'owner-login:'.hash_hmac('sha256',$ip,$secret),8,900);$db->exec('COMMIT');}
    catch(Throwable $e){$db->exec('ROLLBACK');throw $e;}
    $hash=query($db,'SELECT v FROM settings WHERE k=?',['owner_password'])->fetchColumn();
    if(!$hash || !password_verify($password,$hash)){throw new DomainException('Sign-in failed. Check your owner password.',401);}
    session_regenerate_id(true);
    $_SESSION=['owner_until'=>time()+7200,'csrf'=>bin2hex(random_bytes(32))];
}
function admin_board(PDO $db): array {
    $items=query($db,'SELECT f.*,COALESCE(SUM(v.value=1 AND u.blocked=0),0) AS up,COALESCE(SUM(v.value=-1 AND u.blocked=0),0) AS down,COALESCE(SUM(u.blocked=1),0) AS excluded,COALESCE(a.blocked,0) AS author_blocked FROM features f LEFT JOIN votes v ON v.feature=f.id LEFT JOIN users u ON u.id=v.user LEFT JOIN users a ON a.id=f.author GROUP BY f.id ORDER BY f.id DESC')->fetchAll(PDO::FETCH_ASSOC);
    $log=query($db,'SELECT created,action,target,detail FROM moderation_log ORDER BY id DESC LIMIT 60')->fetchAll(PDO::FETCH_ASSOC);
    $comments=public_comments($db);foreach($items as &$r){$r['comments']=$comments[(int)$r['id']]??[];}
    return ['items'=>$items,'log'=>$log,'announcements'=>announcements($db,false)];
}
function admin_change(PDO $db,array $in): array {
    $action=$in['action']??'';$id=$in['id']??null;
    if(in_array($action,['announcement_save','announcement_delete'],true)){return admin_announcement($db,$in);}
    if(!is_int($id)||$id<1){throw new DomainException('Select a suggestion.',400);}
    $db->exec('BEGIN IMMEDIATE');
    try {
        $feature=query($db,'SELECT * FROM features WHERE id=?',[$id])->fetch(PDO::FETCH_ASSOC);
        if(!$feature){throw new DomainException('Suggestion not found.',404);}
        $detail='';
        if($action==='visibility' || $action==='lock') {
            $value=$in['value']??null;if(!is_bool($value)){throw new DomainException('Invalid setting.',400);}
            $column=$action==='visibility'?'hidden':'locked';
            query($db,'UPDATE features SET '.$column.'=? WHERE id=?',[(int)$value,$id]);$detail=$column.'='.($value?'1':'0');
        } elseif($action==='status') {
            $status=$in['value']??'';if(!in_array($status,['Open','Planned','In progress','Completed','Declined'],true)){throw new DomainException('Invalid status.',400);}
            query($db,'UPDATE features SET status=? WHERE id=?',[$status,$id]);$detail=$status;
        } elseif($action==='comment_add' || $action==='comment_edit' || $action==='comment_delete') {
            $comment=$in['comment']??null;
            if($action!=='comment_add' && (!is_int($comment) || !query($db,'SELECT 1 FROM public_comments WHERE id=? AND feature=?',[$comment,$id])->fetchColumn())){throw new DomainException('Comment not found on this suggestion.',404);}
            if($action==='comment_delete') {
                query($db,'DELETE FROM public_comments WHERE id=? AND feature=?',[$comment,$id]);$detail='Removed public comment #'.$comment;
            } else {
                $body=$in['body']??'';
                if(!is_string($body)||!preg_match('//u',$body)||strlen($body)>2400){throw new DomainException('Use 1-600 characters for a public comment.',400);}
                $body=trim(str_replace(["\r\n","\r"],"\n",$body));
                if(preg_match_all('/./us',$body)<1||preg_match_all('/./us',$body)>600){throw new DomainException('Use 1-600 characters for a public comment.',400);}
                if(preg_match('/\p{Cc}/u',str_replace(["\n","\t"],'',$body))){throw new DomainException('Remove control characters from the comment.',400);}
                if($action==='comment_add') {
                    if((int)query($db,'SELECT COUNT(*) FROM public_comments WHERE feature=?',[$id])->fetchColumn()>=10){throw new DomainException('This suggestion has 10 comments. Edit or remove an existing comment first.',409);}
                    query($db,'INSERT INTO public_comments(feature,body,created,edited) VALUES(?,?,?,?)',[$id,$body,time(),time()]);$comment=(int)$db->lastInsertId();
                } else {query($db,'UPDATE public_comments SET body=?,edited=? WHERE id=? AND feature=?',[$body,time(),$comment,$id]);}
                $detail=($action==='comment_add'?'Posted':'Edited').' public comment #'.$comment;
            }
        } elseif($action==='block') {
            $user=$in['user']??'';$value=$in['value']??null;
            if(!is_string($user)||!preg_match('/^[a-f0-9]{64}$/D',$user)||!is_bool($value)){throw new DomainException('Invalid identity.',400);}
            if($user!==$feature['author'] && !query($db,'SELECT 1 FROM votes WHERE user=? AND feature=?',[$user,$id])->fetchColumn()){throw new DomainException('Identity not found on this suggestion.',404);}
            query($db,'UPDATE users SET blocked=? WHERE id=?',[(int)$value,$user]);$detail=($value?'Blocked ':'Restored ').$user;
        } else {throw new DomainException('Invalid moderation action.',400);}
        query($db,'INSERT INTO moderation_log(created,action,target,detail) VALUES(?,?,?,?)',[time(),$action,(string)$id,$detail]);
        $db->exec('COMMIT');return admin_board($db);
    }catch(Throwable $e){$db->exec('ROLLBACK');throw $e;}
}
function admin_voters(PDO $db,int $id,int $offset): array {
    if($id<1||$offset<0||$offset>1000000){throw new DomainException('Invalid page.',400);}
    return ['voters'=>query($db,'SELECT v.user,v.value,u.created,u.blocked,(SELECT COUNT(*) FROM votes WHERE user=v.user) AS total_votes FROM votes v JOIN users u ON u.id=v.user WHERE v.feature=? ORDER BY u.created DESC,v.user LIMIT 100 OFFSET '.(int)$offset,[$id])->fetchAll(PDO::FETCH_ASSOC),'total'=>(int)query($db,'SELECT COUNT(*) FROM votes WHERE feature=?',[$id])->fetchColumn()];
}

function admin_announcement(PDO $db,array $in): array {
    $id=$in['id']??0;$action=$in['action'];
    if(!is_int($id)||$id<0){throw new DomainException('Invalid announcement.',400);}
    $db->exec('BEGIN IMMEDIATE');
    try {
        if($id && !query($db,'SELECT 1 FROM announcements WHERE id=?',[$id])->fetchColumn()){throw new DomainException('Announcement not found.',404);}
        if($action==='announcement_delete') {
            if(!$id){throw new DomainException('Select an announcement.',400);}
            query($db,'DELETE FROM announcements WHERE id=?',[$id]);$detail='Removed announcement';
        } else {
            $title=$in['title']??'';$body=$in['body']??'';$published=$in['published']??null;
            if(!is_bool($published)){throw new DomainException('Choose draft or published.',400);}
            foreach(['title'=>120,'body'=>2000] as $key=>$max){
                $value=$$key;
                if(!is_string($value)||strlen($value)>$max*4||!preg_match('//u',$value)){throw new DomainException('Invalid announcement text.',400);}
                $value=trim(str_replace(["\r\n","\r"],"\n",$value));
                if(preg_match_all('/./us',$value)<1||preg_match_all('/./us',$value)>$max||preg_match('/\p{Cc}/u',$key==='body'?str_replace(["\n","\t"],'',$value):$value)){throw new DomainException('Use 1-'.$max.' characters for '.$key.'.',400);}
                $$key=$value;
            }
            if(!$id){
                if((int)query($db,'SELECT COUNT(*) FROM announcements')->fetchColumn()>=50){throw new DomainException('Remove an old announcement first (maximum 50).',409);}
                query($db,'INSERT INTO announcements(title,body,published,created,edited) VALUES(?,?,?,?,?)',[$title,$body,(int)$published,time(),time()]);$id=(int)$db->lastInsertId();
            } else {query($db,'UPDATE announcements SET title=?,body=?,published=?,edited=? WHERE id=?',[$title,$body,(int)$published,time(),$id]);}
            $detail=$published?'Published announcement':'Saved announcement as draft';
        }
        query($db,'INSERT INTO moderation_log(created,action,target,detail) VALUES(?,?,?,?)',[time(),$action,(string)$id,$detail]);
        $db->exec('COMMIT');return admin_board($db);
    }catch(Throwable $e){$db->exec('ROLLBACK');throw $e;}
}
