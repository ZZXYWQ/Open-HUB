// Built React UI receives actual Rust transaction receipts over a stateful
// memory mouse, including partial write and profile-store failures. No HID I/O.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert'),{spawnSync}=require('child_process');
const out=path.resolve('../outputs/100-model-development/profile-application-receipt-ui');fs.mkdirSync(out,{recursive:true});
function runTest(filter,env,log){const r=spawnSync('cargo',['test','--manifest-path','src-tauri/Cargo.toml','--locked',filter,'--','--ignored'],{windowsHide:true,encoding:'utf8',timeout:60000,env:{...process.env,...env}});fs.writeFileSync(path.join(out,log),(r.stdout??'')+(r.stderr??''));if(r.error)throw r.error;assert.equal(r.status,0,(r.stdout+r.stderr).slice(-3000));assert((r.stdout??'').includes('1 passed; 0 failed'));}
const fixture=path.join(out,'profile.json');runTest('export_full_mchose_profile_ui_fixture',{OPEN_HUB_MHUB_FULL_UI_FIXTURE:fixture},'fixture.log');const profile=JSON.parse(fs.readFileSync(fixture,'utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const calls=[];
async function native(request){const n=calls.length,requestPath=path.join(out,`request-${n}.json`),responsePath=path.join(out,`response-${n}.json`);fs.writeFileSync(requestPath,JSON.stringify(request));runTest('mhub_receipt_ui_bridge',{OPEN_HUB_MHUB_RECEIPT_REQUEST:requestPath,OPEN_HUB_MHUB_RECEIPT_RESPONSE:responsePath},`native-${n}.log`);const response=JSON.parse(fs.readFileSync(responsePath,'utf8'));calls.push({request,response});return response;}
(async()=>{const browser=await chromium.launch({headless:true});try{
const page=await browser.newPage({viewport:{width:1440,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));await page.exposeFunction('__nativeReceipt',native);
await page.addInitScript({content:`(()=>{${base.slice(start,end)};window.__emitDevices=()=>{for(const handler of events.get('devices-changed')||[])callbacks.get(handler)?.({event:'devices-changed',id:handler,payload:null});};window.__emitActivation=payload=>{for(const handler of events.get('profile-activated')||[])callbacks.get(handler)?.({event:'profile-activated',id:handler,payload});};})()`});
await page.addInitScript(({profile})=>{
 const peers=['A','B'].map(k=>({id:'mchose-'+k,name:'A7 V3 Ultra+ '+k,product:'MCHOSE A7 V3 Ultra+',product_id:'1014',vendor_id:0x3837,model_ids:['4026'],kind:'mouse',firmware_name:'A7 V3 Ultra+',connection:'wireless',protocol:'MCHOSE',features:[],profile_key:'session:mchose-'+k,connection_key:'connection:mchose-'+k+':1'}));
 const view={profiles:[profile],active_id:profile.id,auto_switch:false,profile_owners:{},device_profiles:{}};window.__receiptScenario='success';window.__receiptRequests=[];window.__changeConnection=()=>{peers[1].connection_key='connection:mchose-B:2';window.__emitDevices();};const old=window.__TAURI_INTERNALS__.invoke;
 window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(cmd==='get_profiles')return structuredClone(view);
  if(cmd==='scan_devices')return{devices:structuredClone(peers),warnings:[]};if(cmd==='list_devices')return structuredClone(peers);
  if(cmd==='activate_profile'){const request={profile,args:structuredClone(args),scenario:window.__receiptScenario};window.__receiptRequests.push(request);return window.__nativeReceipt(request);}
  return old(cmd,args);
 };
},{profile});
await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'配置',exact:true}).click();
const target=page.getByRole('combobox',{name:`配置 ${profile.name} 目标鼠标`,exact:true});await target.selectOption('mchose-A');const card=page.locator('[data-slot="card"]').filter({has:target}),apply=card.getByRole('button',{name:'应用配置',exact:true});
const result=page.getByRole('complementary',{name:'配置应用结果',exact:true});
async function expand(){await result.waitFor();if(!await result.locator('details').getAttribute('open').then(v=>v!==null))await result.locator('summary').click();}
async function scenario(name,n){await page.evaluate(v=>window.__receiptScenario=v,name);await apply.click();await page.waitForFunction(n=>window.__receiptRequests.length===n,n);}
await scenario('success',1);await result.getByText('写入前原始备份',{exact:true}).waitFor({state:'attached'});await expand();
assert.equal(await result.getByRole('table',{name:'已核对按键'}).locator('tbody tr').count(),6);assert.equal(await result.getByRole('table',{name:'已核对 DPI'}).locator('tbody tr').count(),profile.mchose.count);
await result.getByText('1000 / 2000 Hz',{exact:true}).waitFor();await result.getByText('宏槽 1',{exact:true}).waitFor();await result.getByLabel('已核对宏槽',{exact:true}).filter({hasText:'11 → 1'}).waitFor();
assert((await result.textContent()).includes(calls[0].response.mchose.backup));assert.deepEqual(calls[0].response.mchose.uploaded_macros,[0]);
await page.screenshot({path:path.join(out,'verified-result.png')});
await scenario('unchanged',2);await result.getByText('设置已一致，未重复写入，未新建备份。',{exact:true}).waitFor({state:'attached'});await expand();assert.equal(calls[1].response.mchose.backup,null);assert.deepEqual(calls[1].response.mchose.uploaded_macros,[]);
await scenario('failed',3);await result.getByRole('alert',{includeHidden:true}).filter({hasText:'可能已有部分设置生效'}).waitFor({state:'attached'});await expand();await result.getByRole('alert').filter({hasText:'原始备份'}).waitFor();assert.equal(await result.getByRole('table',{name:'已核对按键'}).count(),0);assert(!calls[2].response.mchose);
await scenario('save_failed',4);await result.getByRole('alert',{includeHidden:true}).filter({hasText:'独立配置保存失败'}).waitFor({state:'attached'});await expand();await result.getByText('写入前原始备份',{exact:true}).waitFor();assert(calls[3].response.mchose);await result.getByText('鼠标设置已核对，但配置记录保存未完成。',{exact:false}).waitFor();
const tray=await native({profile,args:{deviceId:'mchose-B',expectedConnection:'connection:mchose-B:1'},scenario:'success'});tray.reason='tray';await page.evaluate(v=>window.__emitActivation(v),tray);await result.locator('summary').filter({hasText:tray.mchose.device_name}).waitFor();await expand();
await page.evaluate(()=>window.__changeConnection());await result.getByText('设备连接已变化；以下保留的是应用时的核对记录。',{exact:true}).waitFor();assert.equal(await page.evaluate(()=>window.__receiptRequests.length),4);
await page.setViewportSize({width:420,height:900});const rect=await result.boundingBox();assert(rect.x>=0&&rect.x+rect.width<=420);await page.screenshot({path:path.join(out,'historical-result-small.png')});
await result.getByRole('button',{name:'关闭配置应用结果',exact:true}).click();assert.equal(await result.count(),0);
await page.evaluate(v=>window.__emitActivation(v),{id:'logitech-profile',name:'罗技配置',device_id:'logi-A',reason:'tray',errors:[]});await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));assert.equal(await result.count(),0);assert.deepEqual(errors,[]);
fs.writeFileSync(path.join(out,'calls.json'),JSON.stringify(calls,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,native_receipts:5,checks:['built UI activates full profile through native converter/target/transaction/store receipt bridge','DPI and six buttons reflect verified state','USB and wireless rates remain separate','logical macros display allocated physical slots','original backup path survives IPC','unchanged plan reports no write or new backup','partial write has error and backup but no success receipt','store failure retains hardware receipt and original independent selection','tray event is rendered for correct mouse without issuing apply','changed connection marks receipt historical','small viewport result stays within window','dismiss works','Logitech without receipt makes no MCHOSE verification claim','no browser exceptions']},null,2));console.log('14 profile-result UI checks passed with 5 native memory-transport receipts.');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
