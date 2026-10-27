// Native-generated DTOs, mocked read-only IPC; no real USB or desktop control.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const out=path.resolve(process.env.OPEN_HUB_COMPX_BATTERY_UI_OUT??'../outputs/100-model-development/compx-battery-ui');fs.mkdirSync(out,{recursive:true});const views=JSON.parse(fs.readFileSync('../outputs/100-model-development/compx-battery-native-views.json','utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1150}}),errors=[],checks=[];page.on('pageerror',e=>errors.push(String(e)));await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(views=>{
  const clone=x=>structuredClone(x),a=clone(views.find(v=>v.capabilities.battery)),b=clone(a);a.descriptor.id+=':A';a.descriptor.name+=' A';b.descriptor.id+=':B';b.descriptor.name+=' B';a.descriptor.simulated=b.descriptor.simulated=false;
  const live=[a,b],peers=live.map(v=>({id:'multi:'+v.descriptor.id,vendor_id:v.descriptor.vendor_id,product_id:v.descriptor.product_id.toString(16),name:v.descriptor.name,product:v.descriptor.name,kind:'mouse',model_ids:[v.descriptor.product_id.toString(16)],firmware_name:'memory fixture',connection:'USB',protocol:'ATK',features:[]}));
  window.__battery={mode:'valid',pending:null,peers};const old=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(!['scan_devices','list_devices','multi_templates','multi_read','multi_battery'].includes(cmd))return old(cmd,args);window.__qaCalls.push({cmd,args:clone(args)});
   if(cmd==='scan_devices')return{devices:peers,warnings:[]};if(cmd==='list_devices')return peers;if(cmd==='multi_templates')return clone(views);
   if(cmd==='multi_read')return clone(live.find(v=>'multi:'+v.descriptor.id===args.id));
   if(!live.some(v=>v.descriptor.id===args.id))throw Error('wrong battery target');
   if(window.__battery.mode==='hold')return new Promise(resolve=>window.__battery.pending=()=>resolve({percent:99,charging:true}));
   if(window.__battery.mode==='unknown')return null;if(window.__battery.mode==='failure')throw Error('authored unavailable query');return{percent:32,charging:false};
  };
 },views);
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 const button=name=>page.getByRole('button',{name,exact:true}),reading=()=>page.getByLabel('电量读数',{exact:true});
 await button('配置').click();
 for(const v of views){await button('按品牌新建配置').click();const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:v.descriptor.name,exact:true}).click();if(v.capabilities.battery){assert((await reading().textContent()).includes('73% · 正在充电'));assert((await reading().textContent()).includes('模拟数据'));assert(await dialog.getByRole('button',{name:'刷新电量',exact:true}).isDisabled());}else assert.equal(await reading().count(),0);await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});}
 checks.push('nine exact native DTOs show battery and charging; two Pulsar DTOs have no inherited battery control');
 assert.equal((await page.evaluate(()=>window.__qaCalls)).filter(c=>c.cmd==='multi_battery').length,0);checks.push('offline editing issues no battery or hardware queries');
 await button('设备').click();const peers=await page.evaluate(()=>window.__battery.peers);await button('Open '+peers[0].name).click();await reading().waitFor();
 const dpi=page.getByRole('spinbutton',{name:'鼠标档位1X',exact:true});await dpi.fill('1250');assert((await reading().textContent()).includes('73%'));
 await button('刷新电量').click();await button('刷新电量').waitFor({state:'visible'});await page.waitForFunction(()=>document.querySelector('[aria-label="电量读数"]').textContent.includes('32%'));
 assert((await reading().textContent()).includes('未充电'));assert.equal(await dpi.inputValue(),'1250');checks.push('manual device-specific refresh changes telemetry and retains edited DPI');
 for(const mode of ['unknown','failure']){await page.evaluate(mode=>window.__battery.mode=mode,mode);await button('刷新电量').click();await page.waitForFunction(()=>!document.querySelector('[aria-label="电量读数"]').textContent.includes('%'));assert.equal(await dpi.inputValue(),'1250');assert((await reading().textContent()).includes('未能读取电量'));}checks.push('missing data and errors remove stale percentage without inventing 100 percent');
 await page.evaluate(()=>window.__battery.mode='hold');await button('刷新电量').click();await page.waitForFunction(()=>!!window.__battery.pending);assert(await button('正在读取…').isDisabled());await button('Back to devices').click();await button('Open '+peers[1].name).click();await reading().waitFor();await page.evaluate(async()=>{window.__battery.pending();await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)));});assert((await reading().textContent()).includes('73%'));assert(!(await reading().textContent()).includes('99%'));checks.push('late battery response from mouse A cannot populate mouse B');
 const calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>['multi_apply','activate_profile'].includes(c.cmd)));assert.deepEqual(errors,[]);await page.screenshot({path:path.join(out,'battery.png'),fullPage:true});fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_DTO:true,mocked_IPC:true,physical_HID:false,OS_input:false,battery_queries:calls.filter(c=>c.cmd==='multi_battery'),checks},null,2));console.log(`${checks.length} COMPX battery UI groups passed`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});