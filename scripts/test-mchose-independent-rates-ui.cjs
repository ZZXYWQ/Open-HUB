// Built live panel requests are captured and replayed through native ownership,
// complete backups and settings readback by the paired explicit Rust test.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const baseline=JSON.parse(fs.readFileSync('src-tauri/tests/fixtures/mchose.json','utf8')).state;
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
const out=path.resolve('../outputs/100-model-development/mchose-independent-rates-ui');fs.mkdirSync(out,{recursive:true});
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1500,height:1150}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({baseline})=>{
  const peers=['A','B'].map(k=>({id:'mchose-'+k,name:'A7 V3 Ultra+ '+k,product:'MCHOSE A7 V3 Ultra+',vendor_id:0x3837,product_id:'1014',model_ids:['4026'],kind:'mouse',firmware_name:'fixture',connection:'wireless',protocol:'MCHOSE',features:[],profile_key:'session:mchose-'+k,connection_key:'connection:mchose-'+k+':1'}));
  const states=Object.fromEntries(peers.map(p=>[p.id,{...structuredClone(baseline),adaptive:[],target:{device_key:p.profile_key,connection_key:p.connection_key}}]));window.__rateRequests=[];window.__rateStates=states;const old=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
   if(cmd==='scan_devices')return{devices:structuredClone(peers),warnings:[]};if(cmd==='list_devices')return structuredClone(peers);
   if(cmd==='mchose_hardware'){const s=states[args.id];if(s.target.connection_key!==args.connectionKey)throw Error('Wrong connection');return structuredClone(s);}
   if(cmd==='mchose_apply'){
    const s=states[args.id],a=args.action;if(JSON.stringify(args.target)!==JSON.stringify(s.target)||args.expectedProfile!==s.profile)throw Error('Wrong owner or board');
    if(a.kind!=='connection_rate'||JSON.stringify(a.expected_settings)!==JSON.stringify(s.raw_settings))throw Error('Wrong/stale rate request');
    const at=a.connection==='usb'?1:2;s.raw_settings[at]=(s.raw_settings[at]&15)|(a.value<<4);window.__rateRequests.push({id:args.id,board:args.expectedProfile,action:structuredClone(a),target:structuredClone(args.target)});return structuredClone(s);
   }
   return old(cmd,args);
  };
 },{baseline});
 await require('./ui-static-route.cjs')(page,path.resolve('dist'));await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'Open A7 V3 Ultra+ A',exact:true}).click();
 const usb=page.getByRole('group',{name:'USB 回报率',exact:true}),wireless=page.getByRole('group',{name:'无线 回报率',exact:true});await usb.waitFor();
 assert.equal(await usb.getByRole('button').count(),6);assert.equal(await wireless.getByRole('button').count(),6);assert.equal(await page.getByRole('button',{name:/250 Hz/}).count(),0);
 await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).fill('1200');
 await usb.getByRole('button',{name:'USB 2000 Hz',exact:true}).click();await page.waitForFunction(()=>window.__rateRequests.length===1);await page.waitForFunction(()=>!document.querySelector('fieldset:disabled'));
 assert.equal(await usb.getByRole('button',{name:'USB 2000 Hz',exact:true}).getAttribute('aria-pressed'),'true');assert.equal(await wireless.getByRole('button',{name:'无线 1000 Hz',exact:true}).getAttribute('aria-pressed'),'true');
 await wireless.getByRole('button',{name:'无线 8000 Hz',exact:true}).click();await page.waitForFunction(()=>window.__rateRequests.length===2);await page.waitForFunction(()=>!document.querySelector('fieldset:disabled'));
 assert.equal(await usb.getByRole('button',{name:'USB 2000 Hz',exact:true}).getAttribute('aria-pressed'),'true');assert.equal(await page.getByRole('spinbutton',{name:'档 1 X DPI',exact:true}).inputValue(),'1200');
 await usb.getByRole('button',{name:'USB 1000 Hz',exact:true}).click();await page.waitForFunction(()=>window.__rateRequests.length===3);await page.waitForFunction(()=>!document.querySelector('fieldset:disabled'));
 await page.screenshot({path:path.join(out,'independent-links.png')});
 await page.getByRole('button',{name:'Back to devices',exact:true}).click();await page.getByRole('button',{name:'Open A7 V3 Ultra+ B',exact:true}).click();await usb.waitFor();
 assert.equal(await usb.getByRole('button',{name:'USB 1000 Hz',exact:true}).getAttribute('aria-pressed'),'true');assert.equal(await wireless.getByRole('button',{name:'无线 1000 Hz',exact:true}).getAttribute('aria-pressed'),'true');
 await usb.getByRole('button',{name:'USB 8000 Hz',exact:true}).click();await page.waitForFunction(()=>window.__rateRequests.length===4);await page.waitForFunction(()=>!document.querySelector('fieldset:disabled'));
 const requests=await page.evaluate(()=>window.__rateRequests);assert.deepEqual(requests.map(r=>r.action.connection),['usb','wireless','usb','usb']);assert.deepEqual(requests.map(r=>r.action.value),[4,6,3,6]);assert.deepEqual(requests.map(r=>r.id),['mchose-A','mchose-A','mchose-A','mchose-B']);assert(requests.every(r=>r.action.expected_settings.length===19));
 const state=await page.evaluate(()=>window.__rateStates);assert.deepEqual([state['mchose-A'].raw_settings[1]>>4,state['mchose-A'].raw_settings[2]>>4],[3,6]);assert.deepEqual([state['mchose-B'].raw_settings[1]>>4,state['mchose-B'].raw_settings[2]>>4],[6,3]);assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(out,'requests.json'),JSON.stringify(requests,null,2));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,requests:4,checks:['six supported rates per connection','unsupported 250 Hz absent','USB changes preserve wireless selection','wireless changes preserve USB selection','DPI draft preserved during rate writes','full observed settings sent with each action','second mouse retains independent defaults','exact owner/board/connection included','no browser exceptions']},null,2));console.log('9 independent-rate UI checks passed; 4 requests saved for native replay.');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
