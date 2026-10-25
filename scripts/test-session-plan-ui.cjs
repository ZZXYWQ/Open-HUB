// The input DTOs are emitted by the native three-real-driver round-trip test.
// Only the Tauri boundary is mocked here; this suite does not establish hardware support.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const root=path.resolve('dist'),out=path.resolve('../session-plan-ui-qa');fs.mkdirSync(out,{recursive:true});
const views=JSON.parse(fs.readFileSync(path.resolve('../native-session-views.json'),'utf8'));
assert.equal(views.length,3);for(const v of views)assert.equal(v.session.software_state,'running');
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{';
const start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(start>=marker.length&&end>start);
(async()=>{const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-gpu']});try{
 const page=await browser.newPage({viewport:{width:1450,height:1000},deviceScaleFactor:1}),errors=[],checks=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(nativeViews=>{
  const clone=v=>JSON.parse(JSON.stringify(v)),key='open-hub-session-qa';
  let state=JSON.parse(localStorage.getItem(key)||'null')||clone(nativeViews);const persist=()=>localStorage.setItem(key,JSON.stringify(state));persist();
  const original=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
   if(!['scan_devices','list_devices','multi_read','multi_apply','multi_input_sources','multi_stop_device_macros'].includes(cmd))return original(cmd,a);
   window.__qaCalls.push({cmd,args:clone(a)});
   const summaries=state.map(v=>({id:'multi:'+v.descriptor.id,vendor_id:v.descriptor.vendor_id,name:v.descriptor.name,product:v.descriptor.name,product_id:v.descriptor.product_id.toString(16).padStart(4,'0'),kind:'mouse',firmware_name:'',model_ids:[],connection:'USB',protocol:'Open HUB',features:[]}));
   if(cmd==='scan_devices')return {devices:summaries,warnings:[]};if(cmd==='list_devices')return summaries;
   if(cmd==='multi_input_sources')return state.map(v=>({source:v.session.plan.source,kind:v.descriptor.vendor_id===0x1b1c?'Vendor HID':'Keyboard report',serial:v.descriptor.serial??null}));
   const v=state.find(v=>'multi:'+v.descriptor.id===a.id);if(!v)throw Error('Unknown fixture device');
   if(cmd==='multi_read'){const r=clone(v);if(window.__qaBadSession)r.session.plan.vendor=0x046d;return r;}
   if(cmd==='multi_apply'){
    if(a.plan.vendor!==v.descriptor.vendor_id||a.plan.product!==v.descriptor.product_id)throw Error('Cross-model apply');
    v.session.plan=clone(a.plan);v.session.software_state='running';
    for(const s of a.plan.settings){if(s.Dpi)v.state.dpi=clone(s.Dpi);if(s.ReportRate)v.state.report_rate=s.ReportRate;}
    persist();return clone(v);
   }
   if(cmd==='multi_stop_device_macros'){v.session.software_state='stopped';persist();return;}
  };
 },views);
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');
 const open=async v=>{await page.getByRole('button',{name:new RegExp(v.descriptor.name.replace(/[.*+?^${}()|[\]\\]/g,'\\$&'))}).first().click();await page.getByRole('textbox',{name:'软件宏名称',exact:true}).waitFor();};
 for(const v of views){
  const expected=v.session.plan,button=expected.settings.find(s=>s.Button).Button.id;
  await open(v);
  assert.equal(await page.getByRole('textbox',{name:'软件宏名称',exact:true}).inputValue(),expected.macros[0].name);
  assert.equal(await page.getByRole('combobox',{name:`鼠标按键${button}动作`,exact:true}).inputValue(),'Macro');
  assert.equal(await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).inputValue(),expected.source.path);
  assert.equal(await page.getByRole('combobox',{name:'软件宏播放方式',exact:true}).inputValue(),expected.macros[0].mode);
  checks.push(v.descriptor.name+': native full plan displays desired macro instead of native F13');
  await page.getByRole('spinbutton',{name:'鼠标档位1X',exact:true}).fill('2400');
  if(v.capabilities.dpi.separate_xy)await page.getByRole('spinbutton',{name:'鼠标档位1Y',exact:true}).fill('2400');
  await page.getByRole('button',{name:'应用到此鼠标',exact:true}).click();await page.getByRole('status').filter({hasText:'设置已发送'}).waitFor();
  let calls=await page.evaluate(()=>window.__qaCalls),applied=calls.filter(c=>c.cmd==='multi_apply').at(-1);
  assert.equal(applied.args.id,'multi:'+v.descriptor.id);assert.deepEqual(applied.args.plan.macros,expected.macros);assert.deepEqual(applied.args.plan.source,expected.source);
  assert.deepEqual(applied.args.plan.settings.find(s=>s.Button),expected.settings.find(s=>s.Button));
  checks.push(v.descriptor.name+': DPI edit/apply preserves complete macro, binding and exact source');
  await page.reload();await open(v);assert.equal(await page.getByRole('spinbutton',{name:'鼠标档位1X',exact:true}).inputValue(),'2400');
  assert.equal(await page.getByRole('textbox',{name:'软件宏名称',exact:true}).inputValue(),expected.macros[0].name);
  await page.getByRole('button',{name:'保存完整方案到配置中心',exact:true}).click();
  await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='save_profile'));
  calls=await page.evaluate(()=>window.__qaCalls);const saveArgs=calls.filter(c=>c.cmd==='save_profile').at(-1).args;assert.equal(saveArgs.deviceId,'multi:'+v.descriptor.id);assert.equal(saveArgs.makeActive,false);const saved=saveArgs.profile;
  assert(saved.multi);assert.equal(saved.multi.vendor,expected.vendor);assert.equal(saved.multi.product,expected.product);assert.deepEqual(saved.multi.macros,expected.macros);assert.deepEqual(saved.multi.source,expected.source);assert.equal(saved.multi.settings.find(s=>s.Dpi).Dpi.stages[0].x,2400);
  checks.push(v.descriptor.name+': fresh mount saves full private draft to exact mouse without activation');
  await page.reload();await open(v);
  const before=await page.evaluate(()=>JSON.parse(localStorage.getItem('open-hub-session-qa')));
  await page.getByRole('button',{name:'停止此鼠标的软件动作并恢复临时映射',exact:true}).click();
  await page.getByRole('status').filter({hasText:'已停止，重新应用后才能触发'}).waitFor();
  const after=await page.evaluate(()=>JSON.parse(localStorage.getItem('open-hub-session-qa')));
  for(let i=0;i<after.length;i++)assert.equal(after[i].session.software_state,after[i].descriptor.id===v.descriptor.id?'stopped':before[i].session.software_state);
  assert.equal(await page.getByRole('combobox',{name:`鼠标按键${button}动作`,exact:true}).inputValue(),'Macro');
  assert.equal(await page.getByRole('textbox',{name:'软件宏名称',exact:true}).inputValue(),expected.macros[0].name);
  calls=await page.evaluate(()=>window.__qaCalls);assert.deepEqual(calls.filter(c=>c.cmd==='multi_stop_device_macros').at(-1).args,{id:'multi:'+v.descriptor.id});assert(!calls.some(c=>c.cmd==='multi_stop_macros'));
  checks.push(v.descriptor.name+': stop affects only selected mouse and retains editable macro');
  await page.getByRole('button',{name:v.descriptor.vendor_id===0x1b1c?'刷新会话状态':'重新读取',exact:true}).click();await page.getByRole('textbox',{name:'软件宏名称',exact:true}).waitFor();
  assert.equal(await page.getByRole('textbox',{name:'软件宏名称',exact:true}).inputValue(),expected.macros[0].name);
  checks.push(v.descriptor.name+': refresh retains stopped desired plan');
  await page.screenshot({path:path.join(out,v.descriptor.vendor_id.toString(16)+'-full-plan.png'),fullPage:true});
  await page.reload();
 }
 await open(views[0]);await page.evaluate(()=>{window.__qaBadSession=true;});await page.getByRole('button',{name:'重新读取',exact:true}).click();
 await page.getByRole('alert').filter({hasText:'会话方案与鼠标型号不一致'}).waitFor();assert.equal(await page.getByRole('button',{name:'应用到此鼠标',exact:true}).count(),0);assert.equal(await page.getByRole('button',{name:'保存完整方案到配置中心',exact:true}).count(),0);
 await page.evaluate(()=>{window.__qaBadSession=false;});await page.getByRole('button',{name:'重试读取设备方案',exact:true}).click();await page.getByRole('textbox',{name:'软件宏名称',exact:true}).waitFor();checks.push('cross-model session clears stale form and explicit retry restores correct plan');
 assert.deepEqual(errors,[]);checks.push('no browser exceptions');
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({result:'passed',mock_backend:true,native_view_fixtures:true,checks},null,2));console.log(checks.length+' session-plan UI checks passed (native DTOs, mock Tauri)');
 }finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
