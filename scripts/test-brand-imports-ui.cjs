const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const root=path.resolve('dist'),out=path.resolve('../brand-imports-ui-qa');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{';
const start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);if(start<marker.length||end<start)throw Error('Shared fixture not found');
(async()=>{
  const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-gpu']});
  try {
    const page=await browser.newPage({viewport:{width:1450,height:1050},deviceScaleFactor:1}),errors=[];
    page.on('pageerror',error=>errors.push(String(error)));
    await page.addInitScript({content:'(()=>{'+base.slice(start,end)+'})()'});
    await page.addInitScript(()=>{
      const models=[
        {vendor:0x258a,product:0x0036,firmware:'V103',name:'Glorious Model O/O-',brand:'Glorious',buttons:6,profiles:1,sensors:[6]},
        {vendor:0x258a,product:0x0036,firmware:'V105',name:'Glorious Model O/O-',brand:'Glorious',buttons:6,profiles:1,sensors:[6]},
        {vendor:0x258a,product:0x0027,firmware:'3106',name:'DreamMachines DM5 Blink',brand:'DreamMachines',buttons:8,profiles:1,sensors:[15]},
        {vendor:0x258a,product:0x1007,firmware:'9677',name:'Marvo Scorpion G961',brand:'Marvo',buttons:6,profiles:1,sensors:[14]},
        {vendor:0x258a,product:0x0036,firmware:'V161',name:'G-Wolves Hati HT-M Wired',brand:'G-Wolves',buttons:6,profiles:1,sensors:[6]},
        {vendor:0x258a,product:0x0027,firmware:'3110',name:'Genesis Xenon 770',brand:'Genesis',buttons:6,profiles:1,sensors:[6]},
        {vendor:0x258a,product:0x0027,firmware:'V127',name:'Machenike M620',brand:'Machenike',buttons:6,profiles:1,sensors:[6]},
        {vendor:0x258a,product:0x0036,firmware:'V287',name:'Mad Dog GM905',brand:'Mad Dog',buttons:6,profiles:1,sensors:[6]},
        {vendor:0x258a,product:0x0027,firmware:'2642',name:'T-Dagger Imperial T-TGM310',brand:'T-Dagger',buttons:6,profiles:1,sensors:[6]}
      ];
      const device=(id,m,changes={})=>({vendor_id:m.vendor,id:'sino:'+id,name:m.name,product:m.name,product_id:m.product.toString(16),kind:'mouse',firmware_name:m.firmware,model_ids:[m.product.toString(16)],connection:'wired',protocol:'SinoWealth',features:[],...changes});
      const devices=[device('glorious',models[0]),device('dream',models[2]),device('same-vendor-wrong-model',models[0],{model_ids:['9999']})];
      const original=window.__TAURI_INTERNALS__.invoke;
      window.__qaCatalogFail=true;window.__qaWrongBoard=false;
      window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
        if(!['sino_models','scan_devices','list_devices','sino_read'].includes(cmd))return original(cmd,args);
        window.__qaCalls.push({cmd,args});
        if(cmd==='sino_models'){if(window.__qaCatalogFail)throw Error('catalog fixture unavailable');return models;}
        if(cmd==='scan_devices')return {devices,warnings:[]};
        if(cmd==='list_devices')return devices;
        let model=args.id==='sino:glorious'?models[0]:models[2];
        if(window.__qaWrongBoard)model={...model,brand:'Marvo'};
        return {peer:{id:args.id,model,serial:'mock'},plan:{model,profile:0,dpi:[[800,800],[1600,1600]],active:0,rate:1000,buttons:Array.from({length:model.buttons},()=>[17,1,0,0]),macros:[]},free_macro_slots:[],macro_readback:false,backup:null};
      };
    });
    await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');
    await page.getByRole('button',{name:'配置',exact:true}).click();
    assert.equal(await page.getByRole('button',{name:'导入配置',exact:true}).count(),1);
    assert.equal(await page.getByRole('region',{name:'按品牌导入配置'}).count(),0);
    await page.getByRole('button',{name:'导入配置',exact:true}).click();
    const region=page.getByRole('region',{name:'按品牌导入配置'}),cards=region.locator('.hub-source-grid').first();
    assert.equal(await cards.getByRole('button').count(),15);
    for(const brand of ['华硕 / ASUS','Roccat','芝奇 / G.Skill','Etekcity','Mars Gaming','罗技 / Logitech','迈从 / MCHOSE','雷蛇 / Razer','海盗船 / Corsair','赛睿 / SteelSeries','ATK','蜻蜓 / VXE','VGN','HyperX','Pulsar'])assert(await cards.getByText(brand,{exact:true}).isVisible());
    assert(await cards.getByText('Synapse · Naga V2 HyperSpeed · Synapse 4',{exact:true}).isVisible());
    assert(await cards.getByText('M HUB · A7 V3 Ultra+',{exact:true}).isVisible());
    assert.equal(await region.getByText('多品牌',{exact:true}).count(),0);
    await page.getByText(/catalog fixture unavailable/).waitFor();
    await page.evaluate(()=>window.__qaCatalogFail=false);
    await page.getByRole('button',{name:'重试支持列表',exact:true}).click();
    assert.equal(await region.locator('details').count(),0);
    assert.equal(await region.getByRole('button').count(),23);
    for(const brand of ['Glorious','DreamMachines','Marvo','G-Wolves','Genesis','Machenike','Mad Dog','T-Dagger'])assert(await region.getByRole('button',{name:brand+' 板载配置导入',exact:true}).isVisible());
    const gloriousCard=region.getByRole('button',{name:'Glorious 板载配置导入',exact:true});
    assert.equal(await gloriousCard.getByText('Glorious Model O/O-',{exact:true}).count(),1);
    await gloriousCard.click();
    let dialog=page.getByRole('dialog');
    await dialog.getByRole('heading',{name:'Glorious · 读取板载配置',exact:true}).waitFor();
    const selector=dialog.getByRole('combobox',{name:'选择导入鼠标',exact:true});
    assert.equal(await selector.locator('option').count(),1);assert.equal(await selector.inputValue(),'sino:glorious');
    await dialog.getByRole('button',{name:'读取鼠标板载配置',exact:true}).click();
    await dialog.getByText('Glorious · 板载1',{exact:true}).waitFor();
    await dialog.getByRole('button',{name:'导入 1 项',exact:true}).click();await dialog.waitFor({state:'hidden'});
    await page.getByRole('button',{name:'导入配置',exact:true}).click();
    await region.getByRole('button',{name:'DreamMachines 板载配置导入',exact:true}).click();
    dialog=page.getByRole('dialog');await dialog.getByRole('heading',{name:'DreamMachines · 读取板载配置',exact:true}).waitFor();
    assert.equal(await dialog.getByRole('combobox').locator('option').count(),1);
    assert.equal(await dialog.getByRole('combobox').inputValue(),'sino:dream');
    assert.equal(await dialog.getByRole('button',{name:'导入 0 项',exact:true}).isDisabled(),true);
    await page.evaluate(()=>window.__qaWrongBoard=true);
    await dialog.getByRole('button',{name:'读取鼠标板载配置',exact:true}).click();
    await dialog.getByRole('alert').filter({hasText:'读取结果与所选品牌和型号不匹配'}).waitFor();
    assert(await dialog.getByRole('button',{name:'导入 0 项',exact:true}).isDisabled());
    await dialog.getByRole('button',{name:'取消',exact:true}).click();
    await page.getByRole('button',{name:'导入配置',exact:true}).click();
    await region.getByRole('button',{name:'Marvo 板载配置导入',exact:true}).click();
    dialog=page.getByRole('dialog');await dialog.getByRole('heading',{name:'Marvo · 读取板载配置',exact:true}).waitFor();
    assert(await dialog.getByRole('button',{name:'读取鼠标板载配置',exact:true}).isDisabled());
    assert.equal(await dialog.getByRole('combobox').inputValue(),'');
    assert(await dialog.getByText(/Marvo Scorpion G961（9677）/).isVisible());
    await dialog.getByRole('button',{name:'取消',exact:true}).click();
    for(const [label,title,kind] of [['G HUB 配置导入','导入罗技 G HUB 配置','ghub'],['M HUB 板载配置导入','导入迈从 M HUB 配置','mhub']]) {
      await page.getByRole('button',{name:'导入配置',exact:true}).click();
      await cards.getByRole('button',{name:label,exact:true}).click();
      dialog=page.getByRole('dialog');await dialog.getByRole('heading',{name:title,exact:true}).waitFor();
      assert.equal(await dialog.locator('.hub-source-grid').count(),0);
      await dialog.getByRole('button',{name:'读取本地配置',exact:true}).click();
      await dialog.getByRole('button',{name:'导入 1 项',exact:true}).waitFor();
      const calls=await page.evaluate(()=>window.__qaCalls);
      assert.equal(calls.filter(c=>c.cmd==='preview_profile_import').at(-1).args.source,kind);
      await dialog.getByRole('button',{name:'取消',exact:true}).click();
    }
    await page.getByRole('button',{name:'导入配置',exact:true}).click();
    await page.getByRole('button',{name:'恢复 Open HUB 备份',exact:true}).click();
    dialog=page.getByRole('dialog');await dialog.getByRole('heading',{name:'导入 Open HUB 配置包',exact:true}).waitFor();
    assert.equal(await dialog.getByRole('button',{name:'读取本地配置',exact:true}).count(),0);
    await dialog.getByRole('button',{name:'选择配置文件',exact:true}).click();
    await dialog.getByRole('button',{name:'导入 1 项',exact:true}).waitFor();
    await dialog.getByRole('button',{name:'取消',exact:true}).click();
    await page.getByRole('button',{name:'导入配置',exact:true}).click();
    await region.waitFor({state:'visible'});
    await page.getByRole('dialog').evaluate(el=>{el.scrollTop=0});
    await page.screenshot({path:path.join(out,'brand-imports.png'),fullPage:true,animations:'disabled'});
    await page.setViewportSize({width:820,height:850});
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
    await page.getByRole('dialog').evaluate(el=>{el.scrollTop=0});
    await page.waitForFunction(()=>{const r=document.querySelector('[role=dialog]').getBoundingClientRect();return r.x>=15&&r.y>=23&&r.right<=innerWidth-15&&r.bottom<=innerHeight-23});
    const bounds=await page.getByRole('dialog').evaluate(el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,maxHeight:getComputedStyle(el).maxHeight}});
    console.log('Narrow dialog bounds',bounds);
    assert(bounds.x>=0&&bounds.y>=0&&bounds.width<=820&&bounds.height<=850);
    await page.screenshot({path:path.join(out,'brand-imports-narrow.png'),fullPage:true,animations:'disabled'});
    const calls=await page.evaluate(()=>window.__qaCalls),commits=calls.filter(c=>c.cmd==='commit_profile_import');
    assert.equal(commits.length,1);assert.equal(commits[0].args.profiles[0].sino.model.brand,'Glorious');
    assert.deepEqual(calls.filter(c=>c.cmd==='sino_read').map(c=>c.args.id),['sino:glorious','sino:dream']);
    assert.equal(calls.filter(c=>c.cmd==='preview_profile_import').at(-1).args.source,'openhub');
    assert(!calls.some(c=>['multi_apply','sino_apply','activate_profile','rgb_devices'].includes(c.cmd)));
    assert.deepEqual(errors,[]);
    const checks=['ten explicit brand/software entries','per-source supported model and import scope','no generic multibrand card','catalog failure and retry','backend model catalog grouped by brand','duplicate firmware variants share one brand entry','same USB vendor does not mix brands','wrong product and firmware are excluded','board preview remains readonly','import preserves exact brand metadata','switching brand clears previous selection','wrong returned brand cannot be imported','absent matching mouse disables read','G HUB and M HUB invoke their own importers','brand dialogs have no generic source picker','single toolbar import entry with Open HUB backup restore inside chooser','all supported brands expanded without a generic group card','no horizontal overflow at minimum window width','startup and configuration imports do not start RGB or apply hardware','no browser exceptions'];
    fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({result:'passed',mock_backend:true,checks},null,2));
    console.log(checks.length+' brand-specific import UI checks passed (mock backend)');
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
