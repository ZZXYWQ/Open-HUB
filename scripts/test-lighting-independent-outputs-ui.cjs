// Actual built UI, stateful IPC simulation, no RGB/audio/HID access.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/lighting-independent-outputs-ui');
fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8');
const marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);
assert(end>start);
(async()=>{
 const browser=await chromium.launch({headless:true});
 try{
  const page=await browser.newPage({viewport:{width:1450,height:1120}}),errors=[];
  page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
  await page.addInitScript(()=>{
   const delayedDock=new URL(location.href).searchParams.has('delayedDock');
   localStorage.setItem('lighting-auto-mouse',String(delayedDock));
   localStorage.setItem('lighting-dock-excluded',String(!delayedDock));
   const pc={id:2,key:'ViewSonic|Monitor|serial-C',name:'XG270QG',vendor:'ViewSonic',kind:20,leds:12,direct:true};
   const config={kind:'gradient',style:0,color:[12,34,56],palette:[[12,34,56],[78,90,123]],brightness:.6,speed:1.5,gain:2,dock:false,rgb:[{id:pc.id,key:pc.key}]};
   const factory={enabled:true,mode:4,mode_count:6,speed:1,brightness:1,sync:false,color:[200,40,10],direction:1};
   const clone=v=>JSON.parse(JSON.stringify(v));
   const qa=window.__independentQa={
    generation:17,dockReads:0,rejectMusic:false,attached:!delayedDock,
    holdNextStatus:false,holdNextDock:false,heldStatus:[],heldDock:[],
    status:{running:true,kind:config.kind,level:.12,pixels:Array(12).fill([12,34,56]),error:null,config:clone(config)},
    dock:{firmware:'1.0',music_running:false,audio_level:0,music_error:null,light:clone(factory)},
    before:clone(factory),initialConfig:clone(config)
   };
   const interval=window.setInterval.bind(window),discovery=[];
   window.setInterval=(callback,delay,...args)=>{
    if(delay===12000)discovery.push(()=>callback(...args));
    return interval(callback,delay,...args);
   };
   // Trigger the actual 12-second callbacks deterministically, without a
   // wall-clock delay or changing their production scheduling behavior.
   qa.tickDiscovery=()=>discovery.forEach(callback=>callback());
   const stopDock=()=>{if(qa.dock.music_running)qa.dock.light=clone(qa.before);qa.dock.music_running=false;qa.dock.audio_level=0;};
   const stopDockOwnedEngine=()=>{if(qa.status.running&&qa.status.config.dock)qa.status.running=false;};
   const old=window.__TAURI_INTERNALS__.invoke;
   window.__TAURI_INTERNALS__.invoke=async(cmd,a={})=>{
    if(!['rgb_devices','magdock_state','magdock_music','magdock_apply','magdock_reset','effects_status','effects_start','effects_stop'].includes(cmd))return old(cmd,a);
    window.__qaCalls.push({cmd,args:clone(a)});
    switch(cmd){
     case'rgb_devices':return[pc];
     case'magdock_state':{
      qa.dockReads++;if(!qa.attached)throw Error('simulated dock disconnected');
      const snapshot=clone(qa.dock);
      if(qa.holdNextDock){qa.holdNextDock=false;return new Promise(resolve=>qa.heldDock.push(()=>resolve(snapshot)));}
      return snapshot;
     }
     case'effects_status':{
      const snapshot=clone(qa.status);
      if(qa.holdNextStatus){qa.holdNextStatus=false;return new Promise(resolve=>qa.heldStatus.push(()=>resolve(snapshot)));}
      return snapshot;
     }
     case'effects_start':if(a.config.dock)stopDock();qa.generation++;qa.status={...qa.status,running:true,kind:a.config.kind,config:clone(a.config)};return clone(qa.status);
     case'effects_stop':qa.status.running=false;return clone(qa.status);
     case'magdock_music':
      if(a.enabled){
       stopDockOwnedEngine();stopDock();qa.before=clone(qa.dock.light);
       if(qa.rejectMusic){qa.dock.music_error='audio meter unavailable';throw Error('audio meter unavailable');}
       qa.dock={...qa.dock,music_running:true,audio_level:.37,music_error:null,light:{...clone(a.light),enabled:true,mode:5,sync:true}};
      }else stopDock();
      return clone(qa.dock);
     case'magdock_apply':stopDockOwnedEngine();stopDock();qa.dock={...qa.dock,light:clone(a.light),music_error:null};return clone(qa.dock);
     case'magdock_reset':stopDockOwnedEngine();stopDock();qa.dock={...qa.dock,light:clone(factory),music_error:null};return clone(qa.dock);
    }
   };
  });
  await require('./ui-static-route.cjs')(page,root);
  await page.goto('http://127.0.0.1:1431');
  await page.getByRole('button',{name:'灯光同步',exact:true}).click();
  const panel=page.locator('.lighting-studio'),dock=panel.getByRole('region',{name:'独立底座灯光',exact:true});
  await panel.getByRole('button',{name:'应用并重新同步',exact:true}).waitFor();
  const pcTarget=panel.locator('label.hub-device-card').filter({hasText:'XG270QG'}).getByRole('checkbox');
  const dockStatus=dock.getByRole('status',{name:'底座律动状态',exact:true});
  const assertPc=async(generation=17)=>{
   const current=await page.evaluate(()=>({status:window.__independentQa.status,generation:window.__independentQa.generation,initial:window.__independentQa.initialConfig}));
   assert(current.status.running,'PC RGB must remain running');
   assert.equal(current.generation,generation,'dock edits must not replace the PC worker');
   assert.deepEqual(current.status.config,current.initial,'PC output selection and effect parameters must survive');
   assert(await pcTarget.isChecked(),'PC selection must remain checked');
   assert(await panel.getByRole('button',{name:'应用并重新同步',exact:true}).isVisible());
  };
  await assertPc();
  await dock.getByRole('combobox',{name:'原厂灯效模式',exact:true}).selectOption('5');
  await dock.getByLabel('底座灯光颜色',{exact:true}).fill('#2468ac');
  await dock.getByRole('slider',{name:'底座亮度',exact:true}).press('End');
  await dock.getByRole('slider',{name:'底座律动灵敏度',exact:true}).press('End');
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动运行中'}).waitFor();
  await assertPc();
  const music=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='magdock_music').at(-1));
  assert.equal(music.args.enabled,true);assert.equal(music.args.gain,8);
  assert.equal(music.args.light.mode,5);assert.equal(music.args.light.brightness,4);
  assert.deepEqual(music.args.light.color,[36,104,172]);
  assert.equal(await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='effects_start').length),0);

  // Background dock status must update without replacing an editable color draft.
  await dock.getByLabel('底座灯光颜色',{exact:true}).fill('#abcdef');
  const reads=await page.evaluate(()=>{window.__independentQa.dock.audio_level=.62;return window.__independentQa.dockReads;});
  await page.waitForFunction(n=>window.__independentQa.dockReads>n,reads);
  await dockStatus.filter({hasText:'62%'}).waitFor();
  assert.equal(await dock.getByLabel('底座灯光颜色',{exact:true}).inputValue(),'#abcdef');
  await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动已停止'}).waitFor();
  assert(await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).isDisabled());
  assert.equal(await dock.getByRole('combobox',{name:'原厂灯效模式',exact:true}).inputValue(),'4');
  await assertPc();

  await dock.getByRole('combobox',{name:'原厂灯效模式',exact:true}).selectOption('0');
  await dock.getByLabel('底座灯光颜色',{exact:true}).fill('#3579bd');
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await page.waitForFunction(()=>window.__qaCalls.some(c=>c.cmd==='magdock_apply'));
  await assertPc();
  await dock.getByRole('combobox',{name:'原厂灯效模式',exact:true}).selectOption('5');
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动运行中'}).waitFor();
  await dock.getByRole('button',{name:'恢复原厂灯效',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动已停止'}).waitFor();
  await assertPc();

  // Explicit PC stop/start must also preserve the independently running dock.
  await dock.getByRole('combobox',{name:'原厂灯效模式',exact:true}).selectOption('5');
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动运行中'}).waitFor();
  await panel.getByRole('button',{name:'停止并恢复',exact:true}).click();
  await panel.getByRole('button',{name:'开始同步',exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>window.__independentQa.status.running),false);
  assert.equal(await page.evaluate(()=>window.__independentQa.dock.music_running),true);
  assert(await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).isEnabled());
  await panel.getByRole('button',{name:'开始同步',exact:true}).click();
  await panel.getByRole('button',{name:'应用并重新同步',exact:true}).waitFor();
  await assertPc(18);
  assert.equal(await page.evaluate(()=>window.__independentQa.dock.music_running),true);

  // An audio failure belongs to the dock card; PC state and draft stay intact.
  await page.evaluate(()=>window.__independentQa.rejectMusic=true);
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await dock.getByRole('alert',{name:'底座灯光错误',exact:true}).filter({hasText:'audio meter unavailable'}).waitFor();
  await assertPc(18);
  assert.equal(await panel.locator(':scope > .lighting-layout > .lighting-controls > [role="alert"]').count(),0);
  await page.evaluate(()=>window.__independentQa.rejectMusic=false);
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动运行中'}).waitFor();
  assert.equal(await dock.getByRole('alert',{name:'底座灯光错误',exact:true}).count(),0);
  await assertPc(18);
  await page.waitForFunction(()=>!document.querySelector('[data-sonner-toast]'),null,{timeout:10000});
  await dock.scrollIntoViewIfNeeded();
  await page.screenshot({path:path.join(out,'pc-rgb-and-independent-dock-music.png'),fullPage:true});
  const calls=await page.evaluate(()=>window.__qaCalls);
  assert.equal(calls.filter(c=>c.cmd==='effects_start').length,1,'only explicit PC restart starts the shared engine');
  assert.equal(calls.filter(c=>c.cmd==='effects_stop').length,1,'only explicit PC stop stops the shared engine');
  assert(calls.some(c=>c.cmd==='magdock_music'&&!c.args.enabled));
  assert(calls.some(c=>c.cmd==='magdock_apply'));assert(calls.some(c=>c.cmd==='magdock_reset'));

  // Resolve old poll replies after successful mutations; they must never
  // resurrect stopped workers or hide newly started ones in the actual UI.
  const release=async(field)=>page.evaluate(async field=>{
   window.__independentQa[field].shift()();
   await new Promise(resolve=>setTimeout(resolve,0));
   await new Promise(requestAnimationFrame);
  },field);
  await page.evaluate(()=>window.__independentQa.holdNextStatus=true);
  await page.waitForFunction(()=>window.__independentQa.heldStatus.length===1);
  await panel.getByRole('button',{name:'停止并恢复',exact:true}).click();
  await panel.getByRole('button',{name:'开始同步',exact:true}).waitFor();
  await release('heldStatus');
  assert(await panel.getByRole('button',{name:'开始同步',exact:true}).isVisible());
  assert(await panel.getByRole('button',{name:'停止并恢复',exact:true}).isDisabled());
  await page.evaluate(()=>window.__independentQa.holdNextStatus=true);
  await page.waitForFunction(()=>window.__independentQa.heldStatus.length===1);
  await panel.getByRole('button',{name:'开始同步',exact:true}).click();
  await panel.getByRole('button',{name:'应用并重新同步',exact:true}).waitFor();
  await release('heldStatus');
  await assertPc(19);

  await page.evaluate(()=>window.__independentQa.holdNextDock=true);
  await page.waitForFunction(()=>window.__independentQa.heldDock.length===1);
  await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动已停止'}).waitFor();
  await release('heldDock');
  assert(await dockStatus.filter({hasText:'底座音乐律动已停止'}).isVisible());
  assert(await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).isDisabled());
  await assertPc(19);
  await page.evaluate(()=>{window.__independentQa.holdNextDock=true;window.__independentQa.tickDiscovery();});
  await page.waitForFunction(()=>window.__independentQa.heldDock.length===1);
  await dock.getByRole('combobox',{name:'原厂灯效模式',exact:true}).selectOption('5');
  await dock.getByRole('button',{name:'应用到底座',exact:true}).click();
  await dockStatus.filter({hasText:'底座音乐律动运行中'}).waitFor();
  await release('heldDock');
  assert(await dockStatus.filter({hasText:'底座音乐律动运行中'}).isVisible());
  assert(await dock.getByRole('button',{name:'停止底座律动并恢复',exact:true}).isEnabled());
  await assertPc(19);

  // A dock plugged in after PC sync started must appear on the next discovery
  // callback, without selecting it for PC sync or replacing either draft.
  await page.goto('http://127.0.0.1:1431/?delayedDock=1');
  await page.getByRole('button',{name:'灯光同步',exact:true}).click();
  await panel.getByRole('button',{name:'应用并重新同步',exact:true}).waitFor();
  await page.waitForFunction(()=>window.__independentQa.dockReads>0);
  assert.equal(await dock.count(),0);
  await assertPc();
  await page.evaluate(()=>{window.__independentQa.attached=true;window.__independentQa.tickDiscovery();});
  await dock.waitFor();
  await assertPc();
  const dockChoice=panel.locator('label.light-target').filter({hasText:'MCHOSE MagDock'}).getByRole('checkbox');
  assert(!(await dockChoice.isChecked()),'late attach cannot join the running PC session');
  await dock.getByLabel('底座灯光颜色',{exact:true}).fill('#abcdef');
  const discoveryReads=await page.evaluate(()=>{const n=window.__independentQa.dockReads;window.__independentQa.tickDiscovery();return n;});
  await page.waitForFunction(n=>window.__independentQa.dockReads>n,discoveryReads);
  await page.evaluate(()=>new Promise(requestAnimationFrame));
  assert.equal(await dock.getByLabel('底座灯光颜色',{exact:true}).inputValue(),'#abcdef');
  await panel.getByRole('button',{name:'停止并恢复',exact:true}).click();
  await panel.getByRole('button',{name:'开始同步',exact:true}).waitFor();
  assert.equal(await dock.getByLabel('底座灯光颜色',{exact:true}).inputValue(),'#abcdef');
  assert(!(await dockChoice.isChecked()));
  await panel.getByRole('button',{name:'开始同步',exact:true}).click();
  await panel.getByRole('button',{name:'应用并重新同步',exact:true}).waitFor();
  await assertPc(18);
  assert.deepEqual(errors,[]);
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,backend:'stateful IPC simulation',checks:[
   'running PC-only session loads its exact target and parameters',
   'dock music uses independent command without replacing PC worker',
   'dock color/brightness/gain route independently',
   'dock level polling preserves edited color draft',
   'dock stop restores its prior effect and preserves PC',
   'dock onboard apply preserves PC',
   'dock reset stops dock music and preserves PC',
   'explicit PC stop preserves independent dock music',
   'explicit PC restart preserves dock music and PC selection',
   'dock audio error is separate from PC status',
   'dock retry clears its own error without changing PC',
   'no implicit shared-engine start or stop',
   'old running PC status cannot overwrite successful stop',
   'old stopped PC status cannot overwrite successful start',
   'old running dock status cannot overwrite successful stop',
   'old stopped dock status cannot overwrite successful start',
   'delayed dock attach is detected while PC runs without auto-selecting',
   '12-second discovery and PC stop preserve the dock color draft'
  ],pageErrors:errors},null,2));
  console.log('18 independent PC/dock lighting UI checks passed (stateful IPC simulation, no hardware)');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
