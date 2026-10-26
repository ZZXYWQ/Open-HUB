// Actual built React requests, replayed by the native full-profile test.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const profile=JSON.parse(fs.readFileSync('../outputs/100-model-development/mhub-full-profile-ui-fixture.json','utf8'));
const hardware=JSON.parse(fs.readFileSync('src-tauri/tests/fixtures/mchose.json','utf8')).state;
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mhub-full-profile-ui');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true});try{
const page=await browser.newPage({viewport:{width:1500,height:1100}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
await page.addInitScript({content:`(()=>{${base.slice(start,end)};window.__emitDevices=()=>{for(const handler of events.get('devices-changed')||[])callbacks.get(handler)?.({event:'devices-changed',id:handler,payload:null});};})()`});
await page.addInitScript(({profile,hardware})=>{
 const peers=['A','B'].map(key=>({id:'mchose-'+key,name:'A7 V3 Ultra+ '+key,product:'MCHOSE A7 V3 Ultra+',product_id:'1014',vendor_id:0x3837,model_ids:['4026'],kind:'mouse',firmware_name:'A7 V3 Ultra+',connection:'wireless',protocol:'MCHOSE',features:[],profile_key:'session:mchose-'+key,connection_key:'connection:mchose-'+key+':1'}));
 peers.push({id:'logi-A',name:'G502 HERO',product:'G502 HERO',product_id:'c08b',vendor_id:0x046d,model_ids:['c08b'],kind:'mouse',connection:'USB wired',protocol:'HID++',features:[],profile_key:'unit:logi-A',connection_key:'connection:logi-A:1'});
 const view={profiles:[profile],active_id:profile.id,auto_switch:false,profile_owners:{},device_profiles:{}};
 window.__fullRequests=[];window.__race=false;window.__clearConnection=()=>{peers[0].connection_key='';window.__emitDevices();};let generation=1;
 const old=window.__TAURI_INTERNALS__.invoke;
 window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(!['get_profiles','scan_devices','list_devices','activate_profile','mchose_hardware'].includes(cmd))return old(cmd,args);
  window.__qaCalls.push({cmd,args:structuredClone(args)});
  if(cmd==='get_profiles')return structuredClone(view);
  if(cmd==='scan_devices')return {devices:structuredClone(peers),warnings:[]};if(cmd==='list_devices')return structuredClone(peers);
  if(cmd==='mchose_hardware'){const d=peers.find(d=>d.id===args.id);return {...structuredClone(hardware),adaptive:[],target:{device_key:d.profile_key,connection_key:d.connection_key}};}
  const d=peers.find(d=>d.id===args.deviceId);if(!d)throw Error('Missing explicit mouse');
  if(window.__race){window.__race=false;generation=2;peers[0].profile_key='session:mchose-A:2';peers[0].connection_key='connection:mchose-A:2';}
  const ok=!!d.connection_key&&args.expectedConnection===d.connection_key;
  window.__fullRequests.push({args:structuredClone(args),server_generation:d.id==='mchose-A'?generation:1,expected_success:ok});
  if(!ok){window.__emitDevices();throw Error('当前连接已改变，请重新选择鼠标');}
  return {id:profile.id,name:profile.name,reason:'manual',device_id:d.id,errors:[]};
 };
},{profile,hardware});
await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
await page.getByRole('button',{name:'配置',exact:true}).click();
const target=page.getByRole('combobox',{name:`配置 ${profile.name} 目标鼠标`,exact:true});await target.waitFor();
const card=page.locator('[data-slot="card"]').filter({has:target}),apply=card.getByRole('button',{name:'应用配置',exact:true});
assert(await apply.isDisabled());assert.equal(await target.locator('option').count(),3);
await card.getByText('USB 1000 Hz / 无线 2000 Hz',{exact:true}).waitFor();await card.getByText('900 × 950',{exact:true}).waitFor();
await target.selectOption('mchose-A');await apply.click();await page.waitForFunction(()=>window.__fullRequests.length===1);
await page.evaluate(()=>window.__race=true);await apply.click();await page.waitForFunction(()=>window.__fullRequests.length===2);
await page.getByText('Error: 当前连接已改变，请重新选择鼠标',{exact:true}).waitFor();
await page.waitForFunction(()=>window.__qaCalls.filter(c=>c.cmd==='list_devices').length>=1);
await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
await apply.click();await page.waitForFunction(()=>window.__fullRequests.length===3);
assert.equal((await page.evaluate(()=>window.__fullRequests))[2].args.expectedConnection,'connection:mchose-A:2');
await target.selectOption('mchose-B');await apply.click();await page.waitForFunction(()=>window.__fullRequests.length===4);
// Generic profile editor must not independently change a full plan's summary.
await card.getByRole('button',{name:`Edit ${profile.name}`,exact:true}).click();
await page.getByText('迈从完整设备方案',{exact:true}).waitFor();assert.equal(await page.getByLabel('Level 1 DPI',{exact:true}).count(),0);
await page.getByRole('button',{name:'Cancel',exact:true}).click();
await page.getByRole('button',{name:'设备',exact:true}).click();await page.getByRole('button',{name:'Open A7 V3 Ultra+ A',exact:true}).click();
const picker=page.getByRole('combobox',{name:'Active profile',exact:true});await picker.waitFor();await picker.click();await page.getByRole('option',{name:profile.name,exact:true}).click();
await page.waitForFunction(()=>window.__fullRequests.length===5);
await page.getByRole('button',{name:'Back to devices',exact:true}).click();await page.getByRole('button',{name:'配置',exact:true}).click();
await target.selectOption('mchose-A');const listsBefore=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='list_devices').length);await page.evaluate(()=>window.__clearConnection());
await page.waitForFunction(n=>window.__qaCalls.filter(c=>c.cmd==='list_devices').length>n,listsBefore);await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));assert(await apply.isDisabled());
const requests=await page.evaluate(()=>window.__fullRequests);assert.deepEqual(requests.map(r=>r.expected_success),[true,false,true,true,true]);assert.deepEqual(requests.map(r=>r.args.expectedConnection),['connection:mchose-A:1','connection:mchose-A:1','connection:mchose-A:2','connection:mchose-B:1','connection:mchose-A:2']);assert.deepEqual(errors,[]);
fs.writeFileSync(path.join(out,'activation-requests.json'),JSON.stringify(requests,null,2));await page.screenshot({path:path.join(out,'full-profile-connections.png')});
fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,source_fixture:'native to_profile(Plan)',checks:['two same-model mice require explicit target','Logitech excluded','separate USB/wireless and XY summary','selected connection captured by profile center','reconnect race rejects old selection without retry','fresh generation can apply','second mouse retains own connection','generic editor preserves complete payload','header captures current connection','missing connection disables activation']},null,2));console.log('10 full MCHOSE profile UI checks passed; native replay requests saved');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
