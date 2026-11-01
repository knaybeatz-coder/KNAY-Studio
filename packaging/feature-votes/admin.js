'use strict';
const $=id=>document.getElementById(id);let csrf='',items=[],busy=false,reviewId=0,offset=0,total=0;
const endpoint=new URL(location.href);endpoint.search='';endpoint.hash='';
function notice(text){$('notice').textContent=text;}
async function request(body){const r=await fetch(endpoint,{method:'POST',headers:{'Content-Type':'application/json','X-Lemon-CSRF':csrf},body:JSON.stringify(body),credentials:'same-origin',cache:'no-store'});const d=await r.json();if(!r.ok){if(r.status===401){$('workspace').hidden=true;$('login').hidden=false;$('logout').hidden=true;if($('voters').open)$('voters').close();}throw Error(d.error||'Request failed.');}return d;}
async function run(fn){if(busy)return;busy=true;document.querySelectorAll('button').forEach(b=>b.disabled=true);try{await fn();}catch(e){notice(e.message||'Connection unavailable. Please retry.');}finally{busy=false;document.querySelectorAll('button').forEach(b=>b.disabled=false);$('previous').disabled=offset===0;$('next').disabled=offset+100>=total;}}
function el(tag,text,cls){const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;}
function button(text,fn,cls){const n=el('button',text,cls);n.type='button';n.onclick=()=>run(fn);return n;}
function update(data){items=data.items;render();renderAnnouncements(data.announcements||[]);$('activity').replaceChildren();for(const row of data.log){const n=el('div',undefined,'log-row');n.append(el('time',new Date(Number(row.created)*1000).toLocaleString()),el('span',`${row.action} · #${row.target} · ${row.detail}`));$('activity').append(n);}}
async function load(){update(await request({action:'list'}));$('login').hidden=true;$('workspace').hidden=false;$('logout').hidden=false;}
async function change(body,question){if(question&&!confirm(question))return;update(await request(body));notice('Saved. Your change is live.');}
function commentsFor(i){
 const id=Number(i.id),section=el('details',undefined,'public-comments'),comments=i.comments||[];
 section.append(el('summary',`Public comments (${comments.length})`),el('p','Shown to everyone as Michael · Lemon Studio. Up to 600 characters per comment.','caption'));
 for(const c of comments){
  const row=el('div',undefined,'comment-editor'),label=el('label',`Michael · ${new Date(Number(c.created)*1000).toLocaleString()}`),input=el('textarea');
  input.value=c.body;input.maxLength=600;input.rows=3;input.setAttribute('aria-label',`Edit public comment ${c.id}`);
  const actions=el('div',undefined,'actions');
  actions.append(button('Save comment',()=>change({action:'comment_edit',id,comment:Number(c.id),body:input.value})),button('Remove comment',()=>change({action:'comment_delete',id,comment:Number(c.id)},'Remove this public comment?'),'danger'));
  row.append(label,input,actions);section.append(row);
 }
 if(comments.length<10){const input=el('textarea');input.maxLength=600;input.rows=3;input.placeholder='Write a public update for this suggestion…';input.setAttribute('aria-label',`New public comment for idea ${id}`);section.append(input,button('Post public comment',()=>change({action:'comment_add',id,body:input.value}),'primary'));}
 return section;
}
function render(){
 $('count').textContent=items.length;$('votes').textContent=items.reduce((n,i)=>n+Number(i.up)+Number(i.down),0);$('planned').textContent=items.filter(i=>['Planned','In progress'].includes(i.status)).length;$('hidden').textContent=items.filter(i=>Number(i.hidden)).length;
 const q=$('search').value.trim().toLowerCase(),filter=$('filter').value;
 const list=items.filter(i=>(i.title.toLowerCase().includes(q)||('#'+i.id).includes(q))&&(filter==='all'||(filter==='visible'&&!Number(i.hidden))||(filter==='hidden'&&Number(i.hidden))||(filter==='locked'&&Number(i.locked))||i.status===filter));
 list.sort((a,b)=>$('sort').value==='new'?Number(b.id)-Number(a.id):(Number(b.up)-Number(b.down))-(Number(a.up)-Number(a.down))||Number(b.id)-Number(a.id));
 $('ideas').replaceChildren();if(!list.length)$('ideas').append(el('div','No suggestions match this view.','empty'));
 for(const i of list){const id=Number(i.id),hidden=Boolean(Number(i.hidden)),locked=Boolean(Number(i.locked));const card=el('article',undefined,'card'+(hidden?' is-hidden':'')),score=el('div',String(Number(i.up)-Number(i.down)),'score');score.append(el('small',`↑ ${i.up}  ↓ ${i.down}`));const body=el('div'),meta=el('div',undefined,'meta');meta.append(el('span',`IDEA #${id}`),el('span',new Date(Number(i.created)*1000).toLocaleDateString()),el('span',hidden?'Hidden':'Public','badge'));if(locked)meta.append(el('span','Voting closed','badge'));if(Number(i.excluded))meta.append(el('span',`${i.excluded} blocked votes excluded`));body.append(meta,el('h3',i.title));const actions=el('div',undefined,'actions'),status=el('select');status.setAttribute('aria-label',`Status for idea ${id}`);for(const value of ['Open','Planned','In progress','Completed','Declined']){const o=el('option',value);o.value=value;o.selected=value===i.status;status.append(o);}status.onchange=()=>run(async()=>{try{await change({action:'status',id,value:status.value});}catch(e){status.value=i.status;throw e;}});actions.append(status,button(hidden?'Restore to board':'Hide suggestion',()=>change({action:'visibility',id,value:!hidden},hidden?null:'Hide this suggestion from the public board? It can be restored.')),button(locked?'Reopen voting':'Close voting',()=>change({action:'lock',id,value:!locked})),button('Review voters',async()=>{reviewId=id;offset=0;await voters();$('voters').showModal();}));if(i.author){const blocked=Boolean(Number(i.author_blocked));actions.append(button(blocked?'Unblock author':'Block author',()=>change({action:'block',id,user:i.author,value:!blocked},blocked?'Restore this author’s access and votes?':'Block this anonymous author across the board? Their votes will be excluded; the suggestion stays visible until you hide it.'),'danger'));}body.append(actions,commentsFor(i));card.append(score,body);$('ideas').append(card);}
}
async function voters(){const d=await request({action:'voters',id:reviewId,offset});total=d.total;$('voters-title').textContent=`Voters on idea #${reviewId}`;$('voter-list').replaceChildren();if(!d.voters.length)$('voter-list').append(el('p','No votes yet.'));for(const v of d.voters){const n=el('div',undefined,'voter'),text=el('div',`${Number(v.value)===1?'↑ Upvote':'↓ Downvote'} · ${v.user.slice(0,12)}…`);text.title=v.user;text.append(el('small',`${v.total_votes} votes across the board · identity created ${new Date(Number(v.created)*1000).toLocaleDateString()}${Number(v.blocked)?' · Blocked':''}`));n.append(text,button(Number(v.blocked)?'Unblock':'Block identity',async()=>{await change({action:'block',id:reviewId,user:v.user,value:!Number(v.blocked)},Number(v.blocked)?'Restore this identity and all its votes?':'Block this identity and exclude all its votes across the board?');await voters();}));$('voter-list').append(n);}$('page').textContent=total?`${offset+1}–${Math.min(offset+100,total)} of ${total}`:'0 voters';$('previous').disabled=offset===0;$('next').disabled=offset+100>=total;}
$('login-form').onsubmit=e=>{e.preventDefault();run(async()=>{const d=await request({action:'login',password:$('password').value});$('password').value='';csrf=d.csrf;await load();notice('Welcome. You’re signed in securely.');});};
$('logout').onclick=()=>run(async()=>{await request({action:'logout'});location.reload();});$('refresh').onclick=()=>run(async()=>{await load();notice('Board refreshed.');});$('search').oninput=render;$('filter').onchange=render;$('sort').onchange=render;$('close-voters').onclick=()=>$('voters').close();$('previous').onclick=()=>run(async()=>{offset=Math.max(0,offset-100);await voters();});$('next').onclick=()=>run(async()=>{offset+=100;await voters();});
run(async()=>{const url=new URL(endpoint);url.searchParams.set('session','1');const r=await fetch(url,{cache:'no-store',credentials:'same-origin'});if(!r.ok)throw Error('Cannot connect to the dashboard.');const d=await r.json();csrf=d.csrf;if(d.authenticated)await load();});

function renderAnnouncements(rows){
 const root=$('announcements');root.replaceChildren();
 for(const item of [{id:0,title:'',body:'',published:0},...rows]){
  const section=el('details',undefined,'public-comments');
  section.append(el('summary',item.id?`${Number(item.published)?'PUBLISHED':'DRAFT'} · ${item.title}`:'+ New announcement'));
  const title=el('input'),body=el('textarea');title.value=item.title;title.maxLength=120;title.placeholder='Announcement title';title.setAttribute('aria-label','Announcement title');
  body.value=item.body;body.maxLength=2000;body.rows=6;body.placeholder='Your update to the community…';body.setAttribute('aria-label','Announcement text');
  const actions=el('div',undefined,'actions');
  const save=published=>change({action:'announcement_save',id:Number(item.id),title:title.value,body:body.value,published});
  actions.append(button(Number(item.published)?'Unpublish / save draft':'Save draft',()=>save(false)),button(Number(item.published)?'Publish changes':'Publish announcement',()=>save(true),'primary'));
  if(item.id)actions.append(button('Delete',()=>change({action:'announcement_delete',id:Number(item.id)},'Delete this announcement?'),'danger'));
  section.append(el('label','Title · up to 120 characters'),title,el('label','Message · up to 2,000 characters'),body,actions);root.append(section);
 }
}
