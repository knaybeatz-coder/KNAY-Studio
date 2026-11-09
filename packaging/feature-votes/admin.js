'use strict';
const $=id=>document.getElementById(id);let csrf='',items=[],busy=false,reviewId=0,offset=0,total=0;
const endpoint=new URL(location.href);endpoint.search='';endpoint.hash='';
function notice(text){$('notice').textContent=text;}
async function request(body){const r=await fetch(endpoint,{method:'POST',headers:{'Content-Type':'application/json','X-Lemon-CSRF':csrf},body:JSON.stringify(body),credentials:'same-origin',cache:'no-store'});const d=await r.json();if(!r.ok){if(r.status===401){$('workspace').hidden=true;$('login').hidden=false;$('logout').hidden=true;if($('voters').open)$('voters').close();}throw Error(d.error||'Request failed.');}return d;}
async function run(fn){if(busy)return;busy=true;document.querySelectorAll('button').forEach(b=>b.disabled=true);try{await fn();}catch(e){notice(e.message||'Connection unavailable. Please retry.');}finally{busy=false;document.querySelectorAll('button').forEach(b=>b.disabled=false);$('previous').disabled=offset===0;$('next').disabled=offset+100>=total;}}
function el(tag,text,cls){const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;}
function button(text,fn,cls){const n=el('button',text,cls);n.type='button';n.onclick=()=>run(fn);return n;}
function update(data){items=data.items;render();renderAnnouncements(data.announcements||[]);$('activity').replaceChildren();for(const row of data.log){const n=el('div',undefined,'log-row');n.append(el('time',new Date(Number(row.created)*1000).toLocaleString()),el('span',`${row.action} · #${row.target} · ${row.detail}`));$('activity').append(n);}}
async function load(){update(await request({action:'list'}));$('login').hidden=true;$('workspace').hidden=false;$('logout').hidden=false;await loadOwnerProfile();await reviewForum();await loadFeedback();}
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
const sectionOpen=new Set(['Active voting']);
function category(i){return Number(i.hidden)?'Hidden':i.status==='Declined'?'Rejected':i.status!=='Open'?i.status:Number(i.locked)?'Voting closed':'Active voting';}
function render(){
 $('count').textContent=items.length;$('votes').textContent=items.reduce((n,i)=>n+Number(i.up)+Number(i.down),0);$('planned').textContent=items.filter(i=>['Planned','In progress'].includes(i.status)).length;$('hidden').textContent=items.filter(i=>Number(i.hidden)).length;
 const q=$('search').value.trim().toLowerCase(),filter=$('filter').value;
 const list=items.filter(i=>(i.title.toLowerCase().includes(q)||('#'+i.id).includes(q))&&(filter==='all'||(filter==='visible'&&!Number(i.hidden))||(filter==='hidden'&&Number(i.hidden))||(filter==='locked'&&Number(i.locked))||i.status===filter));
 list.sort((a,b)=>$('sort').value==='new'?Number(b.id)-Number(a.id):(Number(b.up)-Number(b.down))-(Number(a.up)-Number(a.down))||Number(b.id)-Number(a.id));
 $('ideas').replaceChildren();if(!list.length)$('ideas').append(el('div','No suggestions match this view.','empty'));
 const groups=new Map();
 for(const name of ['Active voting','Planned','In progress','Completed','Rejected','Voting closed','Hidden']){const count=list.filter(i=>category(i)===name).length;if(!count&&name!=='Active voting')continue;const section=el('details',undefined,'idea-section'),summary=el('summary',`${name} (${count})`),body=el('div',undefined,'ideas');section.open=sectionOpen.has(name);section.ontoggle=()=>{if(section.isConnected){if(section.open)sectionOpen.add(name);else sectionOpen.delete(name);}};section.append(summary,body);$('ideas').append(section);groups.set(name,body);}
 for(const i of list){const id=Number(i.id),hidden=Boolean(Number(i.hidden)),locked=Boolean(Number(i.locked));const card=el('article',undefined,'card'+(hidden?' is-hidden':'')),score=el('div',String(Number(i.up)-Number(i.down)),'score');score.append(el('small',`↑ ${i.up}  ↓ ${i.down}`));const body=el('div'),meta=el('div',undefined,'meta');meta.append(el('span',`IDEA #${id}`),el('span',new Date(Number(i.created)*1000).toLocaleDateString()),el('span',hidden?'Hidden':'Public','badge'));if(locked)meta.append(el('span','Voting closed','badge'));if(Number(i.excluded))meta.append(el('span',`${i.excluded} blocked votes excluded`));body.append(meta,el('h3',i.title));const actions=el('div',undefined,'actions'),status=el('select');status.setAttribute('aria-label',`Status for idea ${id}`);for(const value of ['Open','Planned','In progress','Completed','Declined']){const o=el('option',value==='Declined'?'Rejected':value);o.value=value;o.selected=value===i.status;status.append(o);}status.onchange=()=>run(async()=>{try{await change({action:'status',id,value:status.value});}catch(e){status.value=i.status;throw e;}});actions.append(status,button(hidden?'Restore to board':'Hide suggestion',()=>change({action:'visibility',id,value:!hidden},hidden?null:'Hide this suggestion from the public board? It can be restored.')),button(locked?'Reopen voting':'Close voting',()=>change({action:'lock',id,value:!locked})),button('Review voters',async()=>{reviewId=id;offset=0;await voters();$('voters').showModal();}));if(i.author){const blocked=Boolean(Number(i.author_blocked));actions.append(button(blocked?'Unblock author':'Block author',()=>change({action:'block',id,user:i.author,value:!blocked},blocked?'Restore this author’s access and votes?':'Block this anonymous author across the board? Their votes will be excluded; the suggestion stays visible until you hide it.'),'danger'));}body.append(actions,commentsFor(i));card.append(score,body);groups.get(category(i)).append(card);}
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

let forumThread=0,forumOffset=0;
async function reviewForum(thread=0,page=0){
 forumThread=thread;forumOffset=page;
 const data=await request({action:'forum_review',thread,offset:page}),root=$('forum-review');root.replaceChildren();
 const nav=el('div',undefined,'actions');
 nav.append(button('All threads',()=>reviewForum()),button('Previous',()=>reviewForum(thread,Math.max(0,page-20))),el('span',data.total?`${page+1}–${Math.min(page+20,data.total)} / ${data.total}`:'No threads yet'));
 if(page+20<data.total)nav.append(button('Next',()=>reviewForum(thread,page+20)));root.append(nav);
 const mutate=async(action,id,value)=>{await change({action,id,value});await reviewForum(thread,page);};
 if(!thread){for(const t of data.threads){const row=el('article',undefined,'panel'),actions=el('div',undefined,'actions');row.append(el('h3',t.title),el('p',`${t.author} · ${t.member} · ${t.posts} messages${Number(t.hidden)?' · Hidden':''}${Number(t.locked)?' · Closed':''}`));actions.append(button('Read / moderate',()=>reviewForum(Number(t.id))),button(Number(t.hidden)?'Restore':'Hide thread',()=>mutate('forum_hide_thread',Number(t.id),!Number(t.hidden))),button(Number(t.locked)?'Reopen':'Close thread',()=>mutate('forum_lock',Number(t.id),!Number(t.locked))));row.append(actions);root.append(row);}}
 else{root.append(el('h3',data.topic.title));if(!Number(data.topic.hidden)&&!Number(data.topic.locked)){root.append(ownerReply(thread));}else{root.append(el('p','Restore / reopen this thread before posting or reacting.','caption'));}for(const p of data.posts){const row=el('article',undefined,'panel'),actions=el('div',undefined,'actions');row.append(el('strong',`${p.author} · ${p.member} · #${p.id}${p.parent?' → #'+p.parent:''}`));
 if(/^[a-f0-9]{6144}$/i.test(p.avatar||'')){const canvas=el('canvas');canvas.width=32;canvas.height=32;canvas.setAttribute('aria-label','Member profile picture');const ctx=canvas.getContext('2d'),pixels=ctx.createImageData(32,32);for(let i=0;i<1024;i++){for(let c=0;c<3;c++)pixels.data[i*4+c]=parseInt(p.avatar.slice(i*6+c*2,i*6+c*2+2),16);pixels.data[i*4+3]=255;}ctx.putImageData(pixels,0,0);row.append(canvas);}
 row.style.marginLeft=`${Math.min(Number(p.depth)||0,3)*16}px`;if(p.parent){const quote=el('blockquote',undefined,'forum-body');quote.append(el('strong',`Reply to ${p.parent_author||'message'} #${p.parent}`),el('p',p.parent_body||'Original message unavailable'));row.append(quote);}if(Number(p.completed)){row.append(el('strong','Completed','caption'));}const body=el('p',p.body,'forum-body');row.append(body);if(!Number(p.hidden)&&!Number(data.topic.hidden)&&!Number(data.topic.locked)){const react=async value=>{await request({action:'owner_forum_react',thread,offset:page,post:Number(p.id),value:Number(p.mine)===value?0:value});await reviewForum(thread,page);};actions.append(button(`Like ${p.up}${Number(p.mine)===1?' ✓':''}`,()=>react(1)),button(`Dislike ${p.down}${Number(p.mine)===-1?' ✓':''}`,()=>react(-1)));row.append(ownerReply(thread,Number(p.id)));}actions.append(button(Number(p.completed)?'Reopen report':'Mark completed',()=>mutate('forum_complete_post',Number(p.id),!Number(p.completed))),button(Number(p.hidden)?'Restore post':'Hide post',()=>mutate('forum_hide_post',Number(p.id),!Number(p.hidden))),button(Number(p.blocked)?'Unblock author':'Block author',()=>mutate('forum_block',Number(p.id),!Number(p.blocked)),'danger'),button('Clear name / picture',()=>mutate('forum_clear_profile',Number(p.id),true),'danger'));row.append(actions);root.append(row);}}
}
$('forum-load').onclick=()=>run(()=>reviewForum());

let ownerAvatar='',pictureVersion=0;
function drawOwnerAvatar(){const canvas=$('owner-avatar'),ctx=canvas.getContext('2d');ctx.clearRect(0,0,32,32);if(/^[a-f0-9]{6144}$/.test(ownerAvatar)){const image=ctx.createImageData(32,32);for(let i=0;i<1024;i++){for(let c=0;c<3;c++)image.data[i*4+c]=parseInt(ownerAvatar.slice(i*6+c*2,i*6+c*2+2),16);image.data[i*4+3]=255;}ctx.putImageData(image,0,0);}else{ctx.fillStyle='#41533e';ctx.fillRect(0,0,32,32);ctx.fillStyle='#eff3e9';ctx.font='bold 14px sans-serif';ctx.textAlign='center';ctx.fillText(($('owner-name').value.trim().split(/\s+/).map(s=>s[0]||'').slice(0,2).join('')||'?').toUpperCase(),16,21);}}
async function loadOwnerProfile(){const p=await request({action:'owner_forum_profile_read'});$('owner-name').value=p.name;ownerAvatar=p.avatar;drawOwnerAvatar();}
$('owner-name').oninput=drawOwnerAvatar;
$('owner-picture').onchange=()=>run(async()=>{const file=$('owner-picture').files[0],version=++pictureVersion;if(!file)return;if(!['image/png','image/jpeg'].includes(file.type)||file.size>8*1024*1024)throw Error('Choose a PNG or JPEG smaller than 8 MB.');const bitmap=await createImageBitmap(file);try{if(version!==pictureVersion)return;const canvas=document.createElement('canvas');canvas.width=32;canvas.height=32;const ctx=canvas.getContext('2d'),side=Math.min(bitmap.width,bitmap.height);ctx.fillStyle='#ffffff';ctx.fillRect(0,0,32,32);ctx.drawImage(bitmap,(bitmap.width-side)/2,(bitmap.height-side)/2,side,side,0,0,32,32);const pixels=ctx.getImageData(0,0,32,32).data;let hex='';for(let i=0;i<pixels.length;i+=4)for(let c=0;c<3;c++)hex+=pixels[i+c].toString(16).padStart(2,'0');ownerAvatar=hex;drawOwnerAvatar();notice('Picture ready. Save administrator profile to make it public.');}finally{bitmap.close();}});
$('owner-picture-remove').onclick=()=>{pictureVersion++;ownerAvatar='';$('owner-picture').value='';drawOwnerAvatar();};
$('owner-profile-save').onclick=()=>run(async()=>{await request({action:'owner_forum_profile',name:$('owner-name').value.trim(),avatar:ownerAvatar});await reviewForum(forumThread,forumOffset);notice('Administrator profile saved. Your forum messages now show this name and picture.');});
$('owner-thread-post').onclick=()=>run(async()=>{const p=await request({action:'owner_forum_create',title:$('owner-thread-title').value.trim(),body:$('owner-thread-body').value.trim()});$('owner-thread-title').value='';$('owner-thread-body').value='';$('owner-new-thread').open=false;await reviewForum(Number(p.topic.id),Number(p.offset));notice('Your thread is live.');});
function ownerReply(thread,parent=null){const box=el('details',undefined,'public-comments');box.append(el('summary',parent?`Reply to message #${parent}`:'Reply to this thread'));const input=el('textarea');input.rows=3;input.maxLength=800;input.placeholder='Write your reply…';input.setAttribute('aria-label',parent?`Reply to message ${parent}`:'Thread reply');box.append(input,button('Post reply as administrator',async()=>{const p=await request({action:'owner_forum_reply',thread,parent,body:input.value.trim()});await reviewForum(thread,Number(p.offset));notice('Your reply is live.');},'primary'));return box;}

const deskSections = Array.from(document.querySelectorAll('#workspace > .desk-panel'));
const layoutKey = 'lemon.moderation.sections.v1';
const layoutButton = $('desk-toggle');

function updateLayoutButton() {
  layoutButton.textContent = deskSections.some(section => section.open) ? 'Collapse all' : 'Expand all';
}

function saveLayout() {
  updateLayoutButton();
  try {
    const layout = Object.fromEntries(deskSections.map(section => [section.id, section.open]));
    localStorage.setItem(layoutKey, JSON.stringify(layout));
  } catch {
    // Section controls still work when browser storage is unavailable.
  }
}

let savedLayout;
try {
  savedLayout = JSON.parse(localStorage.getItem(layoutKey));
} catch {
  // Use the default layout on first use or if storage is unavailable.
}
for (const section of deskSections) {
  if (typeof savedLayout?.[section.id] === 'boolean') section.open = savedLayout[section.id];
  section.addEventListener('toggle', saveLayout);
}
layoutButton.onclick = () => {
  const open = !deskSections.some(section => section.open);
  for (const section of deskSections) section.open = open;
  saveLayout();
};
updateLayoutButton();
