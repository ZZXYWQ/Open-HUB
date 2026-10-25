import {nativeMacroActionName,type NativeMacroTrace} from '@/lib/logitech-macros';
export function OnboardMacroDetails({trace}:{trace:NativeMacroTrace}){
 return <details open className="mt-3 rounded-lg border bg-background/70 p-3 text-xs" aria-label="原厂板载宏内容"><summary className="cursor-pointer font-medium">{trace.complete?'已读取至结束标记':'部分内容 · 保留未知指令'} · {trace.steps.length} 条记录 · {trace.pages.length} 页</summary>
 <p className="my-2 text-muted-foreground">仅查看此鼠标的板载内容，不播放或改写。循环与跨页指令按原样展示；页数据原样保留。</p>
 {trace.note&&<p role="status" className="mb-2 text-amber-500">{trace.note}</p>}
 <ol className="max-h-60 space-y-1 overflow-y-auto">{trace.steps.map((step,index)=><li key={index} className="flex gap-3"><span className="w-16 shrink-0 font-mono text-muted-foreground">{step.address.sector}:{step.address.offset}</span><span>{nativeMacroActionName(step.action)}</span></li>)}</ol>
 </details>;
}
