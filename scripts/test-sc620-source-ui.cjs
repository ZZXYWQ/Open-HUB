// Production Rust DTOs -> real built React -> captured source export -> Rust replay.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/sc620-source-ui');fs.mkdirSync(out,{recursive:true});
const dto=JSON.parse(fs.readFileSync('../outputs/100-model-development/sc620-source-ui-dtos.json','utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1400,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({dto})=>{const old=window.__TAURI_INTERNALS__.invoke;window.__sc620Mode='positive';window.__sc620Delayed=false;window.__sc620Cancel=false;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(cmd==='plugin:dialog|open')return window.__sc620Cancel?null:'C:/fixtures/sc620.db';
  if(cmd==='plugin:dialog|save')return 'C:/fixtures/Open-HUB-SC620-source.json';
  if(cmd==='preview_sc620_source'){window.__qaCalls.push({cmd,args});if(window.__sc620Mode==='error')throw Error('Invalid SC620 database');if(window.__sc620Delayed)await new Promise(r=>setTimeout(r,300));return structuredClone(args.profile===42?dto.second:dto[window.__sc620Mode]);}
  if(cmd==='export_sc620_source'){window.__qaCalls.push({cmd,args});return;}
  return old(cmd,args);
 };},{dto});
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'配置',exact:true}).click();await page.getByRole('button',{name:'导入配置',exact:true}).click();await page.getByRole('button',{name:'狼蛛 SC620 原厂配置预览',exact:true}).click();
 const dialog=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'狼蛛 SC620 配置预览',exact:true})});
 await dialog.getByRole('button',{name:'选择 SC620 配置文件',exact:true}).click();await dialog.getByText('按键映射 · 11 项',{exact:true}).waitFor();
 assert.equal(await dialog.getByRole('combobox',{name:'SC620 文件内配置'}).inputValue(),'7');assert.equal(await dialog.getByRole('option').count(),2);
 assert(await dialog.getByText('宏 42 → 槽位 2',{exact:true}).isVisible());assert(await dialog.getByText('宏 7 → 槽位 1',{exact:true}).isVisible());assert(await dialog.getByText('左 Ctrl + 右 Ctrl + A',{exact:true}).isVisible());
 for(const dpi of dto.positive.source.settings.patch.dpi)assert(await dialog.getByText(`${dpi} DPI`,{exact:true}).isVisible());
 assert(await dialog.getByText(/250 Hz · 消抖 17 ms · 灯效：循环呼吸/).isVisible());assert(await dialog.getByText('LOD 2 mm',{exact:true}).isVisible());
 assert.equal(await dialog.getByRole('button',{name:/应用|激活|写入|导入 \d/}).count(),0);
 await dialog.locator('summary').first().click();const event=dto.positive.source.macros[0].events[0];assert(await dialog.getByText(new RegExp(`原值 ${event.value}`)).first().isVisible());
 await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).click();await dialog.getByRole('status').filter({hasText:'源方案已保存'}).waitFor();
 const actual=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='export_sc620_source').at(-1).args.source);assert.deepEqual(actual,dto.positive.source);
 fs.writeFileSync('../outputs/100-model-development/sc620-source-ui-export.json',JSON.stringify(actual,null,2));await page.screenshot({path:path.join(out,'positive.png')});
 await dialog.getByRole('combobox',{name:'SC620 文件内配置'}).selectOption('42');await dialog.getByText('按键映射 · 1 项',{exact:true}).waitFor();assert.equal(await dialog.getByText('宏 42 → 槽位 2',{exact:true}).count(),0);
 await page.evaluate(()=>window.__sc620Mode='blocked');await dialog.getByRole('button',{name:'选择 SC620 配置文件',exact:true}).click();await dialog.getByRole('region',{name:'SC620 未转换内容'}).waitFor();
 assert(await dialog.getByText('dpi_flag = 5',{exact:true}).isVisible());assert(await dialog.getByRole('status').filter({hasText:'不能准备整份设备配置'}).isVisible());assert(await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).isEnabled());
 await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).click();await dialog.getByRole('status').filter({hasText:'源方案已保存'}).waitFor();const blocked=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='export_sc620_source').at(-1).args.source);assert.deepEqual(blocked,dto.blocked.source);
 await page.screenshot({path:path.join(out,'blocked.png')});
 await page.setViewportSize({width:820,height:850});await dialog.evaluate(el=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));await page.screenshot({path:path.join(out,'narrow-before-check.png')});const bounds=await dialog.boundingBox();console.log('SC620 narrow bounds',bounds);assert(bounds.x>=0&&bounds.y>=0&&bounds.width<=820&&bounds.y+bounds.height<=850);assert(await dialog.evaluate(el=>el.scrollWidth<=el.clientWidth+1));await page.screenshot({path:path.join(out,'narrow.png')});
 await page.evaluate(()=>window.__sc620Mode='error');await dialog.getByRole('button',{name:'选择 SC620 配置文件',exact:true}).click();await dialog.getByRole('alert').filter({hasText:'Invalid SC620 database'}).waitFor();assert.equal(await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).count(),0);assert.equal(await dialog.getByRole('combobox',{name:'SC620 文件内配置'}).count(),0);
 await page.evaluate(()=>{window.__sc620Mode='positive';window.__sc620Delayed=true;});await dialog.getByRole('button',{name:'选择 SC620 配置文件',exact:true}).click();assert(await dialog.getByRole('button',{name:'关闭',exact:true}).isDisabled());await dialog.getByText('按键映射 · 11 项',{exact:true}).waitFor();
 await page.evaluate(()=>window.__sc620Cancel=true);await dialog.getByRole('button',{name:'选择 SC620 配置文件',exact:true}).click();assert.equal(await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).count(),0);
 const forbidden=['save_profile','commit_profile_import','activate_profile','sino_apply','multi_apply','mchose_apply'];const calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>forbidden.includes(c.cmd)));assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,real_production_dtos:true,checks:['source IDs and independent profile switching','all eleven controls and sparse macro bindings','DPI colors tuning and lighting preview','original macro events visible','complete source export capture','unknown content visible and preserved on export','bad file clears stale actions','busy state blocks close','cancel clears stale preview','no hardware or generic profile commands','narrow viewport has no horizontal overflow','no browser errors'],browser_errors:errors},null,2));console.log('12 SC620 built UI checks passed; source export captured for native replay');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
