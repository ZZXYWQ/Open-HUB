const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_LOGITECH_BINDING_FIXTURE||'../outputs/100-model-development/logitech-binding-native-fixture.json','utf8'));
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/logitech-restore-preview-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
const page=await browser.newPage({viewport:{width:1450,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
await page.addInitScript(f=>{
 let current=f.snapshot;window.__badBackup=false;window.__changedBackup=true;let picks=0;const old=window.__TAURI_INTERNALS__.invoke;
 window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
  if(!['scan_devices','list_devices','device_state','onboard_snapshot','preview_onboard_restore','restore_onboard_backup','plugin:dialog|open'].includes(cmd))return old(cmd,a);
  window.__qaCalls.push({cmd,args:a});
  if(cmd==='scan_devices')return {devices:[f.device],warnings:[]};if(cmd==='list_devices')return[f.device];
  if(cmd==='device_state')return {battery:null,dpi:null,report_rate:null,onboard_mode:'onboard',errors:[]};if(cmd==='onboard_snapshot')return current;
  if(cmd==='plugin:dialog|open')return `reviewed-backup-${++picks}.json`;
  if(cmd==='preview_onboard_restore'){if(window.__badBackup)throw Error('backup belongs to a different or unknown mouse model');return f.restore_preview;}
  if(cmd==='restore_onboard_backup'){if(window.__changedBackup)throw Error('backup file changed since preview; select it again before restoring');return {sectors_written:3,backup_path:'memory-before-restore.json',snapshot:null,refresh_note:'Restore was read-back verified, but refreshing the view failed: simulated disconnect'};}
 };
},fixture);
await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'Open G502 HERO',exact:true}).click();await page.getByRole('navigation',{name:'Device features',exact:true}).getByRole('button',{name:'Device settings',exact:true}).click();
await page.getByRole('button',{name:'Read from mouse',exact:true}).click();
const launch=page.getByRole('button',{name:'Restore from backup…',exact:true}),dialog=page.getByRole('dialog');
await launch.click();await dialog.getByRole('region',{name:'板载恢复内容预览',exact:true}).waitFor().catch(async()=>{await dialog.locator('[aria-label="板载恢复内容预览"]').waitFor();});
assert(await dialog.getByText(/备份桌面/).isVisible());assert(await dialog.getByText(/备份游戏/).isVisible());assert(await dialog.getByText(/宏内容继续使用鼠标当前存储/).isVisible());assert(await dialog.getByText(/800 \/ 1600 \/ 3200 DPI · 500 Hz/).isVisible());assert(await dialog.getByText(/未启用/).isVisible());await dialog.getByRole('button',{name:'Cancel',exact:true}).click();
await page.evaluate(()=>window.__badBackup=true);await launch.click();await dialog.getByRole('alert').waitFor();assert(await dialog.getByRole('button',{name:'Restore',exact:true}).isDisabled());assert.equal(await dialog.locator('[aria-label="板载恢复内容预览"]').count(),0);assert.equal((await page.evaluate(()=>window.__qaCalls)).filter(x=>x.cmd==='restore_onboard_backup').length,0);
await page.evaluate(()=>window.__badBackup=false);await dialog.getByRole('button',{name:'重新选择备份',exact:true}).click();await dialog.getByRole('button',{name:'Restore',exact:true}).click();await dialog.getByRole('alert').filter({hasText:'backup file changed since preview'}).waitFor();assert(await dialog.getByRole('button',{name:'Restore',exact:true}).isDisabled());assert.equal(await dialog.locator('[aria-label="板载恢复内容预览"]').count(),0);
await page.evaluate(()=>window.__changedBackup=false);await dialog.getByRole('button',{name:'重新选择备份',exact:true}).click();await page.screenshot({path:path.join(out,'restore-preview.png')});await dialog.getByRole('button',{name:'Restore',exact:true}).click();await dialog.waitFor({state:'hidden'});await page.getByText('板载配置已恢复并校验',{exact:true}).waitFor();assert(await launch.isDisabled());
const calls=await page.evaluate(()=>window.__qaCalls),writes=calls.filter(x=>x.cmd==='restore_onboard_backup');assert.equal(writes.length,2);const request=writes.at(-1).args;assert.deepEqual(request,{id:fixture.device.id,path:'reviewed-backup-4.json',expectedDevice:fixture.snapshot.device_key,expectedConnection:fixture.snapshot.connection_key,expectedBackupDigest:fixture.restore_backup_digest});assert(!calls.some(x=>['set_dpi','set_report_rate','multi_apply','activate_profile','write_onboard_slot'].includes(x.cmd)));assert.deepEqual(errors,[]);
fs.writeFileSync('../outputs/100-model-development/logitech-ui-restore-request.json',JSON.stringify(request,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,checks:['preview renders raw decoded Chinese slot names, DPI, rate and enable state','macro-reference scope is shown before restore','foreign backup clears old preview and disables restore','changed file requires fresh preview','restore pins exact byte digest, device and connection','verified save with failed view refresh is not reported as failed write','no global profile or live settings commands']},null,2));console.log('Logitech reviewed backup restore UI passed');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
