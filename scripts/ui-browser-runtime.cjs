// Test-only: reuse installed Edge on Windows instead of downloading another browser.
const {chromium}=require('playwright');const launch=chromium.launch.bind(chromium);chromium.launch=opts=>launch(process.platform==='win32'&&!opts?.channel&&!opts?.executablePath?{...opts,channel:'msedge'}:opts);
