const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const root=path.resolve('dist'),out=path.resolve(process.env.OPEN_HUB_MANUAL_RECOVERY_UI_OUT??'../manual-recovery-ui-qa');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{';
const start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);if(start<marker.length||end<start)throw Error('Shared fixture missing');
(async()=>{const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-gpu']});try{
 const page=await browser.newPage({viewport:{width:1450,height:1000},deviceScaleFactor:1}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(()=>{
  const source={vendor:0x1b1c,product:0x1b2e,path:'fixture:vendor-input'};
  const mouse={id:'multi:fixture:m65',vendor_id:0x1b1c,name:'Corsair M65 PRO RGB',product:'Corsair M65 PRO RGB',product_id:'1B2E',kind:'mouse',firmware_name:'',model_ids:['1B2E'],connection:'USB',protocol:'Open HUB',features:[]};
  const view={descriptor:{id:'fixture:m65',name:mouse.name,vendor_id:source.vendor,product_id:source.product,serial:'unit-a',simulated:true},capabilities:{dpi:{min:100,max:12000,step:1,max_stages:5,separate_xy:true},report_rates:[125,250,500,1000],buttons:[{id:4,name:'后退',remappable:true}],lighting:[]},state:{dpi:{stages:[{x:800,y:800}],active:0},report_rate:1000,buttons:{},lighting:{}},readback:'device_partial',hardware_tested:false};
  let record={key:'macro-original.json',revision:'review-v1',record:{serial:null,device_id:'old:m65',source,software_input:true,settings:[],shift:{original:{stages:[{x:1600,y:1600}],active:0},expected:{stages:[{x:400,y:400}],active:0}}}};
  const original=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
   if(!['scan_devices','list_devices','multi_read','multi_input_sources','multi_recovery_list','multi_recovery_observe','multi_recovery_restore'].includes(cmd))return original(cmd,a);
   window.__qaCalls.push({cmd,args:a});
   if(cmd==='scan_devices')return {devices:[mouse],warnings:[]};if(cmd==='list_devices')return [mouse];if(cmd==='multi_read')return view;
   if(cmd==='multi_input_sources')return [{source,kind:'Vendor HID',serial:null},{source:{...source,path:'fixture:other-input'},kind:'Vendor HID',serial:null}];
   if(cmd==='multi_recovery_list'){if(window.__qaBadList)throw Error('恢复记录损坏，已保留原文件');return {records:record?[{...record,record:{...record.record,serial:window.__qaRecordSerial??null}}]:[],errors:[]};}
   if(cmd==='multi_recovery_observe')return;
   if(cmd==='multi_recovery_restore'){if(window.__qaHeld)throw Error('触发键或鼠标按钮仍按住，请全部松开后重试');if(window.__qaStale)throw Error('恢复记录已变化，请刷新并重新核对');record=null;return;}
  };
 });
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:/Corsair M65 PRO RGB/}).first().click();
 const panel=page.getByRole('region',{name:'待恢复按键记录'});
 await panel.getByRole('button',{name:'查看待恢复记录',exact:true}).click();
 await panel.getByText(/恢复原始 DPI：1600 × 1600/).waitFor();
 assert(await panel.getByRole('button',{name:'准备恢复所选鼠标',exact:true}).isDisabled());
 let calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>c.cmd==='multi_recovery_restore'));
 await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).selectOption('fixture:vendor-input');
 await page.evaluate(()=>{window.__qaRecordSerial='unit-b';});await panel.getByRole('button',{name:'查看待恢复记录',exact:true}).click();await panel.getByRole('alert').filter({hasText:'序列号与当前鼠标不一致'}).waitFor();assert(await panel.getByRole('button',{name:'准备恢复所选鼠标',exact:true}).isDisabled());
 await page.evaluate(()=>{window.__qaRecordSerial=null;});await panel.getByRole('button',{name:'查看待恢复记录',exact:true}).click();
 await panel.getByRole('button',{name:'准备恢复所选鼠标',exact:true}).click();
 await panel.getByRole('checkbox',{name:'确认恢复原始设置',exact:true}).waitFor();
 assert(await panel.getByRole('button',{name:'恢复以上原始设置',exact:true}).isDisabled());
 await panel.getByRole('checkbox',{name:'确认恢复原始设置',exact:true}).check();
 await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).selectOption('fixture:other-input');
 assert.equal(await panel.getByRole('button',{name:'恢复以上原始设置',exact:true}).count(),0);
 await page.getByRole('combobox',{name:'软件宏输入来源',exact:true}).selectOption('fixture:vendor-input');
 await panel.getByRole('button',{name:'准备恢复所选鼠标',exact:true}).click();await panel.getByRole('checkbox',{name:'确认恢复原始设置',exact:true}).check();
 await page.evaluate(()=>{window.__qaHeld=true;});await panel.getByRole('button',{name:'恢复以上原始设置',exact:true}).click();await panel.getByRole('alert').filter({hasText:'仍按住'}).waitFor();
 await page.evaluate(()=>{window.__qaHeld=false;window.__qaStale=true;});await panel.getByRole('button',{name:'恢复以上原始设置',exact:true}).click();await panel.getByRole('alert').filter({hasText:'记录已变化'}).waitFor();
 await page.evaluate(()=>{window.__qaBadList=true;});await panel.getByRole('button',{name:'查看待恢复记录',exact:true}).click();await panel.getByRole('alert').filter({hasText:'恢复记录损坏'}).waitFor();assert.equal(await panel.getByRole('button',{name:'恢复以上原始设置',exact:true}).count(),0);
 await page.evaluate(()=>{window.__qaBadList=false;window.__qaStale=false;});await panel.getByRole('button',{name:'查看待恢复记录',exact:true}).click();await panel.getByRole('button',{name:'准备恢复所选鼠标',exact:true}).click();await panel.getByRole('checkbox',{name:'确认恢复原始设置',exact:true}).check();await page.screenshot({path:path.join(out,'review.png'),fullPage:true});await panel.getByRole('button',{name:'恢复以上原始设置',exact:true}).click();await panel.getByRole('status').filter({hasText:'恢复记录已清除'}).waitFor();
 calls=await page.evaluate(()=>window.__qaCalls);const restores=calls.filter(c=>c.cmd==='multi_recovery_restore');assert.equal(restores.length,3);for(const c of restores)assert.deepEqual(c.args,{id:'multi:fixture:m65',key:'macro-original.json',revision:'review-v1',source:{vendor:0x1b1c,product:0x1b2e,path:'fixture:vendor-input'},confirmed:true});assert(!calls.some(c=>c.cmd==='multi_apply'));assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({result:'passed',mock_backend:true,checks:['original DPI and routing review','source selection required','serial mismatch disables recovery','prepare never applies','explicit confirmation required','source change invalidates confirmation','held input retains recovery','stale record rejected','failed refresh clears old restore action','successful retry clears record','exact device/revision/source payload','no generic apply','no browser exceptions']},null,2));console.log('13 manual recovery UI checks passed (mock backend)');
 }finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
