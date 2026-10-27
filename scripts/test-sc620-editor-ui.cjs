// Built editor consumes actual Rust assessments; captured request is replayed natively.
const {chromium}=require('playwright'),fs=require('fs'),path=require('path'),assert=require('assert');
const dto=JSON.parse(fs.readFileSync('../outputs/100-model-development/sc620-editor-ui-dtos.json','utf8'));
const root=path.resolve('dist'),out=path.resolve('../outputs/100-model-development/sc620-editor-ui');fs.mkdirSync(out,{recursive:true});
const base=fs.readFileSync(path.join(__dirname,'test-synapse-import-ui.cjs'),'utf8'),marker='await page.addInitScript(()=>{',start=base.indexOf(marker)+marker.length,end=base.indexOf('\n});await ',start);assert(end>start);
(async()=>{const browser=await chromium.launch({headless:true});try{
 const page=await browser.newPage({viewport:{width:1400,height:1050}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript({content:`(()=>{${base.slice(start,end)}})()`});
 await page.addInitScript(({dto})=>{const old=window.__TAURI_INTERNALS__.invoke;window.__sc620EditFail=false;window.__TAURI_INTERNALS__.invoke=async(cmd,args={})=>{
  if(cmd==='plugin:dialog|open')return 'C:/fixtures/sc620.db';if(cmd==='plugin:dialog|save')return 'C:/fixtures/SC620-edited.json';
  if(cmd==='preview_sc620_source'){window.__qaCalls.push({cmd,args});return structuredClone(dto.base);}
  if(cmd==='edit_sc620_source'){window.__qaCalls.push({cmd,args});if(window.__sc620EditFail)throw Error('invalid offline edit');return structuredClone(dto.edited);}
  if(cmd==='export_sc620_source'){window.__qaCalls.push({cmd,args});return;}
  return old(cmd,args);
 };},{dto});
 await require('./ui-static-route.cjs')(page,root);await page.goto('http://127.0.0.1:1431');await page.getByRole('button',{name:'配置',exact:true}).click();await page.getByRole('button',{name:'导入配置',exact:true}).click();await page.getByRole('button',{name:'狼蛛 SC620 原厂配置预览',exact:true}).click();
 const dialog=page.getByRole('dialog').filter({has:page.getByRole('heading',{name:'狼蛛 SC620 配置预览',exact:true})});await dialog.getByRole('button',{name:'选择 SC620 配置文件',exact:true}).click();await dialog.getByRole('button',{name:'编辑源方案',exact:true}).click();
 const editor=dialog.getByRole('region',{name:'SC620 源方案编辑',exact:true});await editor.waitFor();assert(await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).isDisabled());
 for(let i=0;i<6;i++){
  await editor.getByRole('spinbutton',{name:`SC620 编辑DPI${i+1}`,exact:true}).fill(String(dto.request.settings.dpi[i]));
  const color='#'+dto.request.settings.colors[i].map(n=>n.toString(16).padStart(2,'0')).join('');await editor.getByLabel(`SC620 编辑指示色${i+1}`,{exact:true}).fill(color);
 }
 await editor.getByRole('radio',{name:'SC620 编辑当前档2',exact:true}).check();await editor.getByRole('combobox',{name:'SC620 编辑回报率',exact:true}).selectOption('1000');await editor.getByRole('spinbutton',{name:'SC620 编辑消抖毫秒',exact:true}).fill('5');
 await editor.getByRole('combobox',{name:'SC620 编辑LOD毫米',exact:true}).selectOption('1');await editor.getByRole('combobox',{name:'SC620 编辑休眠分钟',exact:true}).selectOption('5');await editor.getByRole('combobox',{name:'SC620 编辑灯效',exact:true}).selectOption('Neon');
 for(const [name,on]of [['反向滚轮',false],['电竞模式',false],['波纹控制',true],['直线修正',false],['Motion Sync',true]])await editor.getByRole('checkbox',{name:'SC620 编辑'+name,exact:true}).setChecked(on);
 await editor.getByRole('combobox',{name:'SC620 编辑控制4',exact:true}).selectOption('macro:7');const control5=editor.getByRole('combobox',{name:'SC620 编辑控制5',exact:true});await control5.selectOption({label:'静音'});await editor.getByRole('combobox',{name:'SC620 编辑控制6',exact:true}).selectOption('key');
 assert(await editor.getByRole('checkbox',{name:'SC620 控制6左 Ctrl',exact:true}).isChecked());assert(await editor.getByRole('checkbox',{name:'SC620 控制6右 Ctrl',exact:true}).isChecked());
 await editor.getByRole('combobox',{name:'SC620 编辑控制6键码',exact:true}).selectOption('5');await editor.getByRole('checkbox',{name:'SC620 控制6左 Ctrl',exact:true}).uncheck();await editor.getByRole('checkbox',{name:'SC620 控制6右 Ctrl',exact:true}).uncheck();await editor.getByRole('checkbox',{name:'SC620 控制6左 Shift',exact:true}).check();
 await editor.getByRole('combobox',{name:'SC620 编辑控制11',exact:true}).selectOption('disabled');await editor.getByRole('combobox',{name:'SC620 编辑宏7播放方式',exact:true}).selectOption('Repeat');await editor.getByRole('spinbutton',{name:'SC620 编辑宏7次数',exact:true}).fill('4');await editor.getByRole('combobox',{name:'SC620 编辑宏42播放方式',exact:true}).selectOption('UntilAnyClick');
 await page.screenshot({path:path.join(out,'editing.png')});await editor.getByRole('button',{name:'检查并更新预览',exact:true}).click();await dialog.getByRole('status').filter({hasText:'当前源方案已编辑'}).waitFor();
 const actual=await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='edit_sc620_source').at(-1).args);assert.deepEqual(actual.source,dto.base.source);assert.deepEqual(actual.request,dto.request);fs.writeFileSync('../outputs/100-model-development/sc620-editor-ui-request.json',JSON.stringify(actual,null,2));
 assert(await dialog.getByText(/1000 Hz · 消抖 5 ms · 灯效：霓虹/).isVisible());assert(await dialog.getByText('左 Shift + B',{exact:true}).isVisible());assert(await dialog.getByText('静音',{exact:true}).isVisible());
 await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).click();await dialog.getByRole('status').filter({hasText:'源方案已保存'}).waitFor();assert.deepEqual(await page.evaluate(()=>window.__qaCalls.filter(c=>c.cmd==='export_sc620_source').at(-1).args.source),dto.edited.source);await page.screenshot({path:path.join(out,'reviewed.png')});
 await dialog.getByRole('button',{name:'编辑源方案',exact:true}).click();await editor.getByRole('spinbutton',{name:'SC620 编辑DPI1',exact:true}).fill('999');await editor.getByRole('button',{name:'取消编辑',exact:true}).click();assert(await dialog.getByText('100 DPI',{exact:true}).isVisible());assert.equal(await dialog.getByText('999 DPI',{exact:true}).count(),0);
 await dialog.getByRole('button',{name:'编辑源方案',exact:true}).click();await page.evaluate(()=>window.__sc620EditFail=true);await editor.getByRole('spinbutton',{name:'SC620 编辑DPI1',exact:true}).fill('49');await editor.getByRole('button',{name:'检查并更新预览',exact:true}).click();await dialog.getByRole('alert').filter({hasText:'invalid offline edit'}).waitFor();assert(await editor.isVisible());assert(await dialog.getByRole('button',{name:'保存 Open HUB 源方案',exact:true}).isDisabled());
 await editor.getByRole('button',{name:'取消编辑',exact:true}).click();assert(await dialog.getByText('100 DPI',{exact:true}).isVisible());
 const calls=await page.evaluate(()=>window.__qaCalls);assert(!calls.some(c=>['multi_apply','sino_apply','activate_profile','save_profile','commit_profile_import'].includes(c.cmd)));assert.deepEqual(errors,[]);
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({passed:true,hardware_accessed:false,IPC_mocked:true,production_Rust_dtos:true,checks:['six DPI stages active selection and six colors','polling debounce LOD sleep lighting and sensor edits','sparse macro ID button binding','keyboard usage and preserved initial modifier sides','control-specific disabled action','both edited macro playback modes','exact complete typed editor request','updated preview exported intact','cancel discards only local draft','invalid edit retains old accepted data and disables archive','no generic profile or hardware writes','no browser errors']},null,2));console.log('12 SC620 source editor built UI checks passed; exact request captured');
}finally{await browser.close();}})().catch(e=>{console.error(e);process.exitCode=1;});
