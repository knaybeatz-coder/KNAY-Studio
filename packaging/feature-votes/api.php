<?php
// SPDX-License-Identifier: GPL-3.0-or-later
// Anonymous voting: server-issued identities, atomic unique votes, bounded requests.
declare(strict_types=1);
function reply(int $status, array $data): never {http_response_code($status);echo json_encode($data, JSON_UNESCAPED_UNICODE|JSON_THROW_ON_ERROR);exit;}
function query(PDO $db,string $sql,array $args=[]): PDOStatement {$s=$db->prepare($sql);$s->execute($args);return $s;}
function database(string $directory): PDO {
    if (!is_dir($directory) && !mkdir($directory,0700,true)) {throw new RuntimeException('storage');}
    $db=new PDO('sqlite:'.$directory.'/votes.sqlite',null,null,[PDO::ATTR_ERRMODE=>PDO::ERRMODE_EXCEPTION]);
    $db->exec('PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON');
    $db->exec('CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY, created INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS features(id INTEGER PRIMARY KEY, title TEXT NOT NULL, normalized TEXT UNIQUE NOT NULL, created INTEGER NOT NULL, author TEXT, hidden INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS votes(user TEXT NOT NULL REFERENCES users(id), feature INTEGER NOT NULL REFERENCES features(id), value INTEGER NOT NULL CHECK(value IN(-1,1)), PRIMARY KEY(user,feature));
        CREATE TABLE IF NOT EXISTS limits(k TEXT PRIMARY KEY, n INTEGER NOT NULL, expires INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY, v TEXT NOT NULL);');
    // Additive migration preserves all existing identities, suggestions and votes.
    foreach (['features'=>['status'=>"TEXT NOT NULL DEFAULT 'Open'",'locked'=>'INTEGER NOT NULL DEFAULT 0'],'users'=>['blocked'=>'INTEGER NOT NULL DEFAULT 0']] as $table=>$columns) {
        $existing=array_column(query($db,'PRAGMA table_info('.$table.')')->fetchAll(PDO::FETCH_ASSOC),'name');
        foreach($columns as $name=>$type){if(!in_array($name,$existing,true)){$db->exec('ALTER TABLE '.$table.' ADD COLUMN '.$name.' '.$type);}}
    }
    $db->exec('CREATE TABLE IF NOT EXISTS moderation_log(id INTEGER PRIMARY KEY, created INTEGER NOT NULL, action TEXT NOT NULL, target TEXT NOT NULL, detail TEXT NOT NULL)');
    $db->exec('CREATE TABLE IF NOT EXISTS public_comments(id INTEGER PRIMARY KEY, feature INTEGER NOT NULL REFERENCES features(id), body TEXT NOT NULL, created INTEGER NOT NULL, edited INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS comments_feature ON public_comments(feature,id)');
    $db->exec('CREATE TABLE IF NOT EXISTS announcements(id INTEGER PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL, published INTEGER NOT NULL DEFAULT 0, created INTEGER NOT NULL, edited INTEGER NOT NULL)');
    query($db,'INSERT OR IGNORE INTO settings VALUES(?,?)',['secret',bin2hex(random_bytes(32))]);
    foreach (['Track freeze and unfreeze to save CPU','ARA integration for compatible audio editors','Live collaboration on projects'] as $i=>$title) {
        query($db,'INSERT OR IGNORE INTO features(id,title,normalized,created) VALUES(?,?,?,?)',[$i+1,$title,strtolower($title),time()]);
    }
    return $db;
}
function limit_request(PDO $db,string $key,int $max,int $seconds): void {
    $now=time();$row=query($db,'SELECT n,expires FROM limits WHERE k=?',[$key])->fetch(PDO::FETCH_ASSOC);
    if($row && $row['expires']>$now && $row['n'] >= $max) {throw new DomainException('Please wait before trying again.',429);}
    if(!$row || $row['expires']<=$now){query($db,'INSERT OR REPLACE INTO limits VALUES(?,1,?)',[$key,$now+$seconds]);}
    else{query($db,'UPDATE limits SET n=n+1 WHERE k=?',[$key]);}
}
function public_comments(PDO $db): array {
    $out=[];
    foreach(query($db,'SELECT id,feature,body,created,edited FROM public_comments ORDER BY id')->fetchAll(PDO::FETCH_ASSOC) as $r){
        $feature=(int)$r['feature'];unset($r['feature']);
        foreach(['id','created','edited'] as $k){$r[$k]=(int)$r[$k];}
        $out[$feature][]=$r;
    }
    return $out;
}
function announcements(PDO $db,bool $public): array {
    $rows=query($db,'SELECT id,title,body,published,created,edited FROM announcements'.($public?' WHERE published=1':'').' ORDER BY id DESC LIMIT 50')->fetchAll(PDO::FETCH_ASSOC);
    foreach($rows as &$row){foreach(['id','published','created','edited'] as $key){$row[$key]=(int)$row[$key];}}
    return $rows;
}
function board(PDO $db,string $user): array {
    $sql='SELECT f.id,f.title,f.status,f.locked,COALESCE(SUM(v.value=1),0) AS up,COALESCE(SUM(v.value=-1),0) AS down,COALESCE((SELECT value FROM votes WHERE user=? AND feature=f.id),0) AS mine FROM features f LEFT JOIN votes v ON v.feature=f.id AND v.user IN (SELECT id FROM users WHERE blocked=0) WHERE f.hidden=0 GROUP BY f.id ORDER BY (up-down) DESC,f.id DESC LIMIT 500';
    $items=query($db,$sql,[$user])->fetchAll(PDO::FETCH_ASSOC);
    foreach($items as &$r){foreach(['id','up','down','mine'] as $k){$r[$k]=(int)$r[$k];}}
    unset($r);
    $comments=public_comments($db);
    foreach($items as &$r){$r['comments']=$comments[(int)$r['id']]??[];}
    return ['items'=>$items,'announcements'=>announcements($db,true)];
}
function handle(PDO $db,array $in,string $ip,string $token): array {
    $secret=query($db,'SELECT v FROM settings WHERE k=?',['secret'])->fetchColumn();
    $packed=inet_pton($ip);$network=$packed!==false&&strlen($packed)===16?bin2hex(substr($packed,0,8)):$ip;
    $peer=hash_hmac('sha256',$network,$secret);$action=$in['action']??'list';
    $user=preg_match('/^[a-f0-9]{64}$/D',$token)?hash('sha256',$token):'';
    if($user && !query($db,'SELECT 1 FROM users WHERE id=?',[$user])->fetchColumn()){throw new DomainException('Voter identity unavailable. Contact support rather than resetting votes.',401);}
    // Serialize limit checks and mutation together; uniqueness holds across concurrent requests.
    $db->exec('BEGIN IMMEDIATE');
    try {
        limit_request($db,'requests:'.$peer,120,60);
        $db->exec('COMMIT');$db->exec('BEGIN IMMEDIATE');
        if($user && in_array($action,['vote','suggest'],true) && query($db,'SELECT blocked FROM users WHERE id=?',[$user])->fetchColumn()){throw new DomainException('This voter identity has been blocked by the moderator.',403);}
        if($action==='challenge') {
            limit_request($db,'challenge:'.$peer,12,3600);
            $challenge=(string)(time()+300).'.'.bin2hex(random_bytes(12));
            $result=['challenge'=>$challenge.'.'.hash_hmac('sha256',$challenge.'.'.$peer,$secret)];
        } elseif($action==='register') {
            $c=$in['challenge']??'';$nonce=$in['nonce']??'';
            if(!is_string($c)||!is_string($nonce)||strlen($c)>140||strlen($nonce)>20){throw new DomainException('Invalid request.',400);}
            $parts=explode('.',$c);$signed=implode('.',array_slice($parts,0,2));
            if(count($parts)!==3 || !ctype_digit($parts[0]) || (int)$parts[0]<time() || (int)$parts[0]>time()+300 || !hash_equals(hash_hmac('sha256',$signed.'.'.$peer,$secret),$parts[2]) || !str_starts_with(hash('sha256',$c.':'.$nonce),'0000')){throw new DomainException('Please retry the connection.',400);}
            limit_request($db,'register:'.$peer,3,86400);
            limit_request($db,'used:'.hash('sha256',$c),1,600);
            $token=bin2hex(random_bytes(32));query($db,'INSERT INTO users(id,created) VALUES(?,?)',[hash('sha256',$token),time()]);$result=['token'=>$token];
        } elseif($action==='list') {$result=board($db,$user);}
        elseif($action==='vote') {
            if(!$user){throw new DomainException('Connect before voting.',401);}
            $id=$in['id']??null;$value=$in['value']??null;
            if(!is_int($id)||!in_array($value,[-1,1],true)){throw new DomainException('Invalid vote.',400);}
            if(!query($db,'SELECT 1 FROM features WHERE id=? AND hidden=0',[$id])->fetchColumn()){throw new DomainException('Suggestion no longer available.',404);}
            if(query($db,'SELECT locked FROM features WHERE id=?',[$id])->fetchColumn()){throw new DomainException('Voting on this suggestion is closed.',403);}
            // Idempotent repeat clicks. A changed direction replaces the existing vote, never adds another.
            $old=query($db,'SELECT value FROM votes WHERE user=? AND feature=?',[$user,$id])->fetchColumn();
            if($old===false || (int)$old!==$value){
                limit_request($db,'vote:'.$user,40,60);
                if($old===false){limit_request($db,'feature:'.$peer.':'.$id,3,86400);}
                query($db,'INSERT INTO votes VALUES(?,?,?) ON CONFLICT(user,feature) DO UPDATE SET value=excluded.value',[$user,$id,$value]);
            }
            $result=board($db,$user);
        } elseif($action==='suggest') {
            if(!$user){throw new DomainException('Connect before suggesting.',401);}
            $title=$in['title']??'';
            if(!is_string($title)||!preg_match('//u',$title)||strlen($title)>480){throw new DomainException('Use 8–120 characters.',400);}
            $title=preg_replace('/\s+/u',' ',trim($title));$n=preg_match_all('/./us',$title);
            if($n<8||$n>120||preg_match('/[\p{C}<>]|https?:|www\./iu',$title)){throw new DomainException('Use 8–120 characters, without links or markup.',400);}
            $norm=function_exists('mb_strtolower')?mb_strtolower($title,'UTF-8'):strtolower($title);
            if(query($db,'SELECT 1 FROM features WHERE normalized=?',[$norm])->fetchColumn()){throw new DomainException('That suggestion is already on the board.',409);}
            limit_request($db,'suggest-user:'.$user,3,86400);limit_request($db,'suggest-ip:'.$peer,6,86400);
            if(query($db,'SELECT COUNT(*) FROM features')->fetchColumn()>=500){throw new DomainException('The board is full for now.',429);}
            query($db,'INSERT INTO features(title,normalized,created,author) VALUES(?,?,?,?)',[$title,$norm,time(),$user]);$result=board($db,$user);
        } else {throw new DomainException('Invalid request.',400);}
        query($db,'DELETE FROM limits WHERE expires<?',[time()-86400]);$db->exec('COMMIT');return $result;
    } catch(Throwable $e){$db->exec('ROLLBACK');throw $e;}
}
if(defined('LEMON_VOTES_TEST')){return;}
header('Content-Type: application/json; charset=utf-8');header('Cache-Control: no-store');header('X-Content-Type-Options: nosniff');header("Content-Security-Policy: default-src 'none'");
try {
    if($_SERVER['REQUEST_METHOD']!=='POST'){reply(405,['error'=>'Use the Lemon Studio feature voting tab.']);}
    if(strtolower(explode(';',$_SERVER['CONTENT_TYPE']??'')[0])!=='application/json'){reply(415,['error'=>'JSON required.']);}
    // No cookie authentication or permissive CORS. Cross-site browser requests cannot send authorized JSON.
    if(isset($_SERVER['HTTP_ORIGIN']) && $_SERVER['HTTP_ORIGIN']!=='https://michaellanemusic.de'){reply(403,['error'=>'Origin not permitted.']);}
    $raw=file_get_contents('php://input',false,null,0,4097);if(strlen($raw)>4096){reply(413,['error'=>'Request too large.']);}
    $in=json_decode($raw,true,8,JSON_THROW_ON_ERROR);if(!is_array($in)){reply(400,['error'=>'Invalid request.']);}
    $root=realpath($_SERVER['DOCUMENT_ROOT']);if(!$root){throw new RuntimeException('storage');}
    $directory=dirname($root).'/lemon-feature-votes-private';
    if(str_starts_with($directory.'/',rtrim($root,'/').'/')){throw new RuntimeException('storage');}
    $db=database($directory);
    $auth=$_SERVER['HTTP_X_LEMON_VOTER']??'';
    reply(200,handle($db,$in,$_SERVER['REMOTE_ADDR']??'unknown',$auth));
} catch(DomainException $e){reply($e->getCode(),['error'=>$e->getMessage()]);}
catch(JsonException $e){reply(400,['error'=>'Invalid JSON.']);}
catch(Throwable $e){error_log('Lemon feature board: '.$e->getMessage());reply(503,['error'=>'Feature board temporarily unavailable. Please try later.']);}
