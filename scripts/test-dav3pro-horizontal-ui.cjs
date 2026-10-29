// Production prepare DTOs and React. IPC/inventory/dialogs are mocked; no HID.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const prepared=JSON.parse(fs.readFileSync('../outputs/100-model-development/dav3pro-horizontal-prepared.json','utf8'));
const views=JSON.parse(fs.readFileSync('../outputs/100-model-development/dav3pro-stages-native-views.json','utf8'));
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const buttons=p=>Object.fromEntries(p.settings.filter(s=>s.Button).map(s=>[s.Button.id,s.Button.action]));
(async()=>{const browser=await chromium.launch({channel:'msedge',headless:true});try{
 const page=await browser.newPage({viewport:{width:1450,height:1100}}),errors=[];
 page.on('pageerror',e=>errors.push(String(e)));await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({views,prepared})=>{const old=window.__TAURI_INTERNALS__.invoke;window.__horizontalPrepared=prepared[0];window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(cmd==='prepare_synapse_import'){window.__qaCalls.push({cmd,args});return window.__horizontalPrepared;}
  if(cmd==='multi_templates')return views;
  return old(cmd,args);
 };},{views,prepared});await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 await page.getByRole('button',{name:'配置',exact:true}).click();
 for(const dto of prepared){
  const profile=dto.candidates[0].profile,plan=profile.multi;assert.equal(plan.settings.length,18);
  await page.getByRole('button',{name:'导入配置',exact:true}).click();await page.getByRole('button',{name:'导入 Synapse 配置',exact:true}).click();
  await page.evaluate(v=>window.__horizontalPrepared=v,dto);await page.getByRole('button',{name:'选择 Synapse 导出文件',exact:true}).click();
  const preview=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'雷蛇 Synapse 配置导入',exact:true})});
  assert.equal(await preview.getByText(/无法转换：/).count(),0);await preview.getByRole('button',{name:'检查并编辑转换方案',exact:true}).click();
  let editor=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'选择品牌与鼠标型号',exact:true})});
  await editor.getByRole('textbox',{name:'鼠标配置名称',exact:true}).fill(profile.name+' · '+plan.product.toString(16));
  for(const[layer,ids,expected]of[['普通层',[4,5],['WheelLeft','WheelRight']],['Hypershift 层',[260,261],['WheelRight','WheelLeft']]]){
   await editor.getByRole('button',{name:layer,exact:true}).click();
   for(let i=0;i<2;i++){
    const field=editor.getByRole('combobox',{name:`鼠标按键${ids[i]}鼠标键`,exact:true});assert.equal(await field.inputValue(),expected[i]);
    assert.equal(await field.getByRole('option',{name:'向左滚动',exact:true}).count(),1);assert.equal(await field.getByRole('option',{name:'向右滚动',exact:true}).count(),1);
   }
  }
  await editor.getByRole('button',{name:'保存设备方案',exact:true}).click();await editor.waitFor({state:'hidden'});
  const original=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile);assert.deepEqual(original.multi,plan);
  await page.keyboard.press('Escape');await preview.waitFor({state:'hidden'});
  await page.getByRole('button',{name:'Edit '+original.name,exact:true}).click();editor=page.getByRole('dialog');
  const changed=editor.getByRole('combobox',{name:'鼠标按键4鼠标键',exact:true});await changed.selectOption('WheelRight');
  if(plan.product===0xb6){await changed.evaluate(el=>el.scrollIntoView({block:'center'}));await page.screenshot({path:'../outputs/100-model-development/dav3pro-horizontal-editor.png',animations:'disabled'});}
  await editor.getByRole('button',{name:'保存设备方案',exact:true}).click();await editor.waitFor({state:'hidden'});
  const edited=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='save_profile').at(-1).args.profile),wanted=buttons(plan);wanted[4]={Mouse:'WheelRight'};assert.deepEqual(buttons(edited.multi),wanted);
  await page.getByRole('button',{name:'Edit '+edited.name,exact:true}).click();editor=page.getByRole('dialog');assert.equal(await editor.getByRole('combobox',{name:'鼠标按键4鼠标键',exact:true}).inputValue(),'WheelRight');
  await page.keyboard.press('Escape');await editor.waitFor({state:'hidden'});
 }
 const calls=await page.evaluate(()=>window.__qaCalls),profiles=calls.filter(c=>c.cmd==='save_profile').map(c=>c.args.profile);assert.equal(profiles.length,4);assert.deepEqual(errors,[]);assert(!calls.some(c=>['multi_apply','activate_profile'].includes(c.cmd)));
 fs.writeFileSync('../outputs/100-model-development/dav3pro-horizontal-ui-profiles.json',JSON.stringify(profiles,null,2));
 console.log('DeathAdder horizontal UI passed: both transports/layers, four imported and edited saves, reopen preserves directions; no device writes');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
