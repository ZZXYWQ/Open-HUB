// Test-only bridge to production pure Rust functions. No HID, application IPC
// dispatcher, OEM program or user browser is launched by this helper.
const fs=require('fs'),path=require('path'),assert=require('assert'),{spawnSync}=require('child_process');
module.exports=async function(page,out,{localDatabase}={}){const calls=[];await page.exposeFunction('__nativeMhubPrepare',request=>{
 const n=calls.length,requestPath=path.join(out,`request-${n}.json`),responsePath=path.join(out,`response-${n}.json`);fs.writeFileSync(requestPath,JSON.stringify(request));
 const dir=typeof localDatabase==='function'?localDatabase():localDatabase;
 const run=spawnSync('cargo',['test','--manifest-path','src-tauri/Cargo.toml','--locked','mhub_editor_ui_bridge','--','--ignored'],{windowsHide:true,encoding:'utf8',timeout:60000,env:{...process.env,OPEN_HUB_MHUB_EDITOR_UI_REQUEST:requestPath,OPEN_HUB_MHUB_EDITOR_UI_RESPONSE:responsePath,...(dir?{OPEN_HUB_MHUB_LOCAL_UI_DATABASE:dir}:{})}});
 fs.writeFileSync(path.join(out,`native-${n}.log`),(run.stdout??'')+(run.stderr??''));if(run.error)throw run.error;if(run.status!==0)throw Error('Rust bridge failed: '+(run.stdout+run.stderr).slice(-2500));assert((run.stdout??'').includes('1 passed; 0 failed'));
 const response=JSON.parse(fs.readFileSync(responsePath,'utf8'));calls.push({request,response});return response;
 });return calls;};
