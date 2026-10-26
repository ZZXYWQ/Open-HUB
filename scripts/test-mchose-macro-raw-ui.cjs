// Built production React UI. Only IPC/file dialogs are simulated; no HID or OS input.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const source=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=source.indexOf(marker)+marker.length,end=source.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mchose-macro-raw-ui');fs.mkdirSync(out,{recursive:true});
const hardware=JSON.parse(fs.readFileSync('src-tauri/tests/fixtures/mchose.json','utf8')).state;
hardware.adaptive=[]; // Current A7 capability set, distinct from legacy diagnostic bytes.
(async()=>{const browser=await chromium.launch({headless:true}),checks=[];try{
 const page=await browser.newPage({viewport:{width:1500,height:1150}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${source.slice(start,end)}})()`});
 await page.addInitScript(hardware=>{
  const clone=x=>JSON.parse(JSON.stringify(x)),events=[{kind:'key',code:4,down:true,delay_ms:0},{kind:'delay',code:0,down:false,delay_ms:37},{kind:'key',code:4,down:false,delay_ms:0}];
  const encode=m=>{const raw=Array(64).fill(0),name=Array.from(new TextEncoder().encode(m.name)),length=m.events.length*4;raw[0]=length&255;raw[1]=length>>8;raw[4]=m.mode;raw.splice(8,name.length,...name);for(const [i,e]of m.events.entries()){const kind={key:17,delay:63,mouse:0,scroll:5}[e.kind];raw.push(e.delay_ms&255,e.delay_ms>>8,kind|(e.down?64:0)|(i===m.events.length-1?128:0),e.code);}return raw;};
  const canonical={slot:1,name:'A',mode:1,events:clone(events)};canonical.raw=encode(canonical);
  const opaque={slot:0,name:'A',mode:1,events:clone(events),raw:[...canonical.raw]};opaque.raw.splice(2,2,0xa5,0x17);opaque.raw.splice(5,3,0x91,0x82,0x73);opaque.raw[64]=29;opaque.raw[66]|=128;opaque.events[0].delay_ms=29;
  const state={'mchose-A':{0:opaque,1:canonical},'mchose-B':{}},capture={actions:[],exports:[]};
  window.__rawMacro={state,capture,opaque:clone(opaque),hold:false,pending:null};
  const peers=['A','B'].map(unit=>({id:'mchose-'+unit,profile_key:'session:mchose-'+unit,connection_key:'connection:mchose-'+unit+':1',vendor_id:0x3837,product_id:'1014',model_ids:['4026'],name:'A7 V3 Ultra+ '+unit,product:'A7 V3 Ultra+',kind:'mouse',firmware_name:'fixture',connection:'wireless',protocol:'MCHOSE',features:[]}));
  const old=window.__TAURI_INTERNALS__.invoke;
  const record=m=>{
   const original=m.raw;
   if(original&&original[2]===0xa5){if(m.name!=='A'||m.mode!==1||JSON.stringify(m.events)!==JSON.stringify(opaque.events))throw Error('原始宏含未解释字段，不能无损修改');return clone(m);}
   return {...clone(m),raw:encode(m)};
  };
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(!['scan_devices','list_devices','mchose_hardware','mchose_macro','mchose_apply','mchose_macro_file','plugin:dialog|save'].includes(cmd))return old(cmd,args);
   window.__qaCalls.push({cmd,args:clone(args)});
   if(cmd==='scan_devices')return {devices:peers,warnings:[]};if(cmd==='list_devices')return peers;
   if(cmd.startsWith('mchose_')&&cmd!=='mchose_macro_file'){
     const target={device_key:'session:'+args.id,connection_key:'connection:'+args.id+':1'};
     if(cmd==='mchose_hardware'){if(args.connectionKey!==target.connection_key)throw Error('wrong discovery connection');}
     else if(JSON.stringify(args.target)!==JSON.stringify(target))throw Error('wrong live target');
    }
    if(cmd==='plugin:dialog|save')return 'authored-macro-export.json';
   if(cmd==='mchose_hardware')return {...clone(hardware),target:clone(args.connectionKey?{device_key:'session:'+args.id,connection_key:args.connectionKey}:null)};
   if(cmd==='mchose_macro')return clone(state[args.id][args.slot]??null);
   if(cmd==='mchose_macro_file'){const m=record(args);capture.exports.push({args:clone(args),expected_raw:m.raw});return null;}
   if(args.expectedProfile!==0||args.action.kind!=='macro')throw Error('unexpected authored target/action');
   const request={...clone(args),expected_success:false};capture.actions.push(request);
   const receipt=record(args.action);request.expected_success=true;request.expected_raw=clone(receipt.raw);
   // A receipt belongs to its target slot and contains the newly verified bytes.
   delete receipt.kind;state[args.id][receipt.slot]=clone(receipt);
   const value={...clone(hardware),target:clone(args.target),saved_macro:receipt};
   if(window.__rawMacro.hold){window.__rawMacro.hold=false;return new Promise(resolve=>window.__rawMacro.pending=()=>resolve(value));}
   return value;
  };
 },hardware);
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 const button=name=>page.getByRole('button',{name,exact:true});
 const idle=()=>page.waitForFunction(()=>{const f=document.querySelector('fieldset');return f&&!f.disabled;});
 const click=async name=>{await button(name).click();await idle();};
 const macroName=()=>page.getByRole('textbox',{name:'宏名称',exact:true}),macroText=()=>page.getByRole('textbox',{name:'宏事件编辑器',exact:true});
 const select=async slot=>{await page.getByRole('combobox',{name:'宏槽',exact:true}).selectOption(String(slot));await idle();};
 await button('Open A7 V3 Ultra+ A').click();await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).waitFor();await click('宏');await click('读取鼠标');
 const original=await page.evaluate(()=>window.__rawMacro.opaque);
 await click('保存到宏槽');
 let capture=await page.evaluate(()=>window.__rawMacro.capture);
 assert.deepEqual(capture.actions[0].action.raw,original.raw);assert(capture.actions[0].expected_success);
 await click('导出宏');capture=await page.evaluate(()=>window.__rawMacro.capture);
 assert.deepEqual(capture.exports[0].args.raw,original.raw);
 checks.push('native-read opaque header, non-time word and terminal flags survive unchanged save and export IPC');
 await macroName().fill('Unsupported opaque edit');await click('保存到宏槽');
 assert((await page.getByRole('alert').textContent()).includes('不能无损修改'));assert.equal(await macroName().inputValue(),'Unsupported opaque edit');
 capture=await page.evaluate(()=>window.__rawMacro.capture);assert.equal(capture.actions.at(-1).expected_success,false);assert.deepEqual(capture.actions.at(-1).action.raw,original.raw);
 checks.push('opaque-edit rejection keeps the edited name, event text and original bytes');
 await select(2);await page.locator('input[type=file]').setInputFiles({name:'preserved.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(original))});await idle();
 assert.equal(await macroName().inputValue(),'A');await click('保存到宏槽');
 capture=await page.evaluate(()=>window.__rawMacro.capture);assert.equal(capture.actions.at(-1).action.slot,2);assert.deepEqual(capture.actions.at(-1).action.raw,original.raw);
 checks.push('file import into another selected slot retains exact source bytes and replaces only the selected slot');
 const modes=page.getByRole('combobox',{name:'宏循环方式',exact:true});
 assert.deepEqual(await modes.locator('option').allTextContents(),['执行一次','按住时重复','循环至再次按下','循环至其他按键按下']);
 await select(3);assert.equal(await modes.inputValue(),'2');
 checks.push('official localized mode2 renders as execute once and a new empty slot defaults to it');
 await macroName().fill('Mode choices');await macroText().fill(JSON.stringify([{kind:'key',code:4,down:true,delay_ms:0},{kind:'key',code:4,down:false,delay_ms:0}]));
 for(const mode of [2,0,1,3]){
  await modes.selectOption(String(mode));assert((await page.getByLabel('宏模式说明').textContent()).length>0);await click('保存到宏槽');
  capture=await page.evaluate(()=>window.__rawMacro.capture);assert.equal(capture.actions.at(-1).action.mode,mode);assert.equal(capture.actions.at(-1).expected_raw[4],mode);
 }
 checks.push('all four displayed playback modes submit and retain their independent native header value');
 await select(1);await macroName().fill('Edited');await page.getByRole('combobox',{name:'宏循环方式',exact:true}).selectOption('3');
 const edited=JSON.parse(await macroText().inputValue());edited[1].delay_ms=81;const formatting=JSON.stringify(edited);await macroText().fill(formatting);await click('保存到宏槽');
 assert.equal(await macroText().inputValue(),formatting);assert((await page.getByLabel('宏槽保存状态').textContent()).includes('已核对：Edited'));
 capture=await page.evaluate(()=>window.__rawMacro.capture);const confirmed=capture.actions.at(-1).expected_raw;
 await macroName().fill('Edited again');await click('保存到宏槽');capture=await page.evaluate(()=>window.__rawMacro.capture);assert.deepEqual(capture.actions.at(-1).action.raw,confirmed);
 await click('导出宏');capture=await page.evaluate(()=>window.__rawMacro.capture);assert.deepEqual(capture.exports.at(-1).args.raw,capture.actions.at(-1).expected_raw);
 checks.push('canonical edits use the native saved-macro receipt as the next source baseline and retain JSON formatting');
 // New receipt handling must keep the established mount/target lifetime guard.
 await page.evaluate(()=>window.__rawMacro.hold=true);await button('保存到宏槽').click();await page.waitForFunction(()=>!!window.__rawMacro.pending);
 await button('Back to devices').click();await button('Open A7 V3 Ultra+ B').click();await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).waitFor();await click('宏');
 await page.evaluate(async()=>{const f=window.__rawMacro.pending;window.__rawMacro.pending=null;f();await Promise.resolve();await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)));});
 assert.equal(await macroText().inputValue(),'[]');assert.notEqual(await macroName().inputValue(),'Edited again');assert.equal(await modes.inputValue(),'2');
 checks.push('a late saved-macro receipt from mouse A cannot populate mouse B');
 assert.deepEqual(errors,[]);capture=await page.evaluate(()=>window.__rawMacro.capture);fs.writeFileSync(path.join(out,'capture.json'),JSON.stringify(capture,null,2));
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,mock_IPC:true,physical_HID:false,OS_input:false,actions:capture.actions.length,exports:capture.exports.length,checks},null,2));
 console.log(`${checks.length} MCHOSE macro raw UI groups passed; ${capture.actions.length} actual apply requests and ${capture.exports.length} file requests exported`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
