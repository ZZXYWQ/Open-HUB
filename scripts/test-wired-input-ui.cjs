// The views originate in two serialless production SteelSeries drivers over memory transports.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const fixture=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_WIRED_INPUT_FIXTURE||'../outputs/100-model-development/wired-input-native-views.json','utf8'));
const {views,sources}=fixture;assert.equal(views.length,2);assert(views.every(v=>v.descriptor.serial==null&&!v.descriptor.simulated));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1050}}),errors=[],checks=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({views,sources})=>{
  const old=window.__TAURI_INTERNALS__.invoke;window.__qaPending=[];
  window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
   if(!['scan_devices','list_devices','multi_read','multi_input_sources','multi_simulate'].includes(cmd))return old(cmd,a);
   window.__qaCalls.push({cmd,args:JSON.parse(JSON.stringify(a))});
   const summaries=views.map(v=>({id:'multi:'+v.descriptor.id,vendor_id:v.descriptor.vendor_id,name:v.descriptor.name,product:v.descriptor.name,product_id:v.descriptor.product_id.toString(16),kind:'mouse',firmware_name:'',model_ids:[],connection:'USB',protocol:'Open HUB',features:[]}));
   if(cmd==='scan_devices')return {devices:summaries,warnings:[]};if(cmd==='list_devices')return summaries;
   if(cmd==='multi_read')return views.find(v=>'multi:'+v.descriptor.id===a.id);
   if(cmd==='multi_input_sources')return new Promise(resolve=>window.__qaPending.push({id:a.id,resolve}));
   if(cmd==='multi_simulate')return {packet_count:8};
  };
 },fixture);
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 let requests=0;
 const open=async n=>{await page.getByRole('button',{name:new RegExp(views[0].descriptor.name)}).nth(n).click();await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).waitFor();await page.waitForFunction(n=>window.__qaPending.length>=n,++requests);};
 const resolve=async(index,unit)=>page.evaluate(({index,source})=>{window.__qaPending[index].resolve([source]);}, {index,source:sources[unit]});
 const save=async unit=>{const before=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').length);await page.getByRole('button',{name:'保存完整方案到配置中心',exact:true}).click();await page.waitForFunction(n=>window.__qaCalls.filter(c=>c.cmd==='save_profile').length>n,before);const args=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args);assert.equal(args.deviceId,'multi:'+views[unit].descriptor.id);return args.profile;};
 const home=async()=>{const back=page.getByRole('button',{name:'Back to devices',exact:true});if(await back.count())await back.click();else await page.getByRole('button',{name:'设备',exact:true}).click();};
 const profiles=[];
 await open(0);assert.equal(await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).inputValue(),'');
 await page.getByRole('button',{name:'新增软件宏',exact:true}).click();await page.getByRole('textbox',{name:'软件宏名称',exact:true}).fill('A 独立宏');await page.getByRole('combobox',{name:'软件宏事件1键码',exact:true}).selectOption('6');await page.getByRole('combobox',{name:'软件宏事件3键码',exact:true}).selectOption('6');await page.getByRole('combobox',{name:'鼠标按键4动作',exact:true}).selectOption('Macro');
 await resolve(0,0);await page.waitForFunction(p=>document.querySelector('[aria-label="软件宏输入来源"]').value===p,sources[0].source.path);assert.equal(await page.getByRole('textbox',{name:'软件宏名称',exact:true}).inputValue(),'A 独立宏');assert(await page.getByText(/控制接口与键盘接口属于同一只物理鼠标/).isVisible());assert.equal(await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).locator('option:checked').textContent(),'此鼠标的键盘输入');
 profiles.push(await save(0));assert.deepEqual(profiles[0].multi.source,sources[0].source);assert.equal(profiles[0].multi.macros[0].name,'A 独立宏');assert.equal(profiles[0].multi.macros[0].steps[0].KeyDown,6);checks.push('late exact-target query auto-selects one native verified collection without losing macro edits');
 // The App header returns to the device list and unmounts the old input selector.
 await home();await open(1);await page.getByRole('button',{name:'新增软件宏',exact:true}).click();await page.getByRole('textbox',{name:'软件宏名称',exact:true}).fill('B 独立宏');await page.getByRole('combobox',{name:'软件宏事件1键码',exact:true}).selectOption('7');await page.getByRole('combobox',{name:'软件宏事件3键码',exact:true}).selectOption('7');await page.getByRole('combobox',{name:'鼠标按键4动作',exact:true}).selectOption('Macro');await resolve(1,1);await page.waitForFunction(p=>document.querySelector('[aria-label="软件宏输入来源"]').value===p,sources[1].source.path);profiles.push(await save(1));assert.deepEqual(profiles[1].multi.source,sources[1].source);assert.equal(profiles[1].multi.macros[0].name,'B 独立宏');checks.push('same-model second mouse saves its own collection and full editable macro');
 // A response from an unmounted mouse may never select a source on the new one.
 await home();await open(0);await home();await open(1);await resolve(2,0);assert.equal(await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).inputValue(),'');await resolve(3,1);await page.waitForFunction(p=>document.querySelector('[aria-label="软件宏输入来源"]').value===p,sources[1].source.path);checks.push('source query from unmounted mouse cannot overwrite newly selected mouse');
 const calls=await page.evaluate(()=>window.__qaCalls);assert(calls.filter(c=>c.cmd==='multi_input_sources').every(c=>views.some(v=>v.descriptor.id===c.args.id)));assert(!calls.some(c=>['multi_apply','activate_profile'].includes(c.cmd)));assert.deepEqual(errors,[]);
 const out=path.resolve('../outputs/100-model-development/wired-input-ui');fs.mkdirSync(out,{recursive:true});fs.writeFileSync('../outputs/100-model-development/wired-input-ui-profiles.json',JSON.stringify(profiles,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_dtos:true,hardware_tested:false,checks},null,2));console.log('Wired serialless per-mouse UI: target-scoped queries, async edits, source selection and independent saves passed');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
