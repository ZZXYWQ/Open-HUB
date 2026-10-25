// A lazy image must enter the viewport before decode can resolve.
// Broken/missing images still fail; this only waits for the real load event.
module.exports=async function readyImage(locator){await locator.scrollIntoViewIfNeeded();await locator.evaluate(async img=>{if(!img.complete)await new Promise((resolve,reject)=>{img.addEventListener('load',resolve,{once:true});img.addEventListener('error',()=>reject(Error('device image failed to load')),{once:true});});await img.decode();if(!img.naturalWidth)throw Error('device image has no decoded pixels');});};
