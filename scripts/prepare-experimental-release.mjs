// Build/sign locally; no private key is copied into the project or release.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
function arg(name,optional=false){const i=process.argv.indexOf('--'+name);const v=i>=0?process.argv[i+1]:null;if(!v&&!optional)throw Error('Missing --'+name);return v;}
const version=arg('version'),tag=arg('tag'),repo=arg('repo'),out=path.resolve(arg('out'));
if(!/^\d+\.\d+\.\d+$/.test(version)||!new RegExp('^v'+version.replaceAll('.','\\.')+'-experimental\\.\\d+$').test(tag))throw Error('Expected an explicit experimental version tag');
if(!/^[A-Za-z0-9_-]+\/[A-Za-z0-9_.-]+$/.test(repo))throw Error('Invalid repository');
if(!process.env.TAURI_SIGNING_PRIVATE_KEY&&!process.env.TAURI_SIGNING_PRIVATE_KEY_PATH)throw Error('Provide the private signing key through the local environment, never Git');
const installer=path.resolve(arg('installer')),notes=fs.readFileSync(arg('notes'),'utf8').trim();
if(!fs.existsSync(installer))throw Error('Installer does not exist');
fs.mkdirSync(out,{recursive:true});const name=`Open-HUB-${version}-x64-setup.exe`,asset=path.join(out,name);
if(installer!==asset)fs.copyFileSync(installer,asset);
const cli=fileURLToPath(new URL('../node_modules/@tauri-apps/cli/tauri.js',import.meta.url));
execFileSync(process.execPath,[cli,'signer','sign','--app-version',version,asset],{env:{...process.env,TAURI_SIGNING_PRIVATE_KEY_PASSWORD:process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD??''},stdio:['ignore','pipe','pipe']});
const signature=fs.readFileSync(asset+'.sig','utf8').trim();
const sha256=createHash('sha256').update(fs.readFileSync(asset)).digest('hex');
const latest={version,notes,pub_date:new Date().toISOString(),experimental:true,release_url:`https://github.com/${repo}/releases/tag/${tag}`,platforms:{'windows-x86_64':{signature,url:`https://github.com/${repo}/releases/download/${tag}/${name}`}}};
fs.writeFileSync(path.join(out,'latest.json'),JSON.stringify(latest,null,2)+'\n');
console.log(JSON.stringify({version,tag,installer:name,sha256,versionBoundSignature:true,privateKeyIncluded:false}));
