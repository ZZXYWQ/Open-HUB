// Built React UI, production native DTOs, mocked IPC; no physical device.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync('../outputs/100-model-development/logitech-extended-light-fixture.json','utf8'));
const device=JSON.parse(fs.readFileSync('../outputs/100-model-development/logitech-binding-native-fixture.json','utf8')).device;
if(!device.features.some(f=>f.id===0x8070))device.features.push({id:0x8070,index:0x24,version:4,flags:0,name:'Color LED effects'});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});const writes=[];try{
 for(let i=0;i<2;i++){
 const page=await browser.newPage({viewport:{width:1450,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({data,device})=>{let view=data.view;const old=window.__TAURI_INTERNALS__.invoke;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
 if(!['scan_devices','list_devices','device_state','read_logitech_lighting','apply_logitech_lighting'].includes(cmd))return old(cmd,args);window.__qaCalls.push({cmd,args});
 if(cmd==='scan_devices')return{devices:[device],warnings:[]};if(cmd==='list_devices')return[device];
 if(cmd==='device_state')return{battery:null,dpi:null,report_rate:null,onboard_mode:'onboard',errors:[]};if(cmd==='read_logitech_lighting')return view;
 view=data.after;return{view,unchanged:false};
 };},{data:fixture[i],device});
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'Open '+device.name,exact:true}).click();
 await page.getByRole('navigation',{name:'Device features'}).getByRole('button',{name:'Device settings',exact:true}).click();await page.getByRole('button',{name:'打开鼠标灯光',exact:true}).click();
 const dialog=page.getByRole('dialog'),zone=dialog.getByRole('group',{name:'罗技灯区2',exact:true}),mode=zone.getByRole('combobox',{name:'罗技灯区2灯效',exact:true});await zone.waitFor();
 const kind=i===0?'ExtendedCycle':'ExtendedWave',other=i===0?'ExtendedWave':'ExtendedCycle';
 assert.equal(await mode.locator(`option[value=${other}]`).count(),0);
 const first=dialog.getByRole('group',{name:'罗技灯区1',exact:true});
 assert.equal(await first.locator('option[value=ExtendedCycle],option[value=ExtendedWave]').count(),0);
 await mode.selectOption(kind);
 const brightness=zone.getByRole('slider',{name:'罗技灯区2亮度',exact:true}),saturation=zone.getByRole('slider',{name:'罗技灯区2饱和度',exact:true});
 assert.equal(await brightness.inputValue(),'67');assert.equal(await saturation.inputValue(),'201');
 await brightness.press('Home');for(let n=0;n<36;n++)await brightness.press('ArrowRight');
 await saturation.press('Home');for(let n=0;n<92;n++)await saturation.press('ArrowRight');
 assert.equal((await page.evaluate(()=>window.__qaCalls)).filter(c=>c.cmd==='apply_logitech_lighting').length,0);
 await zone.getByRole('button',{name:'应用此灯区',exact:true}).click();await page.waitForFunction(()=>document.querySelector('select[aria-label="罗技灯区2灯效"]').value==='Keep');
 const calls=await page.evaluate(()=>window.__qaCalls),changes=calls.filter(c=>c.cmd==='apply_logitech_lighting').map(c=>c.args);assert.equal(changes.length,1);
 assert.deepEqual(changes[0].change.effect,{[kind]:{brightness_percent:37,saturation:92}});assert.deepEqual(changes[0].change.expected,fixture[i].view.zones[1].current);
 assert.equal(changes[0].change.zone,1);assert.equal(changes[0].id,device.id);writes.push(changes[0]);
 await mode.selectOption(kind);assert.equal(await brightness.inputValue(),'37');assert.equal(await saturation.inputValue(),'92');
 assert.deepEqual(errors,[]);assert(!calls.some(c=>['set_dpi','set_report_rate','set_onboard_mode','effects_start','multi_apply'].includes(c.cmd)));
 await page.close();
 }
 fs.writeFileSync('../outputs/100-model-development/logitech-extended-light-ui-changes.json',JSON.stringify(writes,null,2));
 console.log('Extended native lighting UI passed: active effect selection, native initial values, saturation/brightness, no implicit writes, independent zone');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
