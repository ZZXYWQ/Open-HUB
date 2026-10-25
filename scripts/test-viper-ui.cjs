const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const root=path.resolve('dist'),out=path.resolve('../viper-ui-qa');fs.mkdirSync(out,{recursive:true});
const views=JSON.parse(fs.readFileSync(path.resolve(process.env.OPEN_HUB_VIPER_FIXTURE||'../outputs/0.3.5-native-viper-views.json'),'utf8'));assert.equal(views.length,6);
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{';const start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(start>=marker.length&&end>start);
(async()=>{const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-gpu']});try{
 const page=await browser.newPage({viewport:{width:1450,height:1000},deviceScaleFactor:1}),errors=[],checks=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(views=>{const original=window.__TAURI_INTERNALS__.invoke;window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
  if(cmd==='multi_templates'){window.__qaCalls.push({cmd,args:a});return views;}
  if(cmd==='multi_simulate'){window.__qaCalls.push({cmd,args:a});return {packet_count:7,simulated:true,hardware_tested:false};}
  return original(cmd,a);
 };},views);
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'配置',exact:true}).click();
 for(const view of views){
  const pid=view.descriptor.product_id,ultimate=[0x007a,0x007b].includes(pid),v3=[0x00c0,0x00c1].includes(pid);
  await page.getByRole('button',{name:'按品牌新建配置',exact:true}).click();const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:view.descriptor.name,exact:true}).click();
  assert.equal(await dialog.getByRole('spinbutton',{name:'鼠标档位1X',exact:true}).getAttribute('step'),'1');
  await dialog.getByRole('spinbutton',{name:'鼠标档位1X',exact:true}).fill('2351');await dialog.getByRole('combobox',{name:'鼠标回报率',exact:true}).selectOption(pid===0x00c1?'8000':'500');
  if(pid!==0x00c1)assert.equal(await dialog.getByRole('combobox',{name:'鼠标回报率',exact:true}).locator('option[value="8000"]').count(),0);
  checks.push(view.descriptor.name+': exact native model, DPI step and rate limits');
  const lighting=dialog.getByRole('region',{name:'原生灯效',exact:true});
  if(ultimate){
   const effects=lighting.getByRole('combobox',{name:'原生灯区4效果',exact:true});assert.deepEqual(await effects.locator('option').evaluateAll(options=>options.filter(o=>o.value).map(o=>o.value)),['Off','Static','Breathing','ColorCycle','Reactive']);
   await effects.selectOption('Breathing');await lighting.getByLabel('鼠标灯区4颜色',{exact:true}).fill('#d94bff');const brightness=lighting.getByRole('slider',{name:'原生灯区4亮度',exact:true});await brightness.press('Home');await brightness.press('ArrowRight');
   assert.equal(await brightness.inputValue(),'1');await lighting.getByText('此协议的呼吸与循环速度由固件决定。',{exact:true}).waitFor();
   checks.push(view.descriptor.name+': all five native effects, explicit colour and brightness');
  }else{assert.equal(await lighting.getByRole('combobox').count(),0);await lighting.getByText(/此型号未提供可控制的 RGB 灯区/).waitFor();checks.push(view.descriptor.name+': no fabricated RGB controls');}
  if(v3){const actions=dialog.getByRole('combobox',{name:'鼠标按键4动作',exact:true});assert.equal(await actions.locator('option[value="Key"],option[value="Macro"],option[value="Dpi"]').count(),0);await actions.selectOption('Disabled');assert.equal(await dialog.getByRole('combobox',{name:'鼠标按键1动作',exact:true}).count(),0);for(const id of [9,10,96]){const control=dialog.getByRole('combobox',{name:`鼠标按键${id}动作`,exact:true});assert.deepEqual(await control.locator('option').evaluateAll(o=>o.map(x=>x.value)),['Default','Disabled']);await control.selectOption('Disabled');}checks.push(view.descriptor.name+': captured native actions, three independent scroll/DPI toggles and protected left click');}
  assert.equal(await dialog.getByRole('textbox',{name:'软件宏名称',exact:true}).count(),0);
  await dialog.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await dialog.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();
  await page.screenshot({path:path.join(out,pid.toString(16)+'-editor.png'),fullPage:true});await dialog.getByRole('button',{name:'保存设备方案',exact:true}).click();await dialog.waitFor({state:'hidden'});
  const saved=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile);assert.equal(saved.target_vendor,0x1532);assert.equal(saved.target_model,pid);assert.equal(saved.multi.product,pid);assert.equal(saved.multi.settings.find(s=>s.Dpi).Dpi.stages[0].x,2351);assert.equal(saved.multi.settings.find(s=>s.ReportRate).ReportRate,pid===0x00c1?8000:500);
  if(ultimate)assert.deepEqual(saved.multi.settings.find(s=>s.Lighting).Lighting,{zone:4,state:{effect:{Breathing:{color:{r:217,g:75,b:255},period_ms:4000}},brightness:1}});
  if(v3)assert.deepEqual(saved.multi.settings.find(s=>s.Button).Button,{id:4,action:'Disabled'});
  checks.push(view.descriptor.name+': full typed plan saved to its own VID/PID after simulation');
 }
 const calls=await page.evaluate(()=>window.__qaCalls);assert.equal(calls.filter(c=>c.cmd==='multi_simulate').length,6);assert(!calls.some(c=>['multi_apply','activate_profile','sino_apply'].includes(c.cmd)));assert.deepEqual(errors,[]);checks.push('offline editor never writes USB; no browser exceptions');
 fs.writeFileSync(path.resolve('../outputs/0.3.5-viper-ui-created-profiles.json'),JSON.stringify(calls.filter(c=>c.cmd==='save_profile').map(c=>c.args.profile),null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({result:'passed',native_view_fixtures:true,mock_backend:true,checks},null,2));console.log(checks.length+' Viper UI checks passed (native DTOs, mock Tauri)');
 }finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
