"""Render the shared native-app changelog into the download page. Run after editing changelog.json."""
from pathlib import Path
import html,json,re
root=Path(__file__).resolve().parent
history=json.loads((root/'changelog.json').read_text(encoding='utf-8'))
parts=['<!-- CHANGELOG START -->','<section class="source" id="change-log" aria-labelledby="change-log-title"><h2 id="change-log-title">Change Log</h2>', '<p>'+html.escape(history['intro'])+'</p>', '<label for="change-search">Search changes</label><input id="change-search" type="search" placeholder="Try Chop, mixer or recording" style="display:block;width:100%;margin:12px 0 20px;padding:12px;border:1px solid #66734c;border-radius:8px;background:#151914;color:#edf0e6;font:inherit">','<p id="change-empty" hidden>No changes match your search.</p>']
days={}
for section in history['sections']:
    day=section['date'][:10]
    changes=days.setdefault(day,[])
    for change in section['changes']:
        if change not in changes:changes.append(change)
for i,(day,changes) in enumerate(sorted(days.items(),reverse=True)):
    parts.append('<details class="change-entry"'+(' open' if i==0 else '')+' style="border-top:1px solid #394133;padding:18px 0"><summary style="cursor:pointer;color:#e0edab;font-weight:650;line-height:1.6">'+html.escape(day)+'</summary><ul style="padding-left:22px;color:#b9c1b5;line-height:1.7">')
    parts.extend('<li style="margin:10px 0">'+html.escape(c)+'</li>' for c in changes)
    parts.append('</ul></details>')
parts.extend(['</section>','<script>','document.getElementById("change-search").addEventListener("input",function(){const q=this.value.trim().toLowerCase();let any=false;document.querySelectorAll(".change-entry").forEach(entry=>{const title=entry.querySelector("summary").textContent.toLowerCase();let matched=false;entry.querySelectorAll("li").forEach(li=>{li.hidden=!!q&&!title.includes(q)&&!li.textContent.toLowerCase().includes(q);if(!li.hidden)matched=true;});entry.hidden=!matched;if(q&&matched)entry.open=true;if(matched)any=true;});document.getElementById("change-empty").hidden=any;});','</script>','<!-- CHANGELOG END -->'])
p=root/'download-page.html';page=p.read_text(encoding='utf-8');block='\n'.join(parts)
if '<!-- CHANGELOG START -->' in page:page=re.sub(r'<!-- CHANGELOG START -->.*?<!-- CHANGELOG END -->',lambda _:block,page,flags=re.S)
else:page=page.replace('<footer>',block+'\n<footer>')
if 'href="#change-log"' not in page:page=page.replace('<section class="grid"','<p><a href="#change-log">Read the Change Log ↓</a></p>\n<section class="grid"',1)
p.write_text(page,encoding='utf-8')
print(f'Rendered {len(history["sections"])} sections, {sum(len(s["changes"]) for s in history["sections"])} changes')
