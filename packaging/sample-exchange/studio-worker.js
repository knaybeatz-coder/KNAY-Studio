import taxonomyText from './taxonomy.json';
import {waveform} from './waveform.mjs';
const taxonomy=JSON.parse(taxonomyText);
const hex=a=>Array.from(new Uint8Array(a),v=>v.toString(16).padStart(2,'0')).join('');
const enc=new TextEncoder(),hash=async v=>hex(await crypto.subtle.digest('SHA-256',typeof v==='string'?enc.encode(v):v));
const j=(v,status=200)=>Response.json(v,{status,headers:{'Cache-Control':'no-store'}});
async function derive(pass,salt){let k=await crypto.subtle.importKey('raw',enc.encode(pass),'PBKDF2',false,['deriveBits']);return hex(await crypto.subtle.deriveBits({name:'PBKDF2',salt:enc.encode(salt),iterations:100000,hash:'SHA-256'},k,256))}
const desktopAgent="LemonStudio-Community/1";
async function rate(e,r,scope,limit,period){
 const bucket=Math.floor(Date.now()/period), key=await hash([scope,r.headers.get('CF-Connecting-IP')||'local',bucket].join('|'));
 const count=await e.DB.prepare("INSERT INTO exchange_limits(k,n,expires) VALUES(?,1,?) ON CONFLICT(k) DO UPDATE SET n=n+1 RETURNING n").bind(key,(bucket+2)*period).first();
 if(count.n===1)await e.DB.prepare("DELETE FROM exchange_limits WHERE expires<?").bind(Date.now()).run();
 return count.n<=limit;
}
async function ticketKey(e){return crypto.subtle.importKey('raw',enc.encode(e.DEVICE),{name:'HMAC',hash:'SHA-256'},false,['sign','verify'])}
async function desktopSession(r,e){
 if(r.method!=='POST'||r.headers.has('Origin')||r.headers.get('User-Agent')!==desktopAgent)return j('Open Community Samples in Lemon Studio',403);
 if(!await rate(e,r,'session',180,3600000))return j('Too many connections. Try again later.',429);
 const payload=crypto.randomUUID()+'.'+(Date.now()+86400000);
 const signature=hex(await crypto.subtle.sign('HMAC',await ticketKey(e),enc.encode('desktop:'+payload)));
 return j({ticket:payload+'.'+signature});
}
async function validTicket(r,e){
 if(r.headers.has('Origin')||r.headers.get('User-Agent')!==desktopAgent)return false;
 const token=r.headers.get('X-Lemon-Desktop')||'', match=/^([a-f0-9-]{36}\.(\d{13}))\.([a-f0-9]{64})$/.exec(token);
 if(!match||Number(match[2])<Date.now())return false;
 return crypto.subtle.verify('HMAC',await ticketKey(e),Uint8Array.from(match[3].match(/../g),b=>parseInt(b,16)),enc.encode('desktop:'+match[1]));
}
export default {async fetch(r,e){
let p=new URL(r.url).pathname,t=r.headers.get('X-Studio-Time'),sig=r.headers.get('X-Studio-Signature')||'';
if(p==='/desktop/session')return desktopSession(r,e);
const desktop=await validTicket(r,e);
if(!desktop && (!t||Math.abs(Date.now()/1000-Number(t))>45||!Number.isFinite(Number(t))||!/^[a-f0-9]{64}$/.test(sig)))return j('Lemon Studio session required',403);
if(Number(r.headers.get('Content-Length')||0)>7200000)return j('File too large',413);
let body=await r.arrayBuffer();if(body.byteLength>7200000)return j('File too large',413);
let k=await crypto.subtle.importKey('raw',enc.encode(e.DEVICE),{name:'HMAC',hash:'SHA-256'},false,['verify']);
if(!desktop && !await crypto.subtle.verify('HMAC',k,Uint8Array.from(sig.match(/../g),s=>parseInt(s,16)),enc.encode([r.method,p,t,await hash(body)].join('\n'))))return j('Invalid device',403);
let data={};try{if(body.byteLength)data=JSON.parse(new TextDecoder().decode(body))}catch{return j('Invalid data',400)}
if(p==='/catalog'){let {results}=await e.DB.prepare('SELECT s.id,title,creator,category,time,d.subtype,d.genre,d.peaks,(SELECT count(*) FROM account_votes v WHERE v.sample_id=s.id) votes FROM samples s LEFT JOIN sample_details d ON d.sample_id=s.id ORDER BY time DESC LIMIT 500').all();return j(results.map(row=>({...row,subtype:row.subtype||"Unclassified",genre:row.genre||"Unclassified",peaks:JSON.parse(row.peaks||"[]")})))}
if(p.startsWith('/wav/')){let a=await e.FILES.get(p.slice(5),'arrayBuffer');return a?new Response(a,{headers:{'Content-Type':'audio/wav','Cache-Control':'private, no-store'}}):j('Missing',404)}
if(p==='/register'||p==='/login'){
if(!await rate(e,r,p,p==='/register'?10:90,3600000))return j('Too many attempts. Try again later.',429);
let email=String(data.email||'').trim().toLowerCase(),pass=String(data.password||'');if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)||email.length>254)return j('Enter a valid email address',400);
if(pass.length<12||pass.length>200)return j('Use a password of 12–200 characters',400);
if(p==='/register'){let invite=await hash(String(data.invite||'')),id=crypto.randomUUID(),salt=crypto.randomUUID(),pw=await derive(pass,salt);
if(!desktop && !await e.DB.prepare('SELECT 1 FROM exchange_invites WHERE digest=? AND email=?').bind(invite,email).first())return j('Invalid invitation',403);
const registered=await e.DB.batch([e.DB.prepare('INSERT OR IGNORE INTO exchange_logins(id,email,salt,pw) VALUES(?,?,?,?)').bind(id,email,salt,pw),
e.DB.prepare('INSERT OR IGNORE INTO members(id,email) SELECT id,email FROM exchange_logins WHERE id=?').bind(id),
e.DB.prepare('DELETE FROM exchange_invites WHERE email=? AND EXISTS(SELECT 1 FROM exchange_logins WHERE id=?)').bind(email,id)]);
if(!registered[0].meta.changes)return j('Unable to register. Try signing in.',409);}

let u=await e.DB.prepare('SELECT * FROM exchange_logins WHERE email=?').bind(email).first();if(!u||await derive(pass,u.salt)!==u.pw)return j('Invalid sign-in',403);
let token=crypto.randomUUID()+crypto.randomUUID();await e.DB.prepare('INSERT INTO exchange_sessions VALUES(?,?,?)').bind(await hash(token),u.id,Date.now()+3600000).run();return j({token})}
let u=await e.DB.prepare('SELECT m.* FROM exchange_sessions s JOIN members m ON m.id=s.member WHERE s.digest=? AND s.expires>?').bind(await hash(r.headers.get('X-Member')||''),Date.now()).first();if(!u)return j('Sign in to contribute',401);
if(p==='/me')return j({newsletter:!!u.newsletter,email:u.email});
if(p==='/logout'){await e.DB.prepare('DELETE FROM exchange_sessions WHERE digest=?').bind(await hash(r.headers.get('X-Member')||'')).run();return j('Signed out')}
if(p==='/newsletter'&&typeof data.subscribe==='boolean'){await e.DB.batch([e.DB.prepare('UPDATE members SET newsletter=? WHERE id=?').bind(+data.subscribe,u.id),e.DB.prepare('INSERT INTO newsletter_events VALUES(?,?,?,?,?)').bind(u.id,u.email,+data.subscribe,Date.now(),'Lemon Studio news and updates v1')]);return j('Preference saved')}
if(p==='/vote'){await e.DB.prepare('INSERT OR IGNORE INTO account_votes SELECT id,? FROM samples WHERE id=?').bind(u.id,String(data.id)).run();return j('Vote recorded')}
if(p==='/upload'){if(!await rate(e,r,'upload',50,3600000))return j('Upload limit reached. Try again later.',429);let {title,creator,category,wav,agree}=data;if(agree!==true||typeof title!=='string'||!title.trim()||title.length>70||typeof creator!=='string'||!creator.trim()||creator.length>60||!Object.hasOwn(taxonomy.families,category)||!taxonomy.families[category].includes(data.subtype)||!taxonomy.genres.includes(data.genre)||typeof wav!=='string'||wav.length>6990510)return j('Check details, file and terms',400);
let s=atob(wav);if(s.length<44||s.length>5242880||s.slice(0,4)!=='RIFF'||s.slice(8,12)!=='WAVE')return j('Invalid WAV',400);
let bytes=Uint8Array.from(s,c=>c.charCodeAt(0)),peaks;try{peaks=waveform(bytes)}catch(err){return j(err.message,400)}
let id=crypto.randomUUID(),now=Date.now();await e.FILES.put(id,bytes);try{await e.DB.batch([e.DB.prepare('INSERT INTO samples VALUES(?,?,?,?,?,?,?)').bind(id,title.trim(),creator.trim(),category,now,'royalty-free-v1',now),e.DB.prepare('INSERT INTO sample_details(sample_id,subtype,genre,peaks) VALUES(?,?,?,?)').bind(id,data.subtype,data.genre,JSON.stringify(peaks)),e.DB.prepare('INSERT INTO upload_receipts VALUES(?,?,?,?)').bind(id,u.id,now,'royalty-free-v1')])}catch(err){await e.FILES.delete(id);throw err}return j({id})}
return j('Unknown request',404)}}

