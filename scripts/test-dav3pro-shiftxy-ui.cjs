// Actual prepare(path) DTOs from authored complete Synapse exports. Browser
// IPC is stubbed; saved plans are separately replayed by the production codec.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const prepared=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_SHIFTXY_IMPORT_PREPARED||'../outputs/100-model-development/dav3pro-shiftxy-import-prepared.json','utf8'));
const views=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_STAGES_FIXTURE||'../outputs/100-model-development/dav3pro-stages-native-views.json','utf8'));
assert.deepEqual(prepared.map(p=>p.candidates[0].profile.target_model),[0xb6,0xb7]);
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/dav3pro-shiftxy-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const buttons=plan=>Object.fromEntries(plan.settings.filter(s=>s.Button).map(s=>[s.Button.id,s.Button.action]));
const expectedButtons=plan=>({...buttons(plan),2:{Dpi:{ShiftXY:{x:1000,y:2000}}},258:{Dpi:{ShiftXY:{x:2400,y:4800}}}});
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1100}}),errors=[],saved=[],checks=[];
 page.on('pageerror',error=>errors.push(String(error)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({views,initial})=>{const old=window.__TAURI_INTERNALS__.invoke;window.__qaPrepared=initial;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(cmd==='prepare_synapse_import'){window.__qaCalls.push({cmd,args});return window.__qaPrepared;}
  if(cmd==='multi_templates'){window.__qaCalls.push({cmd,args});return views;}
  if(cmd==='multi_simulate'){window.__qaCalls.push({cmd,args});return {packet_count:20,simulated:true,hardware_tested:false,software_macros:[]};}
  return old(cmd,args);
 };},{views,initial:prepared[0]});
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'配置',exact:true}).click();await page.getByRole('button',{name:'导入配置',exact:true}).click();await page.getByRole('button',{name:'导入 Synapse 配置',exact:true}).click();
 for(const dto of prepared){
  const profile=dto.candidates[0].profile,plan=profile.multi,original=JSON.stringify(dto);
  assert.equal(plan.settings.length,18);assert.equal(dto.parsed.profiles[0].bindings.length,16);
  assert(!/rawSource|raw_source|sourceJson|payload/.test(original));
  await page.evaluate(value=>window.__qaPrepared=value,dto);await page.getByRole('button',{name:'选择 Synapse 导出文件',exact:true}).click();
  const preview=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'雷蛇 Synapse 配置导入',exact:true})});
  assert.equal(await preview.getByText(/无法转换：/).count(),0);
  await preview.getByRole('button',{name:'检查并编辑转换方案',exact:true}).click();
  const editor=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'选择品牌与鼠标型号',exact:true})});
  await editor.getByRole('textbox',{name:'鼠标配置名称',exact:true}).waitFor();
  const independent=id=>editor.getByRole('checkbox',{name:`鼠标按键${id}独立狙击XY`,exact:true});
  const uniform=id=>editor.getByRole('spinbutton',{name:`鼠标按键${id}狙击DPI`,exact:true});
  const axis=(id,name)=>editor.getByRole('spinbutton',{name:`鼠标按键${id}狙击DPI${name}`,exact:true});
  for(const [layer,xyId,uniformId,x,y,newX,newY,legacy]of[
   ['普通层',2,3,800,1600,1000,2000,1200],
   ['Hypershift 层',258,259,2400,3200,2400,4800,1800],
  ]){
   await editor.getByRole('button',{name:layer,exact:true}).click();
   assert(await independent(xyId).isChecked());assert(!await independent(uniformId).isChecked());
   assert.equal(await axis(xyId,'X').inputValue(),String(x));assert.equal(await axis(xyId,'Y').inputValue(),String(y));
   for(const name of ['X','Y']){assert.equal(await axis(xyId,name).getAttribute('min'),'100');assert.equal(await axis(xyId,name).getAttribute('max'),'30000');}
   assert.equal(await uniform(uniformId).inputValue(),String(legacy));
   await independent(uniformId).check();assert.equal(await axis(uniformId,'X').inputValue(),String(legacy));assert.equal(await axis(uniformId,'Y').inputValue(),String(legacy));
   await independent(uniformId).uncheck();assert.equal(await uniform(uniformId).inputValue(),String(legacy));
   await independent(xyId).uncheck();assert.equal(await uniform(xyId).inputValue(),String(x));
   await uniform(xyId).fill(String(newX));await independent(xyId).check();
   assert.equal(await axis(xyId,'X').inputValue(),String(newX));assert.equal(await axis(xyId,'Y').inputValue(),String(newX));
   await axis(xyId,'Y').fill(String(newY));
   for(const wheel of layer==='普通层'?[9,10]:[265,266])assert.equal(await editor.getByRole('combobox',{name:`鼠标按键${wheel}动作`,exact:true}).locator('option[value="Dpi"]').count(),0);
  }
  await editor.getByRole('button',{name:'普通层',exact:true}).click();assert.equal(await axis(2,'Y').inputValue(),'2000');assert.equal(await uniform(3).inputValue(),'1200');
  assert.equal(await editor.locator('select[aria-label^="原生灯区"]').count(),0);assert.equal(await editor.getByRole('button',{name:'新增软件宏',exact:true}).count(),0);
  assert(await editor.getByText('此型号未提供可控制的 RGB 灯区。独立底座与电脑灯光可在“灯光同步”中选择。',{exact:true}).isVisible());
  await editor.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await editor.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();
  const simulated=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='multi_simulate').at(-1).args.plan);assert.deepEqual(buttons(simulated),expectedButtons(plan));
  await axis(2,'X').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(out,plan.product.toString(16)+'-xy.png')});
  await editor.getByRole('button',{name:'保存设备方案',exact:true}).click();await editor.waitFor({state:'hidden'});
  const actual=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile);
  assert.equal(actual.target_vendor,0x1532);assert.equal(actual.target_model,plan.product);assert.equal(actual.multi.settings.length,18);
  assert.deepEqual(buttons(actual.multi),expectedButtons(plan));assert.deepEqual(actual.multi.macros,[]);assert.equal(actual.multi.source,null);
  assert.deepEqual(actual.multi.settings.filter(s=>!s.Button),plan.settings.filter(s=>!s.Button));
  assert.deepEqual(actual.levels,profile.levels);assert.equal(actual.active_level,profile.active_level);assert.equal(actual.report_rate_hz,profile.report_rate_hz);
  assert.equal(JSON.stringify(dto),original);saved.push(actual);checks.push(plan.product.toString(16)+': real source XY/legacy clutches displayed, checkbox toggled both directions in both layers, full16 saved without RGB or host macro');
 }
 const calls=await page.evaluate(()=>window.__qaCalls);assert.equal(calls.filter(c=>c.cmd==='save_profile').length,2);
 assert(!calls.some(c=>['multi_apply','activate_profile','multi_save_hardware','multi_input_sources'].includes(c.cmd)));assert.deepEqual(errors,[]);
 fs.writeFileSync(process.env.OPEN_HUB_DAV3PRO_SHIFTXY_UI_PROFILES||'../outputs/100-model-development/dav3pro-shiftxy-ui-profiles.json',JSON.stringify(saved,null,2));
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,production_prepare_DTOs:true,authored_source_fixture:true,hardware_tested:false,distinct_models:1,full_compatibility:false,transports:saved.map(p=>p.target_model),checks},null,2));
 console.log('Two exact DA source UI plans saved with independent XY and legacy uniform clutch in both layers for native replay');
}finally{await browser.close();}})().catch(error=>{console.error(error);process.exitCode=1;});
