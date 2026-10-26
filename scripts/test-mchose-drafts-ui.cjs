// Production built React UI; only device IPC is simulated. No HID or OS input output.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mchose-drafts-ui');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true}),checks=[];try{
 const page=await browser.newPage({viewport:{width:1500,height:1150}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(()=>{
  const clone=v=>JSON.parse(JSON.stringify(v));
  const peers=['A','B'].map(unit=>({id:'mchose-'+unit,profile_key:'session:mchose-'+unit,connection_key:'connection:mchose-'+unit+':1',vendor_id:0x3837,product_id:'1014',model_ids:['4026'],name:'A7 V3 Ultra+ '+unit,product:'A7 V3 Ultra+',kind:'mouse',firmware_name:'fixture',connection:'wireless',protocol:'MCHOSE',features:[]}));
  const events=code=>[{kind:'key',code,down:true,delay_ms:0},{kind:'delay',code:0,down:false,delay_ms:25},{kind:'key',code,down:false,delay_ms:0}];
  const state={};
  for(const unit of ['A','B'])state['mchose-'+unit]={profile:0,names:['Native '+unit+' 0','Native '+unit+' 1','Native '+unit+' 2'],rate:3,boards:[0,1,2].map(p=>({dpi_x:Array(6).fill(unit==='A'?800*(p+1):2200),dpi_y:Array(6).fill(unit==='A'?800*(p+1):2200),count:6,active:0,independent:true,sleep:3,never_sleep:false,left_debounce:8,right_debounce:8,sensor:0,rotation:0,lod:2,buttons:[[0,1,0],[0,2,0],[0,4,0],[0,16,0],[0,8,0],[1,0,0,0]]})),macros:unit==='A'?{0:{slot:0,name:'Native macro 0',mode:1,events:events(4)},1:{slot:1,name:'Native macro 1',mode:2,events:events(5)}}:{}};
  window.__mchoseDraftFixture={state,fail:null,hold:null,pending:null};
  const reply=(kind,value)=>{const fixture=window.__mchoseDraftFixture;if(fixture.hold!==kind)return value;fixture.hold=null;return new Promise((resolve,reject)=>{fixture.pending={resolve:()=>resolve(value),reject:()=>reject(Error('AUTHORED_DEFERRED_FAILURE'))};});};
  const hw=id=>{const s=state[id],p=s.profile,b=s.boards[p];return clone({target:{device_key:"session:"+id,connection_key:"connection:"+id+":1"},profile:p,names:s.names,...b,adaptive:[],firmware:'1',receiver_firmware:'1',raw_settings:[p,s.rate<<4,s.rate<<4,b.sleep,Number(b.never_sleep),b.sensor,b.rotation/4,b.left_debounce,b.right_debounce,...Array(10).fill(0)]});};
  const old=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(!['scan_devices','list_devices','mchose_hardware','mchose_apply','mchose_macro','set_report_rate'].includes(cmd))return old(cmd,args);
   window.__qaCalls.push({cmd,args:clone(args)});
   if(cmd==='scan_devices')return {devices:peers,warnings:[]};if(cmd==='list_devices')return peers;
   if(cmd.startsWith('mchose_')&&cmd!=='mchose_macro_file'){
     const target={device_key:'session:'+args.id,connection_key:'connection:'+args.id+':1'};
     if(cmd==='mchose_hardware'){if(args.connectionKey!==target.connection_key)throw Error('wrong discovery connection');}
     else if(JSON.stringify(args.target)!==JSON.stringify(target))throw Error('wrong live target');
    }
    const s=state[args.id];if(!s)throw Error('wrong target');
   if(cmd==='mchose_hardware')return reply('hardware',hw(args.id));
   if(cmd==='mchose_macro')return reply('read_macro',clone(s.macros[args.slot]??null));
   if(cmd==='set_report_rate')throw Error('unbound rate IPC');
   const a=args.action,p=s.profile,b=s.boards[p];
   if(args.expectedProfile!==p)throw Error('wrong board');
   if(window.__mchoseDraftFixture.fail===a.kind){window.__mchoseDraftFixture.fail=null;throw Error('AUTHORED_WRITE_FAILURE');}
   if(a.kind==='dpi')Object.assign(b,{dpi_x:a.x,dpi_y:a.independent?a.y:a.x,count:a.count,active:a.active,independent:a.independent});
   else if(a.kind==='report_rate')s.rate=a.value;
   else if(a.kind==='settings')for(const k of ['sleep','never_sleep','left_debounce','right_debounce','sensor','rotation'])b[k]=a[k];
   else if(a.kind==='button')b.buttons[a.index]=a.binding;
   else if(a.kind==='lod')b.lod=a.index;
   else if(a.kind==='rename')s.names[p]=a.name;
   else if(a.kind==='profile')s.profile=a.index;
   else if(a.kind==='macro')s.macros[a.slot]=clone({slot:a.slot,name:a.name,mode:a.mode,events:a.events});
   else if(a.kind==='delete_macro')delete s.macros[a.slot];
   else throw Error('unknown authored action');
   return reply(a.kind,hw(args.id));
  };
 });
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');
 const tab=name=>page.getByRole('button',{name,exact:true}).click();
 const input=name=>page.getByRole('spinbutton',{name,exact:true});
 const value=async(name,expected)=>assert.equal(await input(name).inputValue(),String(expected));
 const idle=()=>page.waitForFunction(()=>{const field=document.querySelector('fieldset');return field&&!field.disabled;});
 const click=async name=>{await tab(name);await idle();};
 const hold=kind=>page.evaluate(kind=>window.__mchoseDraftFixture.hold=kind,kind);
 const pending=()=>page.waitForFunction(()=>!!window.__mchoseDraftFixture.pending);
 const release=async()=>{await page.evaluate(async()=>{const f=window.__mchoseDraftFixture,p=f.pending;f.pending=null;p.resolve();await Promise.resolve();await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)));});await idle();};
 // Forms remain disabled normally. Temporarily allowing a synthetic later edit
 // exercises snapshot guards independently of that user-interface interlock.
 const allowLaterEdit=()=>page.evaluate(()=>{const field=document.querySelector('fieldset');if(!field?.disabled)throw Error('busy guard absent');field.disabled=false;});

 const refresh=()=>click('重新读取');
 const board=async p=>{await page.getByRole('combobox',{name:'板载配置',exact:true}).selectOption(String(p));await idle();};
 const macroSlot=async slot=>{await page.getByRole('combobox',{name:'宏槽',exact:true}).selectOption(String(slot));await idle();};
 const macroName=()=>page.getByRole('textbox',{name:'宏名称',exact:true}),macroText=()=>page.getByRole('textbox',{name:'宏事件编辑器',exact:true});
 await page.getByRole('button',{name:'Open A7 V3 Ultra+ A',exact:true}).click();await input('档 1 X DPI').waitFor();
 await input('档 1 X DPI').fill('1200');
 await tab('性能');await input('左键消抖').fill('12');
 assert(await page.getByText('A7 V3 Ultra+ 当前原厂界面未开放自动省电回报率；普通休眠设置仍可使用。',{exact:true}).isVisible());
 assert.equal(await page.getByRole('checkbox',{name:'自动省电回报率',exact:true}).count(),0);
 checks.push('A7 OEM adaptive-rate exclusion is visible while ordinary sleep/debounce remain editable');
 await tab('板载/备份');await page.getByRole('textbox',{name:'板载名称',exact:true}).fill('Draft board 0');
 await tab('按键');await click('应用到后侧键');
 await tab('DPI');await value('档 1 X DPI',1200);
 await refresh();await value('档 1 X DPI',1200);
 await click('2000 Hz');await value('档 1 X DPI',1200);
 await tab('性能');await value('左键消抖',12);
 await tab('板载/备份');assert.equal(await page.getByRole('textbox',{name:'板载名称',exact:true}).inputValue(),'Draft board 0');
 checks.push('DPI, performance and name drafts survive button writes, refresh and report-rate updates');
 // A successful section save clears only its own draft: later native changes are
 // visible for the saved section while unrelated unsaved edits remain intact.
 await tab('DPI');await click('保存六档 DPI');
 await page.evaluate(()=>{const s=window.__mchoseDraftFixture.state['mchose-A'];s.boards[0].dpi_x[0]=1800;s.boards[0].left_debounce=9;s.names[0]='External name';});
 await refresh();await value('档 1 X DPI',1800);
 await tab('性能');await value('左键消抖',12);
 await tab('板载/备份');assert.equal(await page.getByRole('textbox',{name:'板载名称',exact:true}).inputValue(),'Draft board 0');
 await tab('DPI');await input('档 1 X DPI').fill('1950');
 await tab('性能');await click('保存性能设置');
 await page.evaluate(()=>window.__mchoseDraftFixture.state['mchose-A'].boards[0].left_debounce=10);
 await refresh();await value('左键消抖',10);
 await tab('DPI');await value('档 1 X DPI',1950);
 await tab('板载/备份');await click('重命名');
 await page.evaluate(()=>window.__mchoseDraftFixture.state['mchose-A'].names[0]='After saved name');
 await refresh();assert.equal(await page.getByRole('textbox',{name:'板载名称',exact:true}).inputValue(),'After saved name');
 await tab('DPI');await value('档 1 X DPI',1950);
 checks.push('successful DPI, performance and rename saves clear only the confirmed section');
 await page.evaluate(()=>window.__mchoseDraftFixture.fail='dpi');await click('保存六档 DPI');
 assert((await page.getByRole('alert').textContent()).includes('AUTHORED_WRITE_FAILURE'));
 await page.evaluate(()=>window.__mchoseDraftFixture.state['mchose-A'].boards[0].dpi_x[0]=2000);
 await refresh();await value('档 1 X DPI',1950);
 checks.push('failed writes do not clear the edited section');
 await hold('dpi');await tab('保存六档 DPI');await pending();await allowLaterEdit();await input('档 1 X DPI').fill('2050');await release();await value('档 1 X DPI',2050);
 await refresh();await value('档 1 X DPI',2050);await input('档 1 X DPI').fill('1950');
 await page.evaluate(()=>window.__mchoseDraftFixture.state['mchose-A'].boards[0].dpi_x[0]=2000);
 checks.push('deferred successful section writes retain edits newer than the submitted draft');

 await board(1);await value('档 1 X DPI',1600);await input('档 1 X DPI').fill('1650');
 await tab('性能');await input('左键消抖').fill('14');
 await tab('板载/备份');await page.getByRole('textbox',{name:'板载名称',exact:true}).fill('Draft board 1');
 await board(2);await tab('DPI');await value('档 1 X DPI',2400);await input('档 1 X DPI').fill('2450');
 await board(0);await value('档 1 X DPI',1950);
 await click('放弃本组草稿并重新读取');await value('档 1 X DPI',2000);
 await board(1);await value('档 1 X DPI',1650);
 await tab('性能');await value('左键消抖',14);
 await tab('板载/备份');assert.equal(await page.getByRole('textbox',{name:'板载名称',exact:true}).inputValue(),'Draft board 1');
 await board(2);await tab('DPI');await value('档 1 X DPI',2450);
 checks.push('boards 0, 1 and 2 keep independent drafts; explicit discard affects only the selected board');
 await tab('宏');await click('读取鼠标');assert.equal(await macroName().inputValue(),'Native macro 0');
 const unfinished='{ unfinished JSON draft';await macroName().fill('Draft macro 0');await page.getByRole('combobox',{name:'宏循环方式',exact:true}).selectOption('3');await macroText().fill(unfinished);
 await macroSlot(1);assert.equal(await macroName().inputValue(),'Native macro 1');
 const sequence=JSON.stringify([{kind:'key',code:6,down:true,delay_ms:0},{kind:'key',code:6,down:false,delay_ms:0}]);
 await macroName().fill('Draft macro 1');await page.getByRole('combobox',{name:'宏循环方式',exact:true}).selectOption('0');await macroText().fill(sequence);
 await macroSlot(0);assert.equal(await macroName().inputValue(),'Draft macro 0');assert.equal(await page.getByRole('combobox',{name:'宏循环方式',exact:true}).inputValue(),'3');assert.equal(await macroText().inputValue(),unfinished);
 await macroSlot(1);assert.equal(await macroName().inputValue(),'Draft macro 1');assert.equal(await macroText().inputValue(),sequence);
 await hold('macro');await tab('保存到宏槽');await pending();assert(await page.locator('fieldset').first().evaluate(el=>el.disabled));await release();
 assert.equal(await macroText().inputValue(),sequence);assert((await page.getByRole('status',{name:'宏槽保存状态'}).textContent()).includes('已核对：Draft macro 1 · 2 条事件'));
 const saved=(await page.evaluate(()=>window.__qaCalls)).filter(c=>c.cmd==='mchose_apply'&&c.args.action.kind==='macro').at(-1);
 assert.equal(saved.args.id,'mchose-A');assert.equal(saved.args.expectedProfile,2);assert.equal(saved.args.action.slot,1);assert.equal(saved.args.action.name,'Draft macro 1');assert.equal(saved.args.action.mode,0);assert.deepEqual(saved.args.action.events,JSON.parse(sequence));
 const formatted=JSON.stringify([{kind:'key',code:7,down:true,delay_ms:0},{kind:'key',code:7,down:false,delay_ms:0},{kind:'key',code:8,down:true,delay_ms:0},{kind:'key',code:8,down:false,delay_ms:0}],null,4);
 await macroName().fill('Pending macro 1');await page.getByRole('combobox',{name:'宏循环方式',exact:true}).selectOption('3');await macroText().fill(formatted);
 await page.evaluate(()=>window.__mchoseDraftFixture.fail='macro');await click('保存到宏槽');
 assert.equal(await macroName().inputValue(),'Pending macro 1');assert.equal(await macroText().inputValue(),formatted);assert.equal(await page.getByRole('combobox',{name:'宏循环方式',exact:true}).inputValue(),'3');assert((await page.getByRole('status',{name:'宏槽保存状态'}).textContent()).includes('未保存草稿'));
 await hold('macro');await tab('保存到宏槽');await pending();await allowLaterEdit();await macroName().fill('Newer macro 1');await page.getByRole('combobox',{name:'宏循环方式',exact:true}).selectOption('2');
 const newerText=formatted.replaceAll('"code": 7','"code": 9');await macroText().fill(newerText);await release();
 assert.equal(await macroName().inputValue(),'Newer macro 1');assert.equal(await macroText().inputValue(),newerText);assert((await page.getByRole('status',{name:'宏槽保存状态'}).textContent()).includes('未保存草稿'));
 await macroSlot(0);assert.equal(await macroText().inputValue(),unfinished);await macroSlot(1);assert.equal(await macroText().inputValue(),newerText);
 await click('保存到宏槽');assert((await page.getByRole('status',{name:'宏槽保存状态'}).textContent()).includes('已核对：Newer macro 1 · 4 条事件'));assert.equal(await macroText().inputValue(),newerText);assert.equal(await page.getByRole('combobox',{name:'宏循环方式',exact:true}).inputValue(),'2');
 checks.push('deferred macro saves confirm submitted parsed events without reformatting text; failures and newer local edits retain their drafts');
 await hold('delete_macro');await tab('删除空闲宏槽');await pending();await allowLaterEdit();await macroSlot(0);assert.equal(await macroName().inputValue(),'Draft macro 0');await release();
 assert.equal(await macroName().inputValue(),'Draft macro 0');assert.equal(await macroText().inputValue(),unfinished);
 await macroSlot(1);assert.equal(await macroText().inputValue(),'[]');assert.equal(await macroName().inputValue(),'新宏 2');assert.equal(await page.getByRole('combobox',{name:'宏循环方式',exact:true}).inputValue(),'2');
 checks.push('deferred macro deletion clears only its captured slot and cannot publish old data onto another selected slot');

 await page.evaluate(()=>window.__mchoseDraftFixture.state['mchose-A'].macros[1]={slot:1,name:'Fresh native macro 1',mode:1,events:[]});
 await click('读取鼠标');assert.equal(await macroName().inputValue(),'Fresh native macro 1');
 await macroSlot(0);assert.equal(await macroName().inputValue(),'Draft macro 0');assert.equal(await macroText().inputValue(),unfinished);
 checks.push('macro slots preserve separate names, modes and exact event text; explicit native reload replaces only its slot');
 await macroSlot(1);await hold('read_macro');await tab('读取鼠标');await pending();await allowLaterEdit();await macroName().fill('Local after read started');await macroText().fill(newerText);await macroSlot(0);await macroName().fill('Local macro 0');await release();
 assert.equal(await macroName().inputValue(),'Local macro 0');assert.equal(await page.getByRole('combobox',{name:'宏槽',exact:true}).inputValue(),'0');
 await macroSlot(1);assert.equal(await macroName().inputValue(),'Local after read started');assert.equal(await macroText().inputValue(),newerText);assert((await page.getByRole('status',{name:'宏槽保存状态'}).textContent()).includes('未保存草稿'));
 checks.push('deferred reads preserve newer slot edits and never redirect a later slot selection');

 await page.getByRole('button',{name:'Back to devices',exact:true}).click();await page.getByRole('button',{name:'Open A7 V3 Ultra+ B',exact:true}).click();await input('档 1 X DPI').waitFor();await value('档 1 X DPI',2200);
 await tab('性能');await value('左键消抖',8);
 await tab('板载/备份');assert.equal(await page.getByRole('textbox',{name:'板载名称',exact:true}).inputValue(),'Native B 0');
 await tab('宏');assert.equal(await macroText().inputValue(),'[]');assert.notEqual(await macroName().inputValue(),'Draft macro 0');
 checks.push('drafts belong to the mounted physical target and never appear on another mouse');
 assert.deepEqual(errors,[]);await page.screenshot({path:path.join(out,'independent-target-drafts.png')});
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,mock_IPC:true,hardware_tested:false,protocol_semantics_changed:false,checks},null,2));console.log(`${checks.length} MCHOSE draft UI groups passed; no physical I/O`);
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
