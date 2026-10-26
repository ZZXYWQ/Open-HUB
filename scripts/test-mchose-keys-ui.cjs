// Actual built MCHOSE UI. All device IPC and keyboard DOM events are authored;
// no HID access, OEM execution or OS input injection occurs.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mchose-keys-ui');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true}),checks=[];try{
 const page=await browser.newPage({viewport:{width:1500,height:1150}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(()=>{
  const clone=v=>JSON.parse(JSON.stringify(v));
  const peers=['A','B'].map(unit=>({id:'mchose-'+unit,profile_key:'session:mchose-'+unit,connection_key:'connection:mchose-'+unit+':1',vendor_id:0x3837,product_id:'1014',model_ids:['4026'],name:'A7 V3 Ultra+ '+unit,product:'A7 V3 Ultra+',kind:'mouse',firmware_name:'fixture',connection:'wireless',protocol:'MCHOSE',features:[]}));
  const states={};for(const unit of ['A','B'])states['mchose-'+unit]={target:{device_key:'session:mchose-'+unit,connection_key:'connection:mchose-'+unit+':1'},profile:0,names:['Board0','Board1','Board2'],dpi_x:Array(6).fill(unit==='A'?800:1600),dpi_y:Array(6).fill(unit==='A'?800:1600),count:6,active:0,independent:true,sleep:3,never_sleep:false,left_debounce:8,right_debounce:8,sensor:0,rotation:0,lod:2,buttons:[[0,1,0],[0,2,0],[0,4,0],[0,16,0],[0,8,0],[255,255,255]],adaptive:[],firmware:'17',receiver_firmware:'1',raw_settings:[0,0x30,0x30,3,0,0,0,8,8,...Array(10).fill(0)]};
  window.__mchoseKeysStates=states;
  const old=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(!['scan_devices','list_devices','mchose_hardware','mchose_apply','mchose_macro'].includes(cmd))return old(cmd,args);
   window.__qaCalls.push({cmd,args:clone(args)});
   if(cmd==='scan_devices')return {devices:peers,warnings:[]};if(cmd==='list_devices')return peers;
   if(cmd.startsWith('mchose_')&&cmd!=='mchose_macro_file'){
     const target={device_key:'session:'+args.id,connection_key:'connection:'+args.id+':1'};
     if(cmd==='mchose_hardware'){if(args.connectionKey!==target.connection_key)throw Error('wrong discovery connection');}
     else if(JSON.stringify(args.target)!==JSON.stringify(target))throw Error('wrong live target');
    }
    if(cmd==='mchose_macro')return null;
   const state=states[args.id];if(!state)throw Error('wrong target');
   if(cmd==='mchose_apply'){if(args.expectedProfile!==state.profile||args.action.kind!=='button'||args.action.index!==4)throw Error('unexpected authored operation');state.buttons[4]=clone(args.action.binding);}
   return clone(state);
  };
 });
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 const tab=name=>page.getByRole('button',{name,exact:true}).click();
 const idle=()=>page.waitForFunction(()=>{const field=document.querySelector('fieldset');return field&&!field.disabled;});
 const open=async unit=>{await page.getByRole('button',{name:'Open A7 V3 Ultra+ '+unit,exact:true}).click();await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).waitFor();await tab('按键');};
 const key=()=>page.getByRole('combobox',{name:'自定义主键',exact:true});
 const modifiers=[['Ctrl（通用）',1],['Shift（通用）',2],['Alt（通用）',4],['Win（通用）',8]];
 const mask=async value=>{for(const [name,bit]of modifiers)await page.getByRole('checkbox',{name,exact:true}).setChecked(Boolean(value&bit));};
 const requests=()=>page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='mchose_apply').map(c=>c.args));
 const apply=async expected=>{await tab('应用到后侧键');await idle();const last=(await requests()).at(-1);assert.deepEqual(last.action.binding,expected);assert.equal(last.action.index,4);assert.equal(last.expectedProfile,0);};
 await open('A');await tab('录入键盘键 / 组合键');
 const expectedUsages=Array.from({length:96},(_,i)=>i+4).filter(value=>value!==50).concat(101);
 const choices=await key().locator('option').evaluateAll(options=>options.map(option=>({value:Number(option.value),label:option.textContent})));
 assert.deepEqual(choices.filter(choice=>choice.value!==0).map(choice=>choice.value).sort((a,b)=>a-b),expectedUsages.concat([224,225,226,227,228,229,230,231]));assert.equal(new Set(choices.map(choice=>choice.value)).size,105);
 assert(!choices.some(choice=>/^F(?:1[3-9]|2[0-4])$/.test(choice.label)));
 assert.equal(choices.find(choice=>choice.value===40).label,'Enter');assert.equal(choices.find(choice=>choice.value===88).label,'数字键盘 Enter');assert.equal(choices.find(choice=>choice.value===101).label,'菜单键');
 for(const usage of expectedUsages){await key().selectOption(String(usage));await apply([0x11,usage,0]);}
 checks.push('all 96 proven nonmodifier choices render once and each submits exact plain-key11 IPC');
 await mask(3);
 for(const usage of [224,225,226,227,228,229,230,231]){await key().selectOption(String(usage));for(const [name]of modifiers){const box=page.getByRole('checkbox',{name,exact:true});assert(await box.isDisabled());assert(!(await box.isChecked()));}await apply([0x11,usage,0]);}
 checks.push('all eight OEM standalone left/right modifiers clear chord masks and submit distinct type11 E0..E7');

 for(const [usage,mods]of [[6,1],[41,3],[88,15]]){await key().selectOption(String(usage));await mask(mods);await apply([0x13,mods,usage]);}
 checks.push('general Ctrl/C, multimore modifier and numpad combinations submit type13 with low-four-bit mask');
 await key().selectOption('0');await mask(0);assert(await page.getByRole('button',{name:'应用到后侧键',exact:true}).isDisabled());
 for(const mods of [1,9,15]){await mask(mods);await apply([0x12,mods,0]);}
 checks.push('modifier-only choices submit type12 and an empty selection cannot submit a stale preset');
 const capture=page.getByRole('textbox',{name:'录入组合键',exact:true});
 await capture.dispatchEvent('keydown',{code:'ControlRight',key:'Control',ctrlKey:true});await capture.dispatchEvent('keydown',{code:'KeyC',key:'c',ctrlKey:true});
 assert(await page.getByRole('checkbox',{name:'Ctrl（通用）',exact:true}).isChecked());assert((await page.getByRole('status',{name:'自定义键盘预览'}).textContent()).includes('Ctrl（通用） + C'));assert((await page.getByRole('group',{name:'自定义键盘组合'}).textContent()).includes('不区分左右'));await apply([0x13,1,6]);
 await capture.dispatchEvent('keydown',{code:'NumpadEnter',key:'Enter'});await apply([0x11,88,0]);
 await capture.dispatchEvent('keydown',{code:'Insert',key:'Insert'});await apply([0x11,73,0]);
 await capture.dispatchEvent('keydown',{code:'F13',key:'F13'});assert(await page.getByRole('button',{name:'应用到后侧键',exact:true}).isDisabled());assert((await page.getByRole('status',{name:'自定义键盘预览'}).textContent()).includes('原厂键表未列出此键'));
 checks.push('DOM capture visibly uses generic modifiers, distinguishes numpad Enter and navigation, and does not invent F13');
 for(const [label,usage]of [['复制',6],['粘贴',25]]){const value=await page.getByRole('combobox',{name:'选择按键动作',exact:true}).locator('option').filter({hasText:new RegExp('^'+label)}).getAttribute('value');await page.getByRole('combobox',{name:'选择按键动作',exact:true}).selectOption(value);await apply([0x16,1,usage]);}
 checks.push('named Copy and Paste presets retain their original type16 encoding');
 for(const [label,expected]of [['停止播放',[0x14,183,0]],['屏幕亮度 +',[0x14,111,0]],['屏幕亮度 −',[0x14,112,0]],['全选',[0x13,1,4]]]){const select=page.getByRole('combobox',{name:'选择按键动作',exact:true}),option=select.locator('option').filter({hasText:label});assert.equal(await option.count(),1);await select.selectOption(await option.getAttribute('value'));await apply(expected);}
 checks.push('Stop and brightness presets use source consumer codes, Select All uses OEM type13 rather than invented type16');

 // Leave an unapplied generic chord in A's component before navigating away.
 await tab('录入键盘键 / 组合键');await key().selectOption('89');await mask(3);await page.getByRole('group',{name:'自定义键盘组合',exact:true}).screenshot({path:path.join(out,'general-chord-builder.png')});
 await tab('宏');await tab('录制键盘宏');
 const recording=page.getByRole('textbox',{name:'宏录制区域',exact:true});
 const recorded=[['ControlLeft','Control',224],['ShiftLeft','Shift',225],['AltLeft','Alt',226],['MetaLeft','Meta',227],['ControlRight','Control',228],['ShiftRight','Shift',229],['AltRight','Alt',230],['MetaRight','Meta',231],['NumpadEnter','Enter',88],['Insert','Insert',73],['ContextMenu','ContextMenu',101]];
 for(const [code,keyName]of recorded){await recording.dispatchEvent('keydown',{code,key:keyName,repeat:false});await recording.dispatchEvent('keyup',{code,key:keyName});}
 await tab('停止录制');const macroEvents=JSON.parse(await page.getByRole('textbox',{name:'宏事件编辑器',exact:true}).inputValue());
 for(const [,,usage]of recorded)assert.deepEqual(macroEvents.filter(event=>event.kind==='key'&&event.code===usage).map(event=>event.down),[true,false]);
 assert.deepEqual([...new Set(macroEvents.filter(event=>event.kind==='key'&&event.code>=224).map(event=>event.code))].sort((a,b)=>a-b),[224,225,226,227,228,229,230,231]);
 fs.writeFileSync(path.join(out,'macro-events.json'),JSON.stringify(macroEvents,null,2));
 checks.push('macro recording retains all eight distinct E0..E7 usages and balanced navigation/numpad events');
 await page.getByRole('button',{name:'Back to devices',exact:true}).click();await open('B');assert.equal(await page.getByRole('group',{name:'自定义键盘组合'}).count(),0);await tab('录入键盘键 / 组合键');assert.equal(await key().inputValue(),'4');for(const [name]of modifiers)assert(!(await page.getByRole('checkbox',{name,exact:true}).isChecked()));await key().selectOption('88');await apply([0x11,88,0]);assert.equal((await requests()).at(-1).id,'mchose-B');await tab('宏');assert.equal(await page.getByRole('textbox',{name:'宏事件编辑器',exact:true}).inputValue(),'[]');
 checks.push('unapplied keyboard and recorded macro drafts never move to another physical target');
 const actions=await requests();assert(actions.slice(0,-1).every(action=>action.id==='mchose-A'));assert.equal(actions.length,120);assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(out,'actions.json'),JSON.stringify(actions,null,2));
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,mock_IPC:true,hardware_tested:false,OS_input_injected:false,keyboard_unique_usages:104,nonmodifier_choices:96,standalone_modifier_choices:8,request_count:actions.length,actions_schema:'Array<{id:string,target:{device_key:string,connection_key:string},expectedProfile:number,action:{kind:"button",index:number,binding:number[]}}>',checks},null,2));await page.screenshot({path:path.join(out,'keyboard-and-target-isolation.png')});console.log(`${checks.length} MCHOSE keyboard UI groups passed; ${actions.length} actual IPC actions exported; no physical I/O`);
}finally{await browser.close();}})().catch(error=>{console.error(error);process.exitCode=1;});
