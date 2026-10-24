// Serve the actual built UI through Playwright interception. This requires no
// browser network access and does not change Windows loopback exemptions.
const fs=require('fs'),path=require('path');
module.exports=async(page,root)=>{
 root=path.resolve(root);
 await page.route('http://127.0.0.1:1431/**',async route=>{
  let pathname;try{pathname=decodeURIComponent(new URL(route.request().url()).pathname);}catch{return route.fulfill({status:400,body:'Invalid path'});}
  let target=path.resolve(root,'.'+pathname);if(target!==root&&!target.startsWith(root+path.sep))return route.fulfill({status:403,body:'Outside UI root'});
  if(!fs.existsSync(target)||fs.statSync(target).isDirectory()){if(path.extname(target))return route.fulfill({status:404,body:'Not found'});target=path.join(root,'index.html');}
  const contentType=({'.html':'text/html; charset=utf-8','.js':'application/javascript; charset=utf-8','.mjs':'application/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.json':'application/json','.webp':'image/webp','.png':'image/png','.jpg':'image/jpeg','.svg':'image/svg+xml','.woff2':'font/woff2','.woff':'font/woff'})[path.extname(target)]||'application/octet-stream';
  return route.fulfill({status:200,contentType,body:fs.readFileSync(target)});
 });
};
