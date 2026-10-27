// Built React UI -> real Rust macro compiler/transaction -> authored memory mice.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert'),{spawnSync}=require('child_process');
const out=path.resolve(process.env.OPEN_HUB_MCHOSE_MACRO_SCOPE_UI_OUT??'../outputs/100-model-development/mchose-macro-scope-ui');fs.mkdirSync(out,{recursive:true});const calls=[];
function native(request){const n=calls.length,input=path.join(out,`request-${n}.json`),output=path.join(out,`response-${n}.json`);fs.writeFileSync(input,JSON.stringify(request));const r=spawnSync('cargo',['test','--manifest-path','src-tauri/Cargo.toml','--locked','mchose_macro_scope_ui_bridge','--','--ignored'],{windowsHide:true,encoding:'utf8',timeout:60000,env:{...process.env,OPEN_HUB_MCHOSE_MACRO_SCOPE_REQUEST:input,OPEN_HUB_MCHOSE_MACRO_SCOPE_RESPONSE:output}});fs.writeFileSync(path.join(out,`native-${n}.log`),(r.stdout??'')+(r.stderr??''));if(r.error)throw r.error;assert.equal(r.status,0,(r.stdout+r.stderr).slice(-3000));assert((r.stdout??'').includes('1 passed; 0 failed'));const response=JSON.parse(fs.readFileSync(output,'utf8'));calls.push({request,response});return response;}
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true}),checks=[];try{
 for(const scenario of ['fork','dedupe','full','dirty','late']){
  const nativeScenario=['dirty','late'].includes(scenario)?'fork':scenario,baseline=native({operation:'hardware',scenario:nativeScenario});assert(baseline.ok);
  const page=await browser.newPage({viewport:{width:1500,height:1150}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));await page.exposeFunction('__nativeScope',native);
  await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
  await page.addInitScript(({baseline,nativeScenario})=>{
   const clone=x=>structuredClone(x);let hardware=clone(baseline.value),macros=clone(baseline.macros);
   const peer={id:'macro-scope-A',name:'A7 V3 Ultra+ macro scope',product:'MCHOSE A7 V3 Ultra+',vendor_id:0x3837,product_id:'1014',model_ids:['4026'],kind:'mouse',firmware_name:'memory fixture',connection:'wireless',protocol:'MCHOSE',features:[],profile_key:hardware.target.device_key,connection_key:hardware.target.connection_key};
   window.__scope={hold:false,pending:null};const old=window.__TAURI_INTERNALS__.invoke;
   window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
    if(cmd==='scan_devices')return{devices:[clone(peer)],warnings:[]};if(cmd==='list_devices')return[clone(peer)];
    if(cmd==='mchose_hardware')return clone(hardware);
    if(cmd==='mchose_macro')return clone(macros.find(m=>m.slot===args.slot)??null);
    if(cmd==='mchose_apply'){
     const response=await window.__nativeScope({operation:'save',scenario:nativeScenario,args});
     if(window.__scope.hold){window.__scope.hold=false;await new Promise(resolve=>window.__scope.pending=resolve);}
     if(!response.ok)throw Error(response.error);hardware=clone(response.value);macros=clone(response.macros);return response.value;
    }
    return old(cmd,args);
   };
  },{baseline,nativeScenario});
  await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
  const button=name=>page.getByRole('button',{name,exact:true}),idle=()=>page.waitForFunction(()=>{const f=document.querySelector('fieldset');return f&&!f.disabled;});
  await button('Open A7 V3 Ultra+ macro scope').click();await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).waitFor();await button('宏').click();
  const slot=page.getByRole('combobox',{name:'宏槽',exact:true}),name=page.getByRole('textbox',{name:'宏名称',exact:true});
  if(scenario==='dirty'){await slot.selectOption('0');await idle();await name.fill('Destination unsaved draft');}
  await slot.selectOption('2');await idle();assert.equal(await name.inputValue(),'A');await name.fill('Edited private');await page.getByRole('combobox',{name:'宏循环方式',exact:true}).selectOption('2');
  if(scenario==='late')await page.evaluate(()=>window.__scope.hold=true);
  await button('保存到宏槽').click();
  if(scenario==='late'){
   await page.waitForFunction(()=>!!window.__scope.pending);await button('Back to devices').click();await button('Open A7 V3 Ultra+ macro scope').click();await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).waitFor();await button('宏').click();
   await page.evaluate(async()=>{window.__scope.pending();await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)));});assert.equal(await slot.inputValue(),'0');assert.notEqual(await name.inputValue(),'Edited private');checks.push('late remapped receipt cannot populate a remounted panel');
  }else{
   await idle();const response=calls.at(-1).response;
   if(scenario==='full'){assert(!response.ok);assert.equal(await slot.inputValue(),'2');assert.equal(await name.inputValue(),'Edited private');assert((await page.getByRole('alert').textContent()).includes('没有安全可用宏槽'));assert.equal(response.audit.has_backup,false);assert.equal(response.audit.macro_set_reports,0);checks.push('full global storage rejects with original draft and zero native SET');}
   else{
    assert(response.ok);const expected=scenario==='dedupe'?7:0;assert.equal(response.value.saved_macro.slot,expected);assert.equal(await slot.inputValue(),String(expected));assert.equal(response.audit.binding_set_reports,2);assert(response.audit.original_macro_preserved&&response.audit.other_boards_preserved);
    if(scenario==='dirty'){assert.equal(await name.inputValue(),'Destination unsaved draft');assert((await page.getByLabel('共享宏另存结果').textContent()).includes('Edited private'));assert((await page.getByLabel('宏槽保存状态').textContent()).includes('未保存草稿'));checks.push('destination unsaved draft survives remapping and carries the verified baseline');}
    else{assert.equal(await name.inputValue(),'Edited private');checks.push(`${scenario} native receipt selects actual slot ${expected+1} and displays saved content`);}
    if(scenario==='dedupe')assert.equal(response.audit.macro_set_reports,0);else assert.equal(response.audit.macro_set_reports,4);
    await slot.selectOption('2');await idle();assert.equal(await name.inputValue(),'A');checks.push(`${scenario} source slot rereads unchanged original macro`);
    await button('按键').click();await page.getByText('宏槽 '+(expected+1),{exact:true}).first().waitFor();
   }
  }
  assert.deepEqual(errors,[]);await page.screenshot({path:path.join(out,`${scenario}.png`)});await page.close();
 }
 fs.writeFileSync(path.join(out,'capture.json'),JSON.stringify(calls,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,native_memory_transactions:true,physical_HID:false,OS_input:false,native_calls:calls.length,checks},null,2));console.log(`${checks.length} macro scope UI checks passed with ${calls.length} real Rust bridge calls`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});