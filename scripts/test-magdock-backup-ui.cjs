const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync('../outputs/100-model-development/magdock-backup-ui-fixture.json','utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/magdock-backup-ui');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1120}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(f=>{
  const copy=v=>structuredClone(v);localStorage.removeItem('lighting-preset');localStorage.setItem('lighting-auto-mouse','false');
  const pc={kind:'gradient',style:0,color:[1,2,3],palette:[[1,2,3],[4,5,6]],brightness:.5,speed:1,gain:2,dock:false,rgb:[{id:1,key:'PC|fixture'}]};
  const qa=window.__dockBackup={view:copy(f.view),status:{running:true,kind:pc.kind,level:0,pixels:Array(12).fill([0,0,0]),error:null,config:copy(pc)},requests:[],legacy:false,holdOpen:false,held:[],ticks:[]};
  const interval=window.setInterval.bind(window);window.setInterval=(cb,delay,...args)=>{if(delay===12000){qa.ticks.push(()=>cb(...args));return 987654;}return interval(cb,delay,...args);};qa.tick=()=>qa.ticks.forEach(cb=>cb());
  const old=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(!['magdock_state','effects_status','rgb_devices','plugin:dialog|open','plugin:dialog|save','magdock_export','magdock_restore_preview','magdock_restore'].includes(cmd))return old(cmd,args);
   window.__qaCalls.push({cmd,args:copy(args)});
   if(cmd==='magdock_state')return copy(qa.view);if(cmd==='effects_status')return copy(qa.status);if(cmd==='rgb_devices')return[];
   if(cmd==='plugin:dialog|save')return 'export.json';
   if(cmd==='plugin:dialog|open'){const path=qa.legacy?'legacy.json':'input.json';if(qa.holdOpen){qa.holdOpen=false;return new Promise(resolve=>qa.held.push(()=>resolve(path)));}return path;}
   qa.requests.push({cmd,args:copy(args)});
   const expected=cmd==='magdock_restore'?args.selection.connection_key:args.expectedConnection;if(expected!==qa.view.connection_key)throw Error('Stale connection');
   if(cmd==='magdock_export')return;
   if(cmd==='magdock_restore_preview')return {...copy(qa.legacy?f.legacy_preview:f.preview),connection_key:qa.view.connection_key,expected_light:copy(qa.view.light)};
   if(cmd==='magdock_restore'){qa.view.light=copy(f.preview.light);return copy(qa.view);}
  };
 },fixture);
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'灯光同步',exact:true}).click();
 const region=page.getByLabel('底座灯效备份',{exact:true}),choose=region.getByRole('button',{name:'从备份恢复灯效',exact:true}),exportButton=region.getByRole('button',{name:'导出底座灯效',exact:true}),dialog=page.getByRole('dialog');
 await region.waitFor();assert(await exportButton.isEnabled());await exportButton.click();await page.waitForFunction(()=>window.__dockBackup.requests.length===1);
 await choose.click();await dialog.getByRole('heading',{name:'恢复底座灯效',exact:true}).waitFor();const table=dialog.getByRole('table',{name:'底座恢复预览',exact:true});await table.getByText('#9614dc',{exact:true}).waitFor();await table.getByText('#112233',{exact:true}).waitFor();assert.equal(await page.evaluate(()=>window.__dockBackup.requests.filter(r=>r.cmd==='magdock_restore').length),0);
 await dialog.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await page.evaluate(()=>window.__dockBackup.requests.length),2);
 await choose.click();await dialog.getByRole('button',{name:'恢复并核对',exact:true}).click();await page.waitForFunction(()=>window.__dockBackup.requests.some(r=>r.cmd==='magdock_restore'));await page.waitForFunction(()=>!document.querySelector('[role="dialog"]'));
 assert.equal(await page.getByLabel('底座灯光颜色',{exact:true}).inputValue(),'#9614dc');assert.equal(await page.evaluate(()=>window.__dockBackup.status.running),true);assert.equal(await page.evaluate(()=>window.__dockBackup.status.config.dock),false);
 await page.evaluate(()=>window.__dockBackup.legacy=true);await choose.click();await dialog.getByText(/旧版设置快照/).waitFor();await dialog.getByRole('button',{name:'取消',exact:true}).click();
 await page.evaluate(()=>window.__dockBackup.legacy=false);await choose.click();await dialog.waitFor();await page.setViewportSize({width:820,height:560});const box=await dialog.boundingBox();assert(box.x>=15&&box.width<=788&&box.height<=512);await dialog.getByRole('button',{name:'恢复并核对',exact:true}).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(out,'restore-preview-small.png'),animations:'disabled'});
 await page.evaluate(()=>{window.__dockBackup.view.connection_key='dock-B:2';window.__dockBackup.view.light.color=[20,40,60];window.__dockBackup.tick();});await page.waitForFunction(()=>!document.querySelector('[role="dialog"]'));assert.equal(await page.getByLabel('底座灯光颜色',{exact:true}).inputValue(),'#14283c');
 // A native file dialog finishing after leaving the page must not read a file
 // for the old connection or reopen the restore preview.
 await page.setViewportSize({width:1450,height:1120});await page.evaluate(()=>window.__dockBackup.holdOpen=true);await choose.click();await page.waitForFunction(()=>window.__dockBackup.held.length===1);await page.getByRole('button',{name:'设备',exact:true}).click();await page.evaluate(()=>window.__dockBackup.held.splice(0).forEach(cb=>cb()));await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));assert.equal(await page.evaluate(()=>window.__dockBackup.requests.length),6);
 await page.getByRole('button',{name:'灯光同步',exact:true}).click();await region.waitFor();await page.evaluate(()=>window.__dockBackup.status.config.dock=true);await page.waitForFunction(()=>[...document.querySelectorAll('button')].find(b=>b.textContent==='导出底座灯效')?.disabled);assert(await choose.isDisabled());
 const requests=await page.evaluate(()=>window.__dockBackup.requests);assert.equal(requests.length,6);assert.deepEqual(requests.map(r=>r.cmd),['magdock_export','magdock_restore_preview','magdock_restore_preview','magdock_restore','magdock_restore_preview','magdock_restore_preview']);assert.equal(requests[3].args.selection.sha256,fixture.preview.sha256);assert.deepEqual(requests[3].args.selection.expected_light,fixture.preview.expected_light);assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(out,'requests.json'),JSON.stringify(requests,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,native_preview_DTO:true,requests:6,checks:['explicit export uses selected connection','native current/source preview shows differences without writes','cancel produces no restore request','explicit restore carries file digest and complete preview context','readback updates dock draft and preserves independent PC status','legacy snapshot provenance is visible','preview and apply stay reachable at820x560','new connection closes old preview and replaces draft','late file dialog after unmount sends no IPC','dock-owned synchronization disables snapshot actions']},null,2));console.log('10 dock backup UI checks passed; 6 requests saved for native replay');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
