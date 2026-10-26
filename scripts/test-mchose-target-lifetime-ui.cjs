// Production app navigation and async continuations; only IPC/File is simulated.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mchose-target-lifetime-ui');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true});const checks=[];try{
 const setup=async()=>{
  const page=await browser.newPage({viewport:{width:1500,height:1150}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
  await page.addInitScript(()=>{
   const peers=['A','B'].map(unit=>({id:'mchose-'+unit,profile_key:'session:mchose-'+unit,vendor_id:0x3837,product_id:'1014',model_ids:['4026'],name:'A7 V3 Ultra+ '+unit,product:'A7 V3 Ultra+',kind:'mouse',firmware_name:'fixture',connection:'wireless',protocol:'MCHOSE',features:[]}));
   const hw=id=>{const p=id==='mchose-A'?0:1,dpi=id==='mchose-A'?800:2200;return {profile:p,names:['板载 A','板载 B','板载 C'],dpi_x:Array(6).fill(dpi),dpi_y:Array(6).fill(dpi),count:6,active:0,independent:true,sleep:3,never_sleep:false,left_debounce:8,right_debounce:8,sensor:0,rotation:0,lod:2,buttons:[[0,1,0],[0,2,0],[0,4,0],[0,16,0],[0,8,0],[1,0,0,0]],adaptive:[0,0,0,0],firmware:'1',receiver_firmware:'1',raw_settings:[p,0x30,0x30,3,0,0,0,8,8,...Array(10).fill(0)]};};
   window.__hold=null;window.__deferred=null;
   const defer=(cmd,args,value)=>new Promise((resolve,reject)=>{window.__deferred={cmd,args,resolve:()=>resolve(value),reject:()=>reject(Error('STALE_A_FAILURE'))};});
   const old=window.__TAURI_INTERNALS__.invoke;
   window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
    if(!['scan_devices','list_devices','mchose_hardware','mchose_apply','mchose_macro','set_report_rate','plugin:dialog|open','plugin:dialog|save','mchose_export','mchose_restore','mchose_macro_file'].includes(cmd))return old(cmd,args);
    window.__qaCalls.push({cmd,args});
    if(cmd==='scan_devices')return{devices:peers,warnings:[]};if(cmd==='list_devices')return peers;
    let value=cmd==='mchose_hardware'||cmd==='mchose_apply'||cmd==='mchose_restore'?hw(args.id):cmd==='mchose_macro'?{slot:args.slot,name:'A old macro',mode:0,events:[]}:cmd.startsWith('plugin:dialog|')?'authored-fixture.json':null;
    if(window.__hold===cmd){window.__hold=null;return defer(cmd,args,value);}return value;
   };
   const fileText=File.prototype.text;File.prototype.text=function(){if(window.__hold==='file:text'){window.__hold=null;return defer('file:text',{},'{invalid macro JSON');}return fileText.call(this);};
  });
  await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
  const open=unit=>page.getByRole('button',{name:'Open A7 V3 Ultra+ '+unit,exact:true}).click();
  const leave=()=>page.getByRole('button',{name:'Back to devices',exact:true}).click();
  const ready=async(unit)=>{await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).waitFor();assert.equal(await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).inputValue(),unit==='A'?'800':'2200');};
  const hold=cmd=>page.evaluate(cmd=>window.__hold=cmd,cmd);
  const pending=()=>page.waitForFunction(()=>!!window.__deferred);
  const release=async reject=>{await page.evaluate(async reject=>{const d=window.__deferred;window.__deferred=null;reject?d.reject():d.resolve();await Promise.resolve();await Promise.resolve();},reject);await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));};
  const finish=async()=>{assert.deepEqual(errors,[]);assert.equal(await page.getByText('STALE_A_FAILURE',{exact:true}).count(),0);await page.close();};
  return{page,open,leave,ready,hold,pending,release,finish};
 };
 // An old hardware read must neither replace B nor show an error in B's view.
 for(const reject of [false,true]){
  const t=await setup();await t.hold('mchose_hardware');await t.open('A');await t.pending();await t.leave();await t.open('B');await t.ready('B');await t.release(reject);await t.ready('B');await t.finish();
 }checks.push('late old hardware success/error isolated from the new target');
 // Closing the target while a native file dialog is open cancels the deferred
 // operation, even if the user later chooses a file in the original dialog.
 for(const [tab,button,dialog,forbidden]of[
  ['板载/备份','从文件恢复','plugin:dialog|open','mchose_restore'],
  ['板载/备份','导出配置','plugin:dialog|save','mchose_export'],
  ['宏','导出宏','plugin:dialog|save','mchose_macro_file']]){
  const t=await setup();await t.open('A');await t.ready('A');await t.page.getByRole('button',{name:tab,exact:true}).click();
  await t.hold(dialog);await t.page.getByRole('button',{name:button,exact:true}).click();await t.pending();await t.leave();await t.open('B');await t.ready('B');await t.release(false);
  assert(!(await t.page.evaluate(()=>window.__qaCalls)).some(c=>c.cmd===forbidden));await t.finish();checks.push('closed target cancels '+forbidden+' after dialog');
 }
 // A write already sent stays bound to A; no old refresh or toast is published
 // after switching to B. The test never claims to cancel an in-flight HID write.
 for(const [cmd,tab,button,reject]of[
  ['set_report_rate','DPI','2000 Hz',false],['set_report_rate','DPI','2000 Hz',true],
  ['mchose_apply','DPI','保存六档 DPI',false],['mchose_macro','宏','读取鼠标',true]]){
  const t=await setup();await t.open('A');await t.ready('A');await t.page.getByRole('button',{name:tab,exact:true}).click();
  await t.hold(cmd);await t.page.getByRole('button',{name:button,exact:true}).click();await t.pending();await t.leave();await t.open('B');await t.ready('B');await t.release(reject);await t.ready('B');
  const calls=await t.page.evaluate(()=>window.__qaCalls);assert.equal(calls.filter(c=>c.cmd==='mchose_hardware'&&c.args.id==='mchose-A').length,1);
  assert(calls.filter(c=>c.cmd===cmd).every(c=>c.args.id==='mchose-A'));assert.equal(await t.page.getByText(/已写入并核对鼠标设置|已设为 2000 Hz/).count(),0);await t.finish();checks.push('late '+cmd+' continuation isolated');
 }
 {
  const t=await setup();await t.open('A');await t.ready('A');await t.page.getByRole('button',{name:'宏',exact:true}).click();await t.hold('file:text');
  await t.page.locator('input[type=file]').setInputFiles({name:'synthetic.json',mimeType:'application/json',buffer:Buffer.from('{}')});await t.pending();await t.leave();await t.open('B');await t.ready('B');await t.release(false);
  assert.equal(await t.page.getByText(/Unexpected token|JSON at position|JSON 格式/).count(),0);await t.finish();checks.push('late local macro file failure does not publish on B');
 }
 {
  const t=await setup();await t.open('A');await t.ready('A');await t.page.getByRole('button',{name:'宏',exact:true}).click();await t.page.getByRole('button',{name:'录制键盘宏',exact:true}).click();
  const input=t.page.getByRole('textbox',{name:'宏录制区域',exact:true});await input.focus();await t.page.keyboard.down('Shift');await t.page.keyboard.down('a');await t.page.getByRole('button',{name:'停止录制',exact:true}).click();await t.page.keyboard.up('a');await t.page.keyboard.up('Shift');
  const events=JSON.parse(await t.page.getByRole('textbox',{name:'宏事件编辑器',exact:true}).inputValue());for(const code of [225,4])assert.deepEqual(events.filter(e=>e.kind==='key'&&e.code===code).map(e=>e.down),[true,false]);
  await t.leave();await t.open('B');await t.ready('B');await t.page.getByRole('button',{name:'宏',exact:true}).click();assert.equal(await t.page.getByRole('textbox',{name:'宏事件编辑器',exact:true}).inputValue(),'[]');
  await t.page.screenshot({path:path.join(out,'fresh-target-macro.png')});await t.finish();checks.push('recording balances held keys on stop and target B has a fresh draft');
 }
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,mock_IPC:true,hardware_tested:false,full_compatibility:false,checks},null,2));console.log(`${checks.length} MCHOSE target lifetime UI cases passed; no physical I/O`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
