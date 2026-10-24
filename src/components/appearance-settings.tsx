import {useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {save} from '@tauri-apps/plugin-dialog';
import {Button} from '@/components/ui/button';
import {getSkin,setSkin,SKINS,SKIN_LIMIT,parseSkin,type HubSkin} from '@/lib/appearance';
export function AppearanceSettings(){
 const [skin,update]=useState<HubSkin>(getSkin),[error,setError]=useState(''),[note,setNote]=useState('');const input=useRef<HTMLInputElement>(null);
 const apply=(next:HubSkin)=>{try{setSkin(next);update(next);setError('');setNote('');return true;}catch(e){setError(String(e));return false;}};
 const patch=(p:Partial<HubSkin>)=>apply({...skin,...p,name:'自定义'});
 return <section aria-label="外观与皮肤" className="space-y-4"><div className="flex items-center gap-3"><img src="/open-hub.png" alt="Open HUB 独立图标" className="size-12"/><div><h3 className="font-semibold">外观与皮肤</h3><p className="text-xs text-muted-foreground">实时预览，自动保存。只调整界面，不修改鼠标灯光。</p></div></div>
 <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">{SKINS.map(s=><button key={s.name} aria-label={`使用${s.name}皮肤`} aria-pressed={skin.name===s.name} className="hub-skin-swatch" onClick={()=>apply({...s})}><span style={{background:s.background}}><i style={{background:s.surface}}/><b style={{background:s.accent}}/></span><small>{s.name}</small></button>)}</div>
 <div className="grid grid-cols-3 gap-3">{([['accent','强调色'],['background','背景色'],['surface','面板色']] as const).map(([key,label])=><label key={key} className="flex items-center justify-between gap-2 text-xs">{label}<input type="color" aria-label={`界面${label}`} value={skin[key]} onChange={e=>patch({[key]:e.target.value})}/></label>)}</div>
 <div className="flex flex-wrap gap-2">{[{name:'实色',opacity:100,panelOpacity:100,blur:0},{name:'半透明',opacity:72,panelOpacity:80,blur:12},{name:'透明',opacity:0,panelOpacity:62,blur:0}].map(mode=><Button key={mode.name} size="sm" variant="outline" onClick={()=>patch({opacity:mode.opacity,panelOpacity:mode.panelOpacity,blur:mode.blur})}>{mode.name}</Button>)}</div>
 <div className="grid gap-3 sm:grid-cols-2">{([['opacity','窗口背景不透明度',0,100,'%'],['panelOpacity','面板不透明度',20,100,'%'],['blur','面板磨砂',0,24,'px'],['radius','圆角',0,24,'px']] as const).map(([key,label,min,max,unit])=><label key={key} className="grid gap-2 text-xs">{label} {skin[key]}{unit}<input aria-label={label} type="range" min={min} max={max} value={skin[key]} onChange={e=>patch({[key]:Number(e.target.value)})}/></label>)}</div>
 <p className="text-xs text-muted-foreground">透明度作用于窗口内容背景，标题栏跟随系统；磨砂只作用于面板。设为实色可减少合成开销。</p>
 <input ref={input} aria-label="选择 Open HUB 皮肤文件" className="hidden" type="file" accept=".json,.openhubskin" onChange={async e=>{const file=e.target.files?.[0];e.target.value='';if(!file)return;try{if(file.size>SKIN_LIMIT)throw Error('皮肤文件不能超过 8 KB');const next=parseSkin(await file.text());if(apply(next))setNote(`已加载：${next.name}`);}catch(err){setError(String(err));}}}/>
 <div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" onClick={()=>input.current?.click()}>加载自己的皮肤</Button><Button variant="outline" size="sm" onClick={async()=>{setError('');try{const path=await save({defaultPath:'Open-HUB-我的皮肤.json',filters:[{name:'Open HUB 皮肤',extensions:['json']}]});if(path){await invoke('export_ui_skin',{path,skin});setNote('皮肤文件已导出，可在其他电脑加载');}}catch(e){setError(String(e));}}}>导出当前皮肤</Button><Button variant="ghost" size="sm" onClick={()=>apply({...SKINS[0]})}>恢复默认外观</Button></div>
 <p className="text-xs text-muted-foreground">皮肤文件最多 8 KB，包含配色、透明度、磨砂和圆角；没有额外主题运行库。</p>{error&&<p role="alert" className="lighting-error">{error}</p>}{note&&<p role="status" className="text-xs">{note}</p>}
 </section>;
}
