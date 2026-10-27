// Built React UI + Rust-generated DTOs. No physical HID or host input injection.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const views=JSON.parse(fs.readFileSync('../outputs/100-model-development/compx-palette-native-views.json','utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const incomplete=structuredClone(views.find(v=>v.capabilities.dpi_color_stages));incomplete.descriptor.id+=':bad';incomplete.descriptor.name+=' · 未读档位';delete incomplete.state.dpi_colors['7'];
const unknown=structuredClone(views.find(v=>v.capabilities.dpi_color_stages));unknown.descriptor.id+=':unknown';unknown.descriptor.name+=' · 未知模式';unknown.state.lighting={};
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1050}}),errors=[],checks=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});await page.addInitScript(views=>{const old=window.__TAURI_INTERNALS__.invoke;window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{if(cmd==='multi_templates'){window.__qaCalls.push({cmd,args:a});return views;}if(cmd==='multi_simulate'){window.__qaCalls.push({cmd,args:a});return{packet_count:20};}return old(cmd,a);};},[...views,incomplete,unknown]);
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'配置',exact:true}).click();
 async function open(v){await page.getByRole('button',{name:'按品牌新建配置',exact:true}).click();const d=page.getByRole('dialog');await d.getByRole('button',{name:v.descriptor.name,exact:true}).click();return d;}
 for(const v of views){const d=await open(v),palette=d.getByRole('region',{name:'DPI 档位指示颜色',exact:true});
  if(!v.capabilities.dpi_color_stages){assert.equal(await palette.count(),0);await page.keyboard.press('Escape');continue;}
  assert.equal(await palette.locator('input[type="color"]').count(),8);assert.equal(await palette.getByLabel('DPI 档位8指示颜色',{exact:true}).inputValue(),'#112233');
  await palette.getByLabel('DPI 档位1指示颜色',{exact:true}).fill('#aabbcc');await palette.getByLabel('DPI 档位8指示颜色',{exact:true}).fill('#334455');
  assert.equal(await d.getByLabel('鼠标灯区0颜色',{exact:true}).inputValue(),'#00ff00');
  for(let n=v.state.dpi.stages.length;n<8;n++)await d.getByRole('button',{name:'新增 DPI 档位',exact:true}).click();
  await d.getByRole('radio',{name:'鼠标档位8',exact:true}).check();assert.equal(await d.getByLabel('鼠标灯区0颜色',{exact:true}).inputValue(),'#334455');
  await d.getByLabel('鼠标灯区0颜色',{exact:true}).fill('#dd4455');assert.equal(await palette.getByLabel('DPI 档位8指示颜色',{exact:true}).inputValue(),'#dd4455');
  await d.getByRole('radio',{name:'鼠标档位1',exact:true}).check();assert.equal(await d.getByLabel('鼠标灯区0颜色',{exact:true}).inputValue(),'#aabbcc');await d.getByRole('radio',{name:'鼠标档位8',exact:true}).check();
  await d.getByRole('combobox',{name:'原生灯区0效果',exact:true}).selectOption('Off');await palette.getByLabel('DPI 档位7指示颜色',{exact:true}).fill('#102030');assert.equal(await d.getByRole('combobox',{name:'原生灯区0效果',exact:true}).inputValue(),'Off');await d.getByRole('combobox',{name:'原生灯区0效果',exact:true}).selectOption('Static');assert.equal(await d.getByLabel('鼠标灯区0颜色',{exact:true}).inputValue(),'#dd4455');
  await d.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await d.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();
  if(!checks.length)await palette.screenshot({path:'../outputs/100-model-development/compx-palette-ui.png'});
  await d.getByRole('button',{name:'保存设备方案',exact:true}).click();await d.waitFor({state:'hidden'});
  const p=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile),colors=p.multi.settings.filter(s=>s.DpiColor).map(s=>s.DpiColor);
  assert.equal(colors.length,8);assert.deepEqual(colors.find(c=>c.stage===0).color,{r:170,g:187,b:204});assert.deepEqual(colors.find(c=>c.stage===7).color,{r:221,g:68,b:85});
  assert.deepEqual(p.multi.settings.find(s=>s.Button?.id===2).Button.action,{Mouse:'Back'});const macro=p.multi.settings.find(s=>s.Button?.id===5).Button.action.Macro;assert.deepEqual(p.multi.macros.find(m=>m.id===macro).steps,v.native_macros.find(m=>m.id===v.state.buttons['5'].Macro).steps);checks.push(v.descriptor.name);
 }
 const bad=await open(incomplete);assert(await bad.getByLabel('DPI 档位8指示颜色',{exact:true}).isDisabled());assert.equal(await bad.getByLabel('DPI 档位1指示颜色',{exact:true}).isDisabled(),false);await page.keyboard.press('Escape');
 const u=await open(unknown);assert.equal(await u.getByRole('combobox',{name:'原生灯区0效果',exact:true}).inputValue(),'');await u.getByLabel('DPI 档位8指示颜色',{exact:true}).fill('#abcdef');await u.getByRole('button',{name:'模拟驱动验证',exact:true}).click();await u.getByRole('status').filter({hasText:'模拟驱动验证通过'}).waitFor();const p=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='multi_simulate').at(-1).args.plan);assert(!p.settings.some(s=>s.Lighting));await page.keyboard.press('Escape');
 const calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>['multi_apply','activate_profile','multi_macro_simulate'].includes(c.cmd)));assert.equal(checks.length,5);assert.deepEqual(errors,[]);
 fs.writeFileSync('../outputs/100-model-development/compx-palette-ui-profiles.json',JSON.stringify(calls.filter(c=>c.cmd==='save_profile').map(c=>c.args.profile),null,2));
 fs.writeFileSync('../outputs/100-model-development/compx-palette-ui-report.json',JSON.stringify({passed:true,hardware_accessed:false,model_cases:checks,negative_model_cases:views.filter(v=>!v.capabilities.dpi_color_stages).map(v=>v.descriptor.name),checks:['all eight per-stage colors from native DTO','disabled-stage color can be edited before enabling','active stage and global alias stay coherent both ways','palette edit retains global Off','unknown mode is not turned Off or Static implicitly','unreadable row disabled','nondefault button and native macro retained','saved native profile replay tested separately']},null,2));console.log('COMPX palette UI passed: five supported models, four no-indicator cases, unknown and unreadable states');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
