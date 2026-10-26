// Production React views with native DTOs; saved plans are replayed by Rust.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const allViews=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_STAGES_FIXTURE||'../outputs/100-model-development/dav3pro-stages-native-views.json','utf8'));
const views=allViews.filter(v=>v.capabilities.calibration);assert(views.length>0);
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/dav3pro-calibration-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1100}}),errors=[],profiles=[],checks=[];
 page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(views=>{const old=window.__TAURI_INTERNALS__.invoke;window.__qaTemplates=views;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{if(cmd==='multi_templates'){window.__qaCalls.push({cmd,args});return window.__qaTemplates;}if(cmd==='multi_simulate'){window.__qaCalls.push({cmd,args});return {packet_count:20,simulated:true,hardware_tested:false,software_macros:[]};}return old(cmd,args);};},allViews);
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'配置',exact:true}).click();
 const open=async view=>{await page.getByRole('button',{name:'按品牌新建配置',exact:true}).click();const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:view.descriptor.name,exact:true}).click();return dialog;};
 for(const view of views){
  assert.deepEqual(view.state.calibration,{Standard:{distance:2}});
  for(const kind of ['Standard','Asymmetric']){
   const dialog=await open(view),region=dialog.getByRole('region',{name:'传感器校准',exact:true});
   const mode=region.getByRole('combobox',{name:'校准模式',exact:true});assert.equal(await mode.inputValue(),'Standard');
   const tracking=region.getByRole('combobox',{name:'追踪等级',exact:true});
   assert.equal(await tracking.inputValue(),'2');assert.deepEqual(await tracking.locator('option').evaluateAll(o=>o.map(x=>Number(x.value))),[1,2,3]);
   if(kind==='Standard'){await tracking.selectOption('3');}
   else{
    await mode.selectOption('Asymmetric');const lift=region.getByRole('combobox',{name:'抬起等级',exact:true}),landing=region.getByRole('combobox',{name:'落下等级',exact:true});
    assert.equal(await lift.inputValue(),'13');assert.equal(await landing.inputValue(),'12');
    assert.deepEqual(await lift.locator('option').evaluateAll(o=>o.map(x=>Number(x.value))),view.capabilities.calibration.lift_levels);
    assert.deepEqual(await landing.locator('option').evaluateAll(o=>o.map(x=>Number(x.value))),view.capabilities.calibration.landing_levels);
    await lift.selectOption('2');assert.equal(await landing.inputValue(),'1');
    await landing.selectOption('25');assert.equal(await lift.inputValue(),'26');
    await mode.selectOption('Standard');assert.equal(await tracking.inputValue(),'2');assert.equal(await lift.count(),0);
    await mode.selectOption('Asymmetric');await landing.selectOption('25');assert.equal(await lift.inputValue(),'26');
   }
   assert.equal(await region.getByText(/mm|毫米/).count(),0);
   assert.equal(await dialog.locator('select[aria-label^="原生灯区"]').count(),0);
   await dialog.getByRole('textbox',{name:'鼠标配置名称',exact:true}).fill(view.descriptor.name+' · 校准 '+kind);
   await dialog.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await dialog.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();
   await region.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(out,view.descriptor.product_id.toString(16)+'-'+kind+'.png')});
   await dialog.getByRole('button',{name:'保存设备方案',exact:true}).click();await dialog.waitFor({state:'hidden'});
   const profile=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile),plan=profile.multi;
   assert.deepEqual(plan.settings.filter(s=>s.SensorCalibration),[{SensorCalibration:kind==='Standard'?{Standard:{distance:3}}:{Asymmetric:{lift:26,landing:25}}}]);
   assert.deepEqual(plan.settings.find(s=>s.Dpi).Dpi,view.state.dpi);
   assert.deepEqual(Object.fromEntries(plan.settings.filter(s=>s.Button).map(s=>[s.Button.id,s.Button.action])),view.state.buttons);
   assert.equal(plan.settings.filter(s=>s.Lighting).length,0);assert.equal(plan.macros.length,0);profiles.push(profile);checks.push(view.descriptor.product_id.toString(16)+' '+kind+' bounds, linked thresholds, saved native plan');
  }
 }
 // Unknown or absent calibration must not acquire defaults during ordinary save.
 const unknown=JSON.parse(JSON.stringify(views[0]));delete unknown.capabilities.calibration;delete unknown.state.calibration;unknown.descriptor.name+=' unreadable calibration';
 await page.evaluate(view=>window.__qaTemplates=[view],unknown);let dialog=await open(unknown);assert.equal(await dialog.getByRole('region',{name:'传感器校准',exact:true}).count(),0);
 await dialog.getByRole('button',{name:'保存设备方案',exact:true}).click();await dialog.waitFor({state:'hidden'});
 assert(!(await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile.multi.settings)).some(s=>s.SensorCalibration));checks.push('unreadable calibration adds no guessed defaults');
 assert(!(await page.evaluate(()=>window.__qaCalls)).some(c=>c.cmd==='multi_input_sources'));
 // Injected foreign identity with borrowed DTO capabilities cannot expose this model UI.
 for(const [vendor,product]of[[0x046d,0xb6],[0x1532,0xb8]]){
  const foreign=JSON.parse(JSON.stringify(views[0]));foreign.descriptor={...foreign.descriptor,vendor_id:vendor,product_id:product,name:`Foreign calibration ${vendor}:${product}`};
  await page.evaluate(view=>window.__qaTemplates=[view],foreign);dialog=await open(foreign);assert.equal(await dialog.getByRole('region',{name:'传感器校准',exact:true}).count(),0);await dialog.getByRole('button',{name:'保存设备方案',exact:true}).click();await dialog.waitFor({state:'hidden'});assert(!(await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile.multi.settings)).some(s=>s.SensorCalibration));
 }checks.push('two foreign identities neither expose nor silently save DA calibration');
 const calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>['multi_apply','activate_profile','multi_save_hardware'].includes(c.cmd)),JSON.stringify(calls.filter(c=>['multi_apply','activate_profile','multi_save_hardware'].includes(c.cmd))));assert.deepEqual(errors,[]);
 fs.writeFileSync('../outputs/100-model-development/dav3pro-calibration-ui-profiles.json',JSON.stringify(profiles,null,2));
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_DTOs:true,hardware_tested:false,distinct_models:1,full_compatibility:false,transports:views.map(v=>v.descriptor.product_id),profiles:profiles.length,checks},null,2));
 console.log(`${profiles.length} calibration UI plans saved for production codec replay; unknown and foreign identities isolated`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
