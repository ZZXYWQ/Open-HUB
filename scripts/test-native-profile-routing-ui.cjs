// Rust-generated DTOs; IPC is simulated and final calls are replayed by native tests.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync('../outputs/100-model-development/profile-routing-native-fixture.json','utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
const page=await browser.newPage({viewport:{width:1500,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
await page.addInitScript(f=>{
 let view=structuredClone(f.view),counter=0;const dpi={'logi-A':800,'logi-B':3200};
 const old=window.__TAURI_INTERNALS__.invoke;
 window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
 if(!['get_profiles','scan_devices','list_devices','device_state','activate_profile','save_profile','set_dpi'].includes(cmd))return old(cmd,args);
 window.__qaCalls.push({cmd,args});
 if(cmd==='get_profiles')return structuredClone(view);
 if(cmd==='scan_devices')return{devices:f.devices,warnings:[]};if(cmd==='list_devices')return f.devices;
 if(cmd==='device_state')return{battery:null,dpi:args.id.startsWith('logi-')?{feature:0x2201,x:dpi[args.id],y:null,default:800,supported:[400,800,1600,2400,3200,6400],supports_y:false,supports_lod:false,lod:null}:null,report_rate:null,onboard_mode:null,errors:[]};
 if(cmd==='set_dpi'){dpi[args.id]=args.x;return;}
 if(cmd==='save_profile'){
  const d=f.devices.find(d=>d.id===args.deviceId);if(!d)throw Error('Missing exact save target');
  if(view.profile_owners[args.profile.id]!==d.profile_key)throw Error('Cross-device profile edit');
  view.profiles=view.profiles.map(p=>p.id===args.profile.id?args.profile:p);return structuredClone(view);
 }
 const d=f.devices.find(d=>d.id===args.deviceId),source=view.profiles.find(p=>p.id===args.id);if(!d||!source)throw Error('Missing exact activation target');
 const owner=view.profile_owners[source.id];if(owner&&owner!==d.profile_key&&!args.asCopy)throw Error('Cross-device copy requires explicit flag');
 const set=view.device_profiles[d.profile_key]??={active_id:null,default_id:null,copies:{}};
 let p=source;if(owner!==d.profile_key){const previous=set.copies[source.id];p=view.profiles.find(p=>p.id===previous);if(!p){p={...structuredClone(source),id:'ui-copy-'+(++counter),ghub_id:null,target_vendor:d.vendor_id,target_model:parseInt(d.model_ids[0],16)};view.profiles.push(p);view.profile_owners[p.id]=d.profile_key;set.copies[source.id]=p.id;}}
 set.active_id=p.id;if(!p.app_path)set.default_id=p.id;view.active_id=p.id;
 return{id:p.id,name:p.name,reason:'manual',device_id:d.id,errors:[]};
 };
},fixture);
await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
await page.getByRole('button',{name:'配置',exact:true}).click();
const target=page.getByRole('combobox',{name:'配置 迈从板载模板 目标鼠标',exact:true});await target.waitFor();const card=page.locator('[data-slot="card"]').filter({has:target});const apply=card.getByRole('button',{name:'应用配置',exact:true});assert(await apply.isDisabled());await target.selectOption('mchose-B');await apply.click();await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='activate_profile'&&c.args.deviceId==='mchose-B'));
await page.getByRole('button',{name:'设备',exact:true}).click();await page.getByRole('button',{name:'Open G502 HERO',exact:true}).nth(0).click();
const picker=page.getByRole('combobox',{name:'Active profile',exact:true});await picker.waitFor();assert((await picker.textContent()).includes('鼠标 A 的独立配置'));assert(!(await picker.textContent()).includes('鼠标 B'));
await picker.click();assert.equal(await page.getByRole('option',{name:'鼠标 B 的独立配置',exact:true}).count(),0);await page.getByRole('option',{name:'罗技游戏模板',exact:true}).click();
await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='activate_profile'&&c.args.deviceId==='logi-A'));
await page.getByRole('button',{name:'DPI level 2: 1600',exact:true}).click({position:{x:20,y:30}});await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='save_profile'&&c.args.deviceId==='logi-A'));
await picker.click();await page.getByRole('option',{name:'罗技游戏模板',exact:true}).click();await page.waitForFunction(()=>window.__qaCalls.filter(c=>c.cmd==='activate_profile'&&c.args.deviceId==='logi-A').length===2);
await page.getByRole('button',{name:'Back to devices',exact:true}).click();await page.getByRole('button',{name:'配置',exact:true}).click();
const ownTarget=page.getByRole('combobox',{name:'配置 鼠标 A 的独立配置 目标鼠标',exact:true});await ownTarget.waitFor();const ownCard=page.locator('[data-slot="card"]').filter({has:ownTarget});await ownTarget.selectOption('logi-B');await ownCard.getByText('将保留原配置，为所选鼠标创建独立副本。',{exact:true}).waitFor();await ownCard.getByRole('button',{name:'复制并应用',exact:true}).click();
await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='activate_profile'&&c.args.deviceId==='logi-B'&&c.args.asCopy===true));
const calls=await page.evaluate(()=>window.__qaCalls),activations=calls.filter(c=>c.cmd==='activate_profile'),saves=calls.filter(c=>c.cmd==='save_profile');assert.deepEqual(activations.map(c=>c.args.deviceId),['mchose-B','logi-A','logi-A','logi-B']);assert.equal(saves.length,1);assert.equal(saves[0].args.profile.id,'private-A');assert.equal(saves[0].args.deviceId,'logi-A');assert.equal(saves[0].args.profile.active_level,1);assert.equal(saves[0].args.profile.levels[1].x,1600);assert.deepEqual(calls.filter(c=>c.cmd==='set_dpi').map(c=>c.args.id),['logi-A']);assert(!calls.some(c=>['multi_apply','sino_apply','write_onboard_bindings','mchose_hardware_apply'].includes(c.cmd)));assert.deepEqual(errors,[]);
const out=path.resolve('../outputs/100-model-development/profile-routing-ui');fs.mkdirSync(out,{recursive:true});await page.screenshot({path:path.join(out,'per-mouse-profile-copies.png')});fs.writeFileSync('../outputs/100-model-development/profile-routing-ui-calls.json',JSON.stringify(calls.filter(c=>['activate_profile','save_profile'].includes(c.cmd)),null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,native_DTO:true,checks:['two MCHOSE mice require explicit target','header shows open mouse active profile rather than last global one','other mouse private copy excluded from picker','header sends exact selected device ID','DPI edit saves only owned copy and writes selected unit','second native mouse unchanged until explicit copy','copy requires visible confirmation action and asCopy IPC flag']},null,2));console.log('Native per-mouse profile ownership UI passed; IPC calls saved for Rust replay');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
