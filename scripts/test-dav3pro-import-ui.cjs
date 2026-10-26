// Actual Rust prepare() DTOs from authored Synapse exports; no OEM execution.
// Only browser IPC is mocked. Saved UI plans are replayed by a Rust codec test.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const prepared=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_IMPORT_PREPARED||'../outputs/100-model-development/dav3pro-import-prepared.json','utf8'));
const blocked=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_IMPORT_BLOCKED_PREPARED||'../outputs/100-model-development/dav3pro-import-blocked-prepared.json','utf8'));
const views=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_STAGES_FIXTURE||'../outputs/100-model-development/dav3pro-stages-native-views.json','utf8'));
assert.equal(prepared.length,2);assert(blocked.length>0);
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/dav3pro-import-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1100}}),errors=[],saved=[],checks=[];
 page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({views,initial})=>{const old=window.__TAURI_INTERNALS__.invoke;window.__qaPrepared=initial;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(cmd==='prepare_synapse_import'){window.__qaCalls.push({cmd,args});return window.__qaPrepared;}
  if(cmd==='multi_templates'){window.__qaCalls.push({cmd,args});return views;}
  if(cmd==='multi_simulate'){window.__qaCalls.push({cmd,args});return {packet_count:20,simulated:true,hardware_tested:false,software_macros:[]};}
  return old(cmd,args);
 };},{views,initial:prepared[0]});
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'配置',exact:true}).click();await page.getByRole('button',{name:'导入配置',exact:true}).click();await page.getByRole('button',{name:'导入 Synapse 配置',exact:true}).click();
 const load=async dto=>{await page.evaluate(value=>window.__qaPrepared=value,dto);await page.getByRole('button',{name:'选择 Synapse 导出文件',exact:true}).click();};
 for(const dto of prepared){
  const profile=dto.candidates[0].profile,plan=profile.multi;assert(profile);assert.equal(plan.settings.length,18);assert.equal(dto.parsed.profiles[0].bindings.length,16);
  assert(dto.parsed.profiles[0].bindings.every(b=>b.action.kind==='modelAction'&&b.label));
  assert(!/rawSource|raw_source|sourceJson|payload/.test(JSON.stringify(dto)));
  await load(dto);const preview=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'雷蛇 Synapse 配置导入',exact:true})});
  await preview.getByText('右 Alt + C',{exact:true}).waitFor();
  for(const binding of dto.parsed.profiles[0].bindings){assert(await preview.getByText(`${binding.controlId} · ${binding.layer}`,{exact:true}).isVisible());}
  assert.equal(await preview.getByText(/无法转换：/).count(),0);
  await preview.getByRole('button',{name:'检查并编辑转换方案',exact:true}).click();
  const editor=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'选择品牌与鼠标型号',exact:true})});
  await editor.getByRole('textbox',{name:'鼠标配置名称',exact:true}).waitFor();
  const dpi=plan.settings.find(s=>s.Dpi).Dpi,rate=plan.settings.find(s=>s.ReportRate).ReportRate;
  for(let i=0;i<dpi.stages.length;i++){
   assert.equal(await editor.getByRole('spinbutton',{name:`鼠标档位${i+1}X`,exact:true}).inputValue(),String(dpi.stages[i].x));
   assert.equal(await editor.getByRole('spinbutton',{name:`鼠标档位${i+1}Y`,exact:true}).inputValue(),String(dpi.stages[i].y));
   assert.equal(await editor.getByRole('radio',{name:`鼠标档位${i+1}`,exact:true}).isChecked(),i===dpi.active);
  }
  assert.equal(await editor.getByRole('combobox',{name:'鼠标回报率',exact:true}).inputValue(),String(rate));
  assert.equal(await editor.getByRole('combobox',{name:'鼠标按键2键码',exact:true}).inputValue(),'6');
  assert(await editor.getByLabel('右 Alt',{exact:true}).isChecked());
  assert.equal(await editor.locator('select[aria-label$="动作"]').count(),9); // 8 inputs + the DPI action selector
  await editor.getByRole('button',{name:'Hypershift 层',exact:true}).click();
  for(const id of [257,258,259,260,261,265,266,352])assert(await editor.getByRole('combobox',{name:`鼠标按键${id}动作`,exact:true}).isVisible());
  assert.equal(await editor.locator('select[aria-label^="原生灯区"]').count(),0);
  assert.equal(await editor.getByRole('combobox',{name:'软件宏播放方式',exact:true}).count(),0);
  await editor.getByRole('button',{name:'普通层',exact:true}).click();
  await editor.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await editor.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();
  await page.screenshot({path:path.join(out,plan.product.toString(16)+'-editor.png')});
  await editor.getByRole('button',{name:'保存设备方案',exact:true}).click();await editor.waitFor({state:'hidden'});
  const actual=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile);
  assert.equal(actual.id,profile.id);assert.equal(actual.target_model,plan.product);assert.equal(actual.target_vendor,plan.vendor);
  assert.deepEqual(actual.multi,plan);assert.deepEqual(actual.levels,profile.levels);assert.equal(actual.active_level,2);assert.equal(actual.report_rate_hz,rate);
  saved.push(actual);await preview.getByRole('button',{name:'保存转换后的方案',exact:true}).click();
  assert.deepEqual((await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile)).multi,plan);
  checks.push(plan.product.toString(16)+': source preview, 5 XY stages, one-based selection, rate, 16 independent bindings, native plan unchanged');
 }
 for(const dto of blocked){
  assert(dto.candidates.every(c=>!c.profile));await load(dto);
  assert.equal(await page.getByRole('button',{name:'检查并编辑转换方案',exact:true}).count(),0);
  assert.equal(await page.getByRole('button',{name:'保存转换后的方案',exact:true}).count(),0);
  for(const c of dto.candidates)for(const issue of c.issues)assert(await page.getByText(issue,{exact:true}).isVisible());
  checks.push('real blocked prepare DTO cannot save or reuse earlier accepted profile');
 }
 const calls=await page.evaluate(()=>window.__qaCalls);assert.equal(calls.filter(c=>c.cmd==='save_profile').length,4);
 assert(!calls.some(c=>['multi_apply','activate_profile','multi_save_hardware','multi_input_sources'].includes(c.cmd)));assert.deepEqual(errors,[]);
 fs.writeFileSync('../outputs/100-model-development/dav3pro-import-ui-profiles.json',JSON.stringify(saved,null,2));
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_prepare_DTOs:true,authored_source_fixture:true,hardware_tested:false,distinct_models:1,full_compatibility:false,transports:saved.map(p=>p.target_model),checks},null,2));
 console.log(`${saved.length} strict Synapse import UI plans preserved for production codec replay; ${blocked.length} blocked sources cannot save`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
