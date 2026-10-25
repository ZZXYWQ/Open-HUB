// Run built UI regression suites with the existing browser; no vendor device writes.
const fs=require('fs'),path=require('path'),{spawnSync}=require('child_process');
const files=fs.readdirSync(__dirname).filter(f=>/^test-.*ui.*\.cjs$/.test(f)).sort();const results=[];
for(const file of files){const r=spawnSync(process.execPath,['-r',path.join(__dirname,'ui-browser-runtime.cjs'),path.join(__dirname,file)],{encoding:'utf8',timeout:120000,windowsHide:true,env:process.env});results.push({file,passed:r.status===0,status:r.status,output:(r.stdout||'')+(r.stderr||'')});console.log((r.status===0?'PASS ':'FAIL ')+file);if(r.status!==0)console.log(results.at(-1).output.slice(-2300));}
fs.writeFileSync(path.resolve('../outputs/0.3.3-ui-regression.json'),JSON.stringify({passed:results.every(r=>r.passed),suites:results.length,hardware_accessed:false,results},null,2));process.exitCode=results.every(r=>r.passed)?0:1;
