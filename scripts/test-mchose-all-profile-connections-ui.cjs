// Built React UI: legacy board and generic DPI activations share the native
// connection guard. Native replay checks captured requests; no hardware access.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync('../outputs/100-model-development/profile-routing-native-fixture.json','utf8'));
const hardware=JSON.parse(fs.readFileSync('src-tauri/tests/fixtures/mchose.json','utf8')).state;
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mchose-all-profile-connections');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true});try{
const captures=[];
for(const kind of ['board','generic']){
 const profile=structuredClone(fixture.view.profiles.find(p=>p.id===(kind==='board'?'mchose-template':'desktop')));assert(profile);
 const page=await browser.newPage({viewport:{width:1500,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)};window.__emitDevices=()=>{for(const handler of events.get('devices-changed')||[])callbacks.get(handler)?.({event:'devices-changed',id:handler,payload:null});};})()`});
 await page.addInitScript(({profile,hardware})=>{
 const peers=['A','B'].map(key=>({id:'mchose-'+key,name:'A7 V3 Ultra+ '+key,product:'MCHOSE A7 V3 Ultra+',product_id:'1014',vendor_id:0x3837,model_ids:['4026'],kind:'mouse',firmware_name:'A7 V3 Ultra+',connection:'wireless',protocol:'MCHOSE',features:[],profile_key:'session:mchose-'+key,connection_key:'connection:mchose-'+key+':1'}));
 peers.push({id:'logi-A',name:'G502 HERO',product:'G502 HERO',product_id:'c08b',vendor_id:0x046d,model_ids:['c08b'],kind:'mouse',connection:'USB wired',protocol:'HID++',features:[],profile_key:'unit:logi-A'});
 const view={profiles:[profile],active_id:profile.id,auto_switch:false,profile_owners:{},device_profiles:{}};
 window.__requests=[];window.__race=false;let generation=1;
 window.__connection=value=>{peers[0].connection_key=value;window.__emitDevices();};
 const old=window.__TAURI_INTERNALS__.invoke;
 window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
 if(!['get_profiles','scan_devices','list_devices','activate_profile','mchose_hardware'].includes(cmd))return old(cmd,args);
 window.__qaCalls.push({cmd,args:structuredClone(args)});
 if(cmd==='get_profiles')return structuredClone(view);
 if(cmd==='scan_devices')return{devices:structuredClone(peers),warnings:[]};if(cmd==='list_devices')return structuredClone(peers);
 if(cmd==='mchose_hardware'){const d=peers.find(d=>d.id===args.id);return{...structuredClone(hardware),adaptive:[],target:{device_key:d.profile_key,connection_key:d.connection_key}};}
 const d=peers.find(d=>d.id===args.deviceId);if(!d)throw Error('Missing explicit mouse');
 if(window.__race){window.__race=false;generation=2;peers[0].connection_key='connection:mchose-A:2';}
 const ok=d.vendor_id!==0x3837||!!d.connection_key&&args.expectedConnection===d.connection_key;
 window.__requests.push({args:structuredClone(args),server_generation:d.id==='mchose-A'?generation:1,expected_success:ok});
 if(!ok){window.__emitDevices();throw Error('当前连接已改变，请重新选择鼠标');}
 return{id:profile.id,name:profile.name,reason:'manual',device_id:d.id,errors:[]};
 };
 },{profile,hardware});
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'配置',exact:true}).click();
 const target=page.getByRole('combobox',{name:`配置 ${profile.name} 目标鼠标`,exact:true});await target.waitFor();
 const card=page.locator('[data-slot="card"]').filter({has:target}),apply=card.getByRole('button',{name:'应用配置',exact:true});
 assert(await apply.isDisabled());assert.equal(await target.locator('option').count(),kind==='generic'?4:3);
 await target.selectOption('mchose-A');await apply.click();await page.waitForFunction(()=>window.__requests.length===1);
 await page.evaluate(()=>window.__race=true);await apply.click();await page.waitForFunction(()=>window.__requests.length===2);
 await page.getByText('Error: 当前连接已改变，请重新选择鼠标',{exact:true}).waitFor();
 await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='list_devices'));await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
 await apply.click();await page.waitForFunction(()=>window.__requests.length===3);
 await target.selectOption('mchose-B');await apply.click();await page.waitForFunction(()=>window.__requests.length===4);
 await page.getByRole('button',{name:'设备',exact:true}).click();await page.getByRole('button',{name:'Open A7 V3 Ultra+ A',exact:true}).click();
 const picker=page.getByRole('combobox',{name:'Active profile',exact:true});await picker.waitFor();await picker.click();await page.getByRole('option',{name:profile.name,exact:true}).click();
 await page.waitForFunction(()=>window.__requests.length===5);
 const before=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='list_devices').length);await page.evaluate(()=>window.__connection(''));
 await page.waitForFunction(n=>window.__qaCalls.filter(c=>c.cmd==='list_devices').length>n,before);await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
 await picker.click();assert.equal(await page.getByRole('option',{name:profile.name,exact:true}).getAttribute('aria-disabled'),'true');await page.keyboard.press('Escape');
 await page.getByRole('button',{name:'Back to devices',exact:true}).click();await page.getByRole('button',{name:'配置',exact:true}).click();await target.selectOption('mchose-A');assert(await apply.isDisabled());
 const requests=await page.evaluate(()=>window.__requests);assert.deepEqual(requests.map(r=>r.expected_success),[true,false,true,true,true]);assert.deepEqual(requests.map(r=>r.args.expectedConnection),['connection:mchose-A:1','connection:mchose-A:1','connection:mchose-A:2','connection:mchose-B:1','connection:mchose-A:2']);assert.deepEqual(errors,[]);
 captures.push({kind,profile,requests});await page.screenshot({path:path.join(out,kind+'.png'),animations:'disabled'});await page.close();
}
fs.writeFileSync(path.join(out,'activation-requests.json'),JSON.stringify(captures,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,profile_shapes:['legacy board reference','unbranded generic DPI'],IPC_requests:10,checks:['explicit target required','same-owner same-path reconnect rejects stale generation','no retry until next user action','fresh generation accepted','second mouse keeps own token','header captures generation for both profile shapes','missing token disables header and center','generic Desktop retains Logitech as candidate']},null,2));console.log('Legacy board and generic DPI connection UI passed; 10 requests saved for native preflight replay');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
