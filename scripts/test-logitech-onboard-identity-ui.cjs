const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_LOGITECH_BINDING_FIXTURE||'../outputs/100-model-development/logitech-binding-native-fixture.json','utf8'));
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/logitech-onboard-identity-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
const page=await browser.newPage({viewport:{width:1450,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
await page.addInitScript(f=>{
 let current=f.snapshot;window.__staleSlot=true;window.__dpiRefreshWarning=false;const old=window.__TAURI_INTERNALS__.invoke;
 const profile=(id,x)=>({id,name:id,target_vendor:0x046d,target_model:0xc08b,mchose_board:null,app_path:null,levels:[{x,y:null},{x:x*2,y:null}],active_level:0,report_rate_hz:500});
 window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
  if(!['scan_devices','list_devices','device_state','onboard_snapshot','get_profiles','write_onboard_slot','restore_onboard_backup','preview_onboard_restore','plugin:dialog|open'].includes(cmd))return old(cmd,a);
  window.__qaCalls.push({cmd,args:a});
  if(cmd==='scan_devices')return {devices:[f.device],warnings:[]};if(cmd==='list_devices')return[f.device];
  if(cmd==='device_state')return {battery:null,dpi:null,report_rate:null,onboard_mode:'onboard',errors:[]};if(cmd==='onboard_snapshot')return current;
  if(cmd==='get_profiles')return {profiles:[profile('other-mouse-global',6400),profile('this-mouse',1200)],active_id:'other-mouse-global',auto_switch:false,device_profiles:{[current.device_key]:{active_id:'this-mouse',default_id:'this-mouse',copies:{}}},profile_owners:{'other-mouse-global':'unit:046d:another','this-mouse':current.device_key}};
  if(cmd==='write_onboard_slot'){if(window.__staleSlot)throw Error('mouse connection changed since preview; read it again');current=structuredClone(f.after_dpi_snapshot);current.profiles[0].default_dpi_index=a.settings.default_index;current.profiles[0].dpi_levels=a.settings.dpi_levels;current.profiles[0].report_rate_hz=a.settings.report_rate_hz;return {report:f.after_dpi_report,backup_path:'memory-test-backup.json',snapshot:window.__dpiRefreshWarning?null:current,refresh_note:window.__dpiRefreshWarning?'DPI saved and readback verified; refreshing the view failed: simulated disconnect':null};}
  if(cmd==='preview_onboard_restore')return f.restore_preview;
  if(cmd==='plugin:dialog|open')return 'memory-test-backup.json';
  if(cmd==='restore_onboard_backup')return {sectors_written:0,backup_path:'memory-before-restore.json',snapshot:current};
 };
},fixture);
await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'Open G502 HERO',exact:true}).click();await page.getByRole('navigation',{name:'Device features',exact:true}).getByRole('button',{name:'Device settings',exact:true}).click();
assert(await page.getByRole('button',{name:'Restore from backup…',exact:true}).isDisabled());
await page.getByRole('button',{name:'Read from mouse',exact:true}).click();await page.getByRole('button',{name:'Edit slot 1',exact:true}).click();const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:'使用此鼠标的当前配置',exact:true}).click();
await page.waitForFunction(()=>document.querySelector('[aria-label="Level 1 DPI"]')?.value==='1200');assert.equal(await dialog.getByRole('spinbutton',{name:'Level 2 DPI',exact:true}).inputValue(),'2400');
await dialog.getByRole('spinbutton',{name:'Level 1 DPI',exact:true}).fill('400');await dialog.getByRole('button',{name:'Save to mouse',exact:true}).click();await page.getByText(/mouse connection changed since preview/).waitFor();assert(await dialog.isVisible());assert.equal(await dialog.getByRole('spinbutton',{name:'Level 1 DPI',exact:true}).inputValue(),'400');
await page.evaluate(()=>window.__staleSlot=false);await dialog.getByRole('button',{name:'Save to mouse',exact:true}).click();await dialog.waitFor({state:'hidden'});
await page.getByRole('button',{name:'Edit slot 1',exact:true}).click();await page.evaluate(()=>window.__dpiRefreshWarning=true);await dialog.getByRole('button',{name:'Save to mouse',exact:true}).click();await dialog.waitFor({state:'hidden'});await page.getByText('DPI 已保存并校验',{exact:true}).waitFor();assert(await page.getByRole('button',{name:'Restore from backup…',exact:true}).isDisabled());await page.getByRole('button',{name:'Read from mouse',exact:true}).click();
await page.getByRole('button',{name:'Restore from backup…',exact:true}).click();await page.getByRole('dialog').getByRole('button',{name:'Restore',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
const calls=await page.evaluate(()=>window.__qaCalls),writes=calls.filter(x=>x.cmd==='write_onboard_slot');assert.equal(writes.length,3);const s=writes.at(-1).args.settings;assert.equal(s.expected_device,fixture.snapshot.device_key);assert.equal(s.expected_connection,fixture.snapshot.connection_key);assert.equal(s.sector,1);assert.deepEqual(s.dpi_levels,[400,2400]);assert.equal(s.default_index,0);assert.equal(s.report_rate_hz,500);
const restore=calls.find(x=>x.cmd==='restore_onboard_backup');assert.deepEqual(restore.args,{id:fixture.device.id,path:'memory-test-backup.json',expectedDevice:fixture.snapshot.device_key,expectedConnection:fixture.snapshot.connection_key,expectedBackupDigest:fixture.restore_backup_digest});assert(!calls.some(x=>['set_dpi','set_report_rate','multi_apply','activate_profile'].includes(x.cmd)));assert.deepEqual(errors,[]);
fs.writeFileSync('../outputs/100-model-development/logitech-ui-slot-settings.json',JSON.stringify(s,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,checks:['restore waits for reviewed current-device snapshot','slot editor reads only this mouse private profile','global last-used profile does not leak into slot','stale preview keeps edits','verified DPI save with refresh failure clears stale view and reports saved','DPI write carries connection/unit identity and reviewed sector','restore carries current target identity','no live/global profile activation']},null,2));console.log('Logitech on-board per-mouse DPI and restore identity UI passed');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
