// Real native-generated views; UI IPC is mocked, saved requests replay in Rust.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_LOGITECH_WAVEFORM_FIXTURE||'../outputs/100-model-development/logitech-waveform-native-fixture.json','utf8'));
const device=JSON.parse(fs.readFileSync('../outputs/100-model-development/logitech-binding-native-fixture.json','utf8')).device;
if(!device.features.some(f=>f.id===0x8070))device.features.push({id:0x8070,index:0x24,version:4,flags:0,name:'Color LED effects'});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({fixture,device})=>{let view=fixture.view;const old=window.__TAURI_INTERNALS__.invoke;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(!['scan_devices','list_devices','device_state','read_logitech_lighting','apply_logitech_lighting'].includes(cmd))return old(cmd,args);window.__qaCalls.push({cmd,args});
  if(cmd==='scan_devices')return{devices:[device],warnings:[]};if(cmd==='list_devices')return[device];
  if(cmd==='device_state')return{battery:null,dpi:null,report_rate:null,onboard_mode:'onboard',errors:[]};if(cmd==='read_logitech_lighting')return view;
  view=args.change.effect.BreathingWaveform?fixture.waveform_views[args.change.effect.BreathingWaveform.waveform]:fixture.legacy_view;return{view,unchanged:false};
 };},{fixture,device});
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'Open '+device.name,exact:true}).click();
 await page.getByRole('navigation',{name:'Device features'}).getByRole('button',{name:'Device settings',exact:true}).click();await page.getByRole('button',{name:'打开鼠标灯光',exact:true}).click();
 const dialog=page.getByRole('dialog'),zone=dialog.getByRole('group',{name:'罗技灯区2',exact:true}),mode=zone.getByRole('combobox',{name:'罗技灯区2灯效',exact:true});await zone.waitFor();
 await mode.selectOption('Breathing');const shape=zone.getByRole('combobox',{name:'罗技灯区2呼吸波形',exact:true});
 assert.equal(await shape.inputValue(),'keep');assert.deepEqual(await shape.locator('option').evaluateAll(o=>o.map(v=>v.value)),['keep','0','1','2','3','4','5','6']);
 assert((await shape.locator('option[value=keep]').textContent()).includes('未识别'));
 await zone.getByLabel('罗技灯区2颜色',{exact:true}).fill('#abcdef');const brightness=zone.getByRole('slider',{name:'罗技灯区2亮度',exact:true});await brightness.press('Home');for(let i=0;i<30;i++)await brightness.press('ArrowRight');
 const apply=async()=>{await zone.getByRole('button',{name:'应用此灯区',exact:true}).click();await page.waitForFunction(()=>document.querySelector('select[aria-label="罗技灯区2灯效"]').value==='Keep');};
 assert.equal((await page.evaluate(()=>window.__qaCalls)).filter(c=>c.cmd==='apply_logitech_lighting').length,0);await apply();
 for(let waveform=0;waveform<=6;waveform++){await mode.selectOption('Breathing');await shape.selectOption(String(waveform));await apply();}
 const calls=await page.evaluate(()=>window.__qaCalls),writes=calls.filter(c=>c.cmd==='apply_logitech_lighting').map(c=>c.args);assert.equal(writes.length,8);
 assert.deepEqual(writes[0].change.effect,{Breathing:{color:[171,205,239],brightness_percent:31}});assert.deepEqual(writes[0].change.expected,fixture.view.zones[1].current);
 for(let i=0;i<7;i++){
  assert.deepEqual(writes[i+1].change.effect,{BreathingWaveform:{color:[171,205,239],brightness_percent:31,waveform:i}});
  assert.deepEqual(writes[i+1].change.expected,(i===0?fixture.legacy_view:fixture.waveform_views[i-1]).zones[1].current);
 }
 for(const request of writes){assert.equal(request.id,device.id);assert.equal(request.change.zone,1);assert.equal(request.change.expected_device,fixture.view.device_key);assert.equal(request.change.expected_connection,fixture.view.connection_key);}
 await mode.selectOption('Breathing');await shape.selectOption('keep');assert((await shape.locator('option[value=keep]').textContent()).includes('指数'));
 assert(!calls.some(c=>['set_dpi','set_report_rate','set_onboard_mode','effects_start','multi_apply'].includes(c.cmd)));assert.deepEqual(errors,[]);
 const out=path.resolve('../outputs/100-model-development/logitech-waveform-ui');fs.mkdirSync(out,{recursive:true});await page.screenshot({path:path.join(out,'native-breath-waveforms.png')});
 fs.writeFileSync('../outputs/100-model-development/logitech-waveform-ui-changes.json',JSON.stringify(writes,null,2));
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_DTOs:true,mock_IPC:true,hardware_tested:false,full_compatibility:false,requests:8,checks:['seven explicit native forms and opaque keep choice','legacy JSON action preserves unknown native waveform','only explicit waveform edit uses new additive action','color and brightness survive shape changes','fresh connection/zone baseline per write','no implicit edits or other device feature writes','actual requests retained for native transaction replay']},null,2));
 console.log('Native Logitech breathing waveform UI passed: legacy keep plus seven explicit native forms');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
