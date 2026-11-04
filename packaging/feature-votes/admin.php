<?php
// SPDX-License-Identifier: GPL-3.0-or-later
declare(strict_types=1);
require __DIR__.'/admin-core.php';
header('Cache-Control: no-store');header('X-Content-Type-Options: nosniff');header('Referrer-Policy: no-referrer');header('X-Frame-Options: DENY');
header("Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'");
ini_set('session.use_strict_mode','1');ini_set('session.use_only_cookies','1');
session_name('LEMON_OWNER');session_set_cookie_params(['lifetime'=>0,'path'=>'/lemon-studio/features/','secure'=>true,'httponly'=>true,'samesite'=>'Strict']);session_start();
if(!isset($_SESSION['csrf'])){$_SESSION['csrf']=bin2hex(random_bytes(32));}
if($_SERVER['REQUEST_METHOD']==='GET') {
    if(isset($_GET['session'])){header('Content-Type: application/json');reply(200,['authenticated'=>admin_authenticated(),'csrf'=>$_SESSION['csrf']]);}
    header('Content-Type: text/html; charset=utf-8');readfile(__DIR__.'/admin.html');exit;
}
header('Content-Type: application/json; charset=utf-8');
try {
    if($_SERVER['REQUEST_METHOD']!=='POST'){reply(405,['error'=>'POST required.']);}
    if(strtolower(explode(';',$_SERVER['CONTENT_TYPE']??'')[0])!=='application/json'){reply(415,['error'=>'JSON required.']);}
    if(isset($_SERVER['HTTP_ORIGIN']) && $_SERVER['HTTP_ORIGIN']!=='https://michaellanemusic.de'){reply(403,['error'=>'Origin not permitted.']);}
    admin_csrf($_SERVER['HTTP_X_LEMON_CSRF']??'');
    $raw=file_get_contents('php://input',false,null,0,16385);if(strlen($raw)>16384){reply(413,['error'=>'Request too large.']);}
    $in=json_decode($raw,true,8,JSON_THROW_ON_ERROR);if(!is_array($in)){reply(400,['error'=>'Invalid request.']);}
    $action=$in['action']??'';
    if($action!=='login' && !admin_authenticated()){reply(401,['error'=>'Sign in to moderate the board.']);}
    $db=database(admin_storage());
    if($action==='login') {
        if(!is_string($in['password']??null)){reply(400,['error'=>'Password required.']);}
        admin_login($db,$in['password'],$_SERVER['REMOTE_ADDR']??'unknown');reply(200,['csrf'=>$_SESSION['csrf']]);
    }
    if($action==='logout'){$_SESSION=[];session_destroy();setcookie('LEMON_OWNER','',['expires'=>time()-3600,'path'=>'/lemon-studio/features/','secure'=>true,'httponly'=>true,'samesite'=>'Strict']);reply(200,['ok'=>true]);}
    if($action==='list'){reply(200,admin_board($db));}
    if($action==='voters'){if(!is_int($in['id']??null)||!is_int($in['offset']??0)){reply(400,['error'=>'Invalid page.']);}reply(200,admin_voters($db,$in['id'],$in['offset']??0));}
    reply(200,admin_change($db,$in));
}catch(DomainException $e){reply($e->getCode(),['error'=>$e->getMessage()]);}
catch(JsonException $e){reply(400,['error'=>'Invalid JSON.']);}
catch(Throwable $e){error_log('Lemon moderation: '.$e->getMessage());reply(503,['error'=>'Dashboard temporarily unavailable. Please retry.']);}
