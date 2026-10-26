// Production UI over native DTOs; outputs are replayed by the real Rust codec separately.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const currentViews=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_FIXTURE||'../outputs/100-model-development/dav3pro-native-views.json','utf8'));
const stageViews=JSON.parse(fs.readFileSync(process.env.OPEN_HUB_DAV3PRO_STAGES_FIXTURE||'../outputs/100-model-development/dav3pro-stages-native-views.json','utf8'));
for(const views of [currentViews,stageViews])assert.deepEqual(views.map(v=>v.descriptor.product_id),[0xb6,0xb7]);
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/dav3pro-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const profiles=[];
 for(const [mode,views]of[['current',currentViews],['stages',stageViews]]){
 const page=await browser.newPage({viewport:{width:1450,height:1100}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(views=>{const old=window.__TAURI_INTERNALS__.invoke;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{if(['multi_templates','multi_simulate'].includes(cmd)){window.__qaCalls.push({cmd,args});return cmd==='multi_templates'?views:{packet_count:24,software_macros:[],simulated:true,hardware_tested:false};}return old(cmd,args);};},views);
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'配置',exact:true}).click();
 for(const view of views){
  await page.getByRole('button',{name:'按品牌新建配置',exact:true}).click();const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:view.descriptor.name,exact:true}).click();
  await dialog.getByRole('button',{name:'保存设备方案',exact:true}).click();await dialog.waitFor({state:'hidden'});
   const initial=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile.multi);
   const initialButtons=Object.fromEntries(initial.settings.filter(s=>s.Button).map(s=>[String(s.Button.id),s.Button.action]));assert.deepEqual(initialButtons,view.state.buttons);
   await page.getByRole('button',{name:'按品牌新建配置',exact:true}).click();await dialog.getByRole('button',{name:view.descriptor.name,exact:true}).click();
   const art=dialog.getByRole('img',{name:view.descriptor.name+' 外观图',exact:true});await require('./ui-image-ready.cjs')(art);assert((await art.getAttribute('src')).endsWith('razer-deathadder-v3-pro.png'));assert(await art.evaluate(img=>img.naturalWidth===300&&img.naturalHeight===300));
  const dpi=dialog.getByRole('spinbutton',{name:'鼠标档位1X',exact:true});assert.equal(await dpi.getAttribute('min'),'100');assert.equal(await dpi.getAttribute('max'),'30000');assert.equal(await dpi.getAttribute('step'),'50');assert.equal(await dpi.inputValue(),mode==='stages'?'400':'1600');
  assert.equal(await dialog.getByRole('radio').count(),mode==='stages'?5:1);assert.equal(await dialog.getByRole('button',{name:'新增 DPI 档位',exact:true}).count(),0);
  if(mode==='stages'){
   assert(await dialog.getByRole('radio',{name:'鼠标档位3',exact:true}).isChecked());
   while(await dialog.getByRole('radio').count()>2)await dialog.getByRole('button',{name:'删除',exact:true}).last().click();
   assert.equal(await dialog.getByRole('button',{name:'删除',exact:true}).count(),0);
   while(await dialog.getByRole('radio').count()<5)await dialog.getByRole('button',{name:'新增 DPI 档位',exact:true}).click();
   for(const axis of ['X','Y'])await dialog.getByRole('spinbutton',{name:'鼠标档位5'+axis,exact:true}).fill('30000');
   await dialog.getByRole('radio',{name:'鼠标档位5',exact:true}).check();
  }
  await dpi.fill('3200');await dialog.getByRole('spinbutton',{name:'鼠标档位1Y',exact:true}).fill('3200');
  const rate=dialog.getByRole('combobox',{name:'鼠标回报率',exact:true});assert.deepEqual(await rate.locator('option').evaluateAll(o=>o.filter(x=>x.value).map(x=>Number(x.value))),[125,500,1000]);await rate.selectOption('500');
  const perf=dialog.getByRole('region',{name:'鼠标性能',exact:true}),idle=perf.getByRole('combobox',{name:'自动休眠时间',exact:true}),low=perf.getByRole('combobox',{name:'低电量模式阈值',exact:true});
  assert.equal(await idle.inputValue(),'300');assert.equal(await low.inputValue(),'30');
  for(const [el,choices]of[[idle,view.capabilities.tuning.device_sleep_seconds],[low,view.capabilities.tuning.low_power_percent]])assert.deepEqual(await el.locator('option').evaluateAll(o=>o.filter(x=>x.value).map(x=>Number(x.value))),choices);
  await idle.selectOption('900');await low.selectOption('5');assert.equal(await perf.getByText('按键与追踪',{exact:true}).count(),0);assert.equal(await perf.getByRole('switch').count(),0);
  assert.equal(await perf.getByRole('combobox',{name:'按键消抖',exact:true}).count(),0);assert.equal(await perf.getByRole('combobox',{name:'传感器休眠时间',exact:true}).count(),0);
  assert.equal(await dialog.getByRole('button',{name:'新增软件宏',exact:true}).count(),0);assert.equal(await dialog.locator('select[aria-label^="原生灯区"]').count(),0);
  await dialog.getByRole('button',{name:'普通层',exact:true}).click();
  for(const id of [1,2,3,4,5,9,10,96])assert.equal(await dialog.getByRole('combobox',{name:`鼠标按键${id}动作`,exact:true}).count(),1);
  const action=id=>dialog.getByRole('combobox',{name:`鼠标按键${id}动作`,exact:true});
  assert(await action(1).locator('option[value="Key"]').isDisabled());
   await action(5).selectOption('Mouse');await action(1).selectOption('Key');await dialog.getByRole('combobox',{name:'鼠标按键1键码',exact:true}).selectOption('7');
   assert(await action(5).locator('option[value="Disabled"]').isDisabled());
   const keyCodes=await dialog.getByRole('combobox',{name:'鼠标按键1键码',exact:true}).locator('option').evaluateAll(o=>o.map(x=>Number(x.value)));assert(keyCodes.length>30&&keyCodes.every(x=>x>=4&&x<=0xa4));
   await action(2).selectOption('Key');await dialog.getByRole('combobox',{name:'鼠标按键2键码',exact:true}).selectOption('6');const right=action(2).locator('..');await right.getByLabel('Ctrl',{exact:true}).check();await right.getByLabel('右 Alt',{exact:true}).check();
  await action(3).selectOption('Media');await dialog.getByRole('combobox',{name:'鼠标按键3多媒体动作',exact:true}).selectOption('205');
  await action(4).selectOption('Hypershift');
  assert.equal(await dialog.getByRole('combobox',{name:'鼠标按键5鼠标键',exact:true}).inputValue(),'Left');
  await action(9).selectOption('Key');await dialog.getByRole('combobox',{name:'鼠标按键9键码',exact:true}).selectOption('8');assert.equal(await action(9).locator('option[value="Dpi"]').count(),0);
  await action(10).selectOption('Disabled');await action(96).selectOption('Dpi');await dialog.getByRole('combobox',{name:'鼠标按键96DPI动作',exact:true}).selectOption('Shift');const clutch=dialog.getByRole('spinbutton',{name:'鼠标按键96狙击DPI',exact:true});assert.equal(await clutch.getAttribute('max'),'30000');await clutch.fill('30000');
  await dialog.getByRole('button',{name:'Hypershift 层',exact:true}).click();assert.equal(await action(2).count(),0);
  for(const id of [257,258,259,260,261,265,266,352]){assert.equal(await action(id).count(),1);assert.equal(await action(id).locator('option[value="Hypershift"]').count(),id===260?1:0);}
  await action(257).selectOption('Mouse');await dialog.getByRole('combobox',{name:'鼠标按键257鼠标键',exact:true}).selectOption('Right');
  await action(258).selectOption('Key');await dialog.getByRole('combobox',{name:'鼠标按键258键码',exact:true}).selectOption('9');await action(258).locator('..').getByLabel('右 Win',{exact:true}).check();
  await action(259).selectOption('Media');await dialog.getByRole('combobox',{name:'鼠标按键259多媒体动作',exact:true}).selectOption('234');
  assert(await action(260).isDisabled());assert.equal(await action(260).inputValue(),'Hypershift');await action(261).selectOption('Dpi');await dialog.getByRole('combobox',{name:'鼠标按键261DPI动作',exact:true}).selectOption('Down');
  await action(265).selectOption('Key');await dialog.getByRole('combobox',{name:'鼠标按键265键码',exact:true}).selectOption('8');assert.equal(await action(265).locator('option[value="Dpi"]').count(),0);
  await action(266).selectOption('Disabled');await action(352).selectOption('Dpi');await dialog.getByRole('combobox',{name:'鼠标按键352DPI动作',exact:true}).selectOption('Shift');await dialog.getByRole('spinbutton',{name:'鼠标按键352狙击DPI',exact:true}).fill('2400');
  await dialog.getByRole('button',{name:'普通层',exact:true}).click();assert.equal(await action(4).inputValue(),'Hypershift');assert.equal(await action(2).inputValue(),'Key');
  await dialog.getByRole('button',{name:'Hypershift 层',exact:true}).click();assert.equal(await dialog.getByRole('combobox',{name:'鼠标按键257鼠标键',exact:true}).inputValue(),'Right');
  assert.equal(await dialog.getByText('此型号的按键写入仍在适配，当前不能应用按键和宏。',{exact:true}).count(),0);assert(await dialog.getByText('此型号未提供可控制的 RGB 灯区。独立底座与电脑灯光可在“灯光同步”中选择。',{exact:true}).isVisible());
  await dialog.getByRole('textbox',{name:'鼠标配置名称',exact:true}).fill(view.descriptor.name+' · '+mode+' 已知原生功能');
  await dialog.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await dialog.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();await perf.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(out,mode+'-'+view.descriptor.product_id.toString(16)+'-power.png')});
  await dialog.getByRole('button',{name:'保存设备方案',exact:true}).click();await dialog.waitFor({state:'hidden'});
  const p=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile),settings=p.multi.settings;
  assert.equal(p.target_model,view.descriptor.product_id);assert.equal(p.target_vendor,0x1532);assert.equal(settings.find(s=>s.Tuning).Tuning.device_sleep_seconds,900);assert.equal(settings.find(s=>s.Tuning).Tuning.low_power_percent,5);assert.equal(settings.find(s=>s.Dpi).Dpi.stages[0].x,3200);assert.equal(settings.find(s=>s.ReportRate).ReportRate,500);assert.equal(settings.filter(s=>s.Button).length,16);assert.equal(settings.filter(s=>s.Lighting).length,0);assert.deepEqual(settings.find(s=>s.Button?.id===2).Button.action,{Key:{usage:6,mods:65}});assert.equal(settings.find(s=>s.Button?.id===3).Button.action.Media,205);assert.deepEqual(settings.find(s=>s.Button?.id===96).Button.action,{Dpi:{Shift:30000}});assert.equal(settings.find(s=>s.Button?.id===4).Button.action,'Hypershift');assert.equal(settings.find(s=>s.Button?.id===260).Button.action,'Hypershift');assert.deepEqual(settings.find(s=>s.Button?.id===1).Button.action,{Key:{usage:7,mods:0}});assert.deepEqual(settings.find(s=>s.Button?.id===5).Button.action,{Mouse:'Left'});assert.deepEqual(settings.find(s=>s.Button?.id===257).Button.action,{Mouse:'Right'});assert.deepEqual(settings.find(s=>s.Button?.id===258).Button.action,{Key:{usage:9,mods:128}});assert.deepEqual(settings.find(s=>s.Button?.id===352).Button.action,{Dpi:{Shift:2400}});assert.equal(p.multi.macros.length,0);const saved=settings.find(s=>s.Dpi).Dpi;assert.equal(saved.stages.length,mode==='stages'?5:1);assert.equal(saved.active,mode==='stages'?4:0);if(mode==='stages')assert.equal(saved.stages[4].x,30000);profiles.push(p);
 }
 const calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>['multi_apply','activate_profile','multi_input_sources','multi_save_hardware'].includes(c.cmd)));assert.deepEqual(errors,[]);await page.close();
 }
 assert.equal(profiles.length,4);
 fs.writeFileSync('../outputs/100-model-development/dav3pro-ui-profiles.json',JSON.stringify(profiles,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_DTOs:true,hardware_tested:false,distinct_models:1,transports:2,full_compatibility:false,checks:['exact original model artwork','full 2–5 stage editor, active slot and 30K XY; separate explicit-refusal current-only editor','30K bound and stock receiver rates','15 native idle choices and 20 low-battery choices','eight exact physical inputs; all sixteen known native records retained; conditional primary remap and click witness first; paired native Hypershift actuator locks own secondary; implemented key choices only; right modifiers/media/DPI','no unrelated tracking, invented RGB or unverified host macros','four saved UI plans captured for native driver replay']},null,2));console.log('DeathAdder V3 Pro wired/wireless UI saved four current/five-stage model plans for native replay');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
