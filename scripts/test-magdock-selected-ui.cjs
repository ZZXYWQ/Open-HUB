// Actual built React requests with Rust-exported DTOs. Native replay uses the
// production controller/engine plus simulated reports and audio levels.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync('../outputs/100-model-development/magdock-selected-ui-fixture.json','utf8'));
const out=path.resolve('../outputs/100-model-development/magdock-selected-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1120}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(f=>{
  const copy=v=>structuredClone(v);localStorage.removeItem('lighting-preset');localStorage.setItem('lighting-auto-mouse','true');localStorage.setItem('lighting-dock-excluded','false');
  const qa=window.__dockSelected={dock:copy(f.a),status:{running:false,kind:'',level:0,pixels:Array(12).fill([0,0,0]),error:null,config:null},requests:[],hold:false,held:[],discovery:[],missing:false};
  const interval=window.setInterval.bind(window);window.setInterval=(callback,delay,...args)=>{if(delay===12000){qa.discovery.push(()=>callback(...args));return 987654;}return interval(callback,delay,...args);};
  qa.tick=()=>qa.discovery.forEach(cb=>cb());qa.replace=key=>{qa.dock={...copy(f.b),connection_key:key};};
  const old=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(!['magdock_state','magdock_apply','magdock_reset','magdock_music','effects_status','effects_start','effects_stop','rgb_devices','plugin:dialog|save','effects_preset'].includes(cmd))return old(cmd,args);
   window.__qaCalls.push({cmd,args:copy(args)});
   if(cmd==='rgb_devices')return[];
   if(cmd==='effects_status')return copy(qa.status);
   if(cmd==='effects_stop'){qa.status.running=false;return copy(qa.status);}
   if(cmd==='plugin:dialog|save')return 'simulated-lighting.json';if(cmd==='effects_preset')return;
   if(cmd==='magdock_state'){
    const snapshot=copy(qa.dock);if(qa.missing)snapshot.connection_key='';
    if(qa.hold){qa.hold=false;return new Promise(resolve=>qa.held.push(()=>resolve(snapshot)));}return snapshot;
   }
   const expected=cmd==='effects_start'?args.expectedDockConnection:args.expectedConnection,ok=expected===qa.dock.connection_key;
   qa.requests.push({cmd,args:copy(args),server_connection:qa.dock.connection_key,expected_success:ok});
   if(!ok)throw Error('底座连接已改变，请重新读取');
   if(cmd==='effects_start'){qa.status={...qa.status,running:true,kind:args.config.kind,config:copy(args.config)};return copy(qa.status);}
   if(cmd==='magdock_apply')qa.dock.light=copy(args.light);
   if(cmd==='magdock_reset')qa.dock.light=copy(f.a.light);
   if(cmd==='magdock_music'){qa.dock.music_running=args.enabled;qa.dock.audio_level=args.enabled?0.7:0;if(args.enabled)qa.dock.light={...copy(args.light),mode:5,sync:true};else qa.dock.light=copy(f.a.light);}
   return copy(qa.dock);
  };
 },fixture);
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'灯光同步',exact:true}).click();
 const panel=page.locator('.lighting-studio'),dock=panel.getByRole('region',{name:'独立底座灯光',exact:true}),color=dock.getByLabel('底座灯光颜色',{exact:true}),apply=dock.getByRole('button',{name:'应用到底座',exact:true}),read=dock.getByRole('button',{name:'读取',exact:true}),mode=dock.getByRole('combobox',{name:'原厂灯效模式',exact:true});
 await dock.waitFor();assert.equal(await color.inputValue(),'#112233');await color.fill('#1234ab');await apply.click();
 await page.waitForFunction(()=>window.__dockSelected.requests.length===1);await dock.getByRole('button',{name:'恢复原厂灯效',exact:true}).click();await page.waitForFunction(()=>window.__dockSelected.requests.length===2);
 await mode.selectOption('5');await apply.click();await dock.getByRole('status',{name:'底座律动状态',exact:true}).filter({hasText:'运行中'}).waitFor();
 await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).click();await page.waitForFunction(()=>window.__dockSelected.requests.length===4);
 // A new session appears without updating the UI snapshot. Reject old A.
 await color.fill('#ff00aa');await page.evaluate(()=>window.__dockSelected.replace('dock-B:1'));await apply.click();await page.waitForFunction(()=>window.__dockSelected.requests.length===5);
 await dock.getByRole('alert',{name:'底座灯光错误',exact:true}).filter({hasText:'底座连接已改变'}).waitFor();
 assert.equal(await page.evaluate(()=>window.__dockSelected.requests.length),5);
 // Hold a discovery result from A, then explicitly read B. Releasing the old
 // result cannot replace B's fresh token or its draft.
 await page.evaluate(f=>{window.__dockSelected.dock=structuredClone(f.a);window.__dockSelected.hold=true;window.__dockSelected.tick();},fixture);
 await page.waitForFunction(()=>window.__dockSelected.held.length===1);await page.evaluate(()=>window.__dockSelected.replace('dock-B:1'));await read.click();
 await page.waitForFunction(()=>window.__qaCalls.filter(c=>c.cmd==='magdock_state').length>=3);await color.waitFor();await page.waitForFunction(()=>document.querySelector('input[aria-label="底座灯光颜色"]').value==='#14283c');
 await page.evaluate(()=>window.__dockSelected.held.splice(0).forEach(release=>release()));await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));assert.equal(await color.inputValue(),'#14283c');
 await color.fill('#abcdef');await apply.click();await page.waitForFunction(()=>window.__dockSelected.requests.length===6);
 await panel.getByRole('button',{name:'常亮',exact:true}).click();await page.evaluate(()=>window.__dockSelected.replace('dock-B:2'));await panel.getByRole('button',{name:'开始同步',exact:true}).click();await page.waitForFunction(()=>window.__dockSelected.requests.length===7);
 await panel.getByRole('alert').filter({hasText:'底座连接已改变'}).waitFor();assert.equal(await page.evaluate(()=>window.__dockSelected.status.running),false);
 await read.click();await page.waitForFunction(()=>document.querySelector('input[aria-label="底座灯光颜色"]').value==='#14283c');await panel.getByRole('button',{name:'开始同步',exact:true}).click();await panel.getByRole('button',{name:'应用并重新同步',exact:true}).waitFor();
 const requests=await page.evaluate(()=>window.__dockSelected.requests);assert.equal(requests.length,8);assert.deepEqual(requests.map(r=>r.expected_success),[true,true,true,true,false,true,false,true]);
 assert.deepEqual(requests.map(r=>r.args.expectedConnection||r.args.expectedDockConnection),['dock-A:1','dock-A:1','dock-A:1','dock-A:1','dock-A:1','dock-B:1','dock-B:1','dock-B:2']);
 assert(requests.filter(r=>r.cmd==='effects_start').every(r=>!JSON.stringify(r.args.config).includes('connection')));
 await panel.getByRole('button',{name:'导出方案',exact:true}).click();await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='effects_preset'));
 const exported=await page.evaluate(()=>window.__qaCalls.find(c=>c.cmd==='effects_preset').args);assert(!JSON.stringify(exported).includes('dock-B:2'));
 await panel.getByRole('button',{name:'停止并恢复',exact:true}).click();await panel.getByRole('button',{name:'开始同步',exact:true}).waitFor();
 await page.evaluate(()=>window.__dockSelected.missing=true);await read.click();await page.waitForFunction(()=>!document.querySelector('[aria-label="独立底座灯光"]'));assert(await panel.getByRole('button',{name:'开始同步',exact:true}).isDisabled());
 const absentChoice=panel.locator('label.light-target').filter({hasText:'MCHOSE MagDock'}).getByRole('checkbox');assert(await absentChoice.isEnabled());await absentChoice.uncheck();assert(!(await absentChoice.isChecked()));
 assert.deepEqual(errors,[]);fs.writeFileSync(path.join(out,'requests.json'),JSON.stringify(requests,null,2));await page.screenshot({path:path.join(out,'missing-connection-disabled.png'),animations:'disabled'});
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,native_DTO:true,requests:8,accepted:6,rejected:2,checks:['apply/reset/music/stop carry preview connection','stale native write rejected without retry','fresh different connection replaces old color draft','late discovery result cannot replace explicit new read','fresh apply uses B','stale effect start preserves stopped status','explicit read permits fresh-generation start','connection is separate from reusable config and export','missing token clears old controls and disables dock sync','unavailable selected dock can still be deselected for independent PC use']},null,2));console.log('10 selected dock connection UI checks passed; 8 requests saved for native replay');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
