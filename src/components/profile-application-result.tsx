import type {Activation,DeviceSummary} from '@/lib/api';
import {MCHOSE_BINDINGS,MCHOSE_BUTTONS,MCHOSE_LOD} from '@/lib/mchose-options';
import {mchoseKeyboardLabel} from '@/lib/mchose-keys';
import {mchoseRateHz} from '@/lib/mchose-plan';
import {Button} from '@/components/ui/button';

function bindingName(bytes:number[]) {
  if(bytes[0]===0x21)return `宏槽 ${bytes[1]+1}`;
  return mchoseKeyboardLabel(bytes)??MCHOSE_BINDINGS.find(([,value])=>value.length===bytes.length&&value.every((v,i)=>v===bytes[i]))?.[0]??bytes.map(v=>v.toString(16).padStart(2,'0')).join(' ');
}

/** Historical result from the native transaction; opening it never reads or writes HID. */
export function ProfileApplicationResult({activation,devices,onDismiss}:{activation:Activation|null;devices:DeviceSummary[];onDismiss:()=>void}) {
  if(!activation||(!activation.mchose&&!activation.errors.length))return null;
  const r=activation.mchose;
  const sameConnection=r&&devices.some(d=>d.id===activation.device_id&&d.connection_key===r.target.connection_key&&d.profile_key===r.target.device_key);
  return <aside className="mx-6 my-2 shrink-0 rounded-lg border bg-card text-sm" aria-label="配置应用结果">
    <div className="flex items-start gap-3 px-4 py-3">
      <details className="min-w-0 flex-1" open={activation.errors.length>0||undefined} key={`${activation.id}:${r?.target.connection_key}:${r?.backup}:${activation.errors.join('|')}`}>
        <summary className="cursor-pointer font-semibold">{activation.errors.length?'应用结果需要处理':'配置已读回核对'} · {activation.name}{r?` · ${r.device_name}`:''}</summary>
        <div className="mt-3 max-h-[45vh] space-y-3 overflow-y-auto pr-2">
          {activation.errors.length>0&&<div role="alert" className="space-y-1 text-destructive">{activation.errors.map((e,i)=><p key={i} className="break-words whitespace-pre-wrap">{e}</p>)}</div>}
          {r&&<>
            <p className="text-muted-foreground">{sameConnection?'以下为应用时的读回核对结果。':'设备连接已变化；以下保留的是应用时的核对记录。'}{activation.errors.length>0?'鼠标设置已核对，但配置记录保存未完成。':''}</p>
            <dl className="grid grid-cols-2 gap-x-5 gap-y-1 sm:grid-cols-4">
              <div><dt className="text-muted-foreground">板载组</dt><dd>{r.profile+1} · {r.names[r.profile]}</dd></div>
              <div><dt className="text-muted-foreground">USB / 无线</dt><dd>{mchoseRateHz(r.raw_settings[1]>>4)??'?'} / {mchoseRateHz(r.raw_settings[2]>>4)??'?'} Hz</dd></div>
              <div><dt className="text-muted-foreground">LOD</dt><dd>{MCHOSE_LOD[r.lod]??r.lod}</dd></div>
              <div><dt className="text-muted-foreground">宏</dt><dd>新写入 {r.uploaded_macros.length} 个，引用 {Object.keys(r.slot_map).length} 个</dd></div>
            </dl>
            <div className="grid gap-4 sm:grid-cols-2">
              <table className="w-full text-left" aria-label="已核对 DPI"><caption className="pb-1 text-left font-semibold">DPI · {r.independent?'X / Y 独立':'X / Y 同步'}</caption><thead><tr><th>档位</th><th>X</th><th>Y</th></tr></thead><tbody>{r.dpi_x.slice(0,r.count).map((x,i)=><tr key={i} className={i===r.active?'font-semibold text-primary':''}><td>{i+1}{i===r.active?'（当前）':''}</td><td>{x}</td><td>{r.independent?r.dpi_y[i]:x}</td></tr>)}</tbody></table>
              <table className="w-full text-left" aria-label="已核对按键"><caption className="pb-1 text-left font-semibold">按键映射</caption><tbody>{r.buttons.map((b,i)=><tr key={i}><th className="font-normal">{MCHOSE_BUTTONS[i]??`按键 ${i+1}`}</th><td>{bindingName(b)}</td></tr>)}</tbody></table>
            </div>
            <p>左 / 右键消抖 {r.left_debounce} / {r.right_debounce} ms · 旋转 {r.rotation}° · {r.never_sleep?'永不休眠':`休眠 ${r.sleep} 分钟`}</p>
            {Object.keys(r.slot_map).length>0&&<p aria-label="已核对宏槽">方案宏槽 → 鼠标宏槽：{Object.entries(r.slot_map).map(([logical,physical])=>`${Number(logical)+1} → ${physical+1}`).join('，')}。{r.uploaded_macros.length?'新增写入的宏已逐字节读回核对。':'已有宏内容匹配，未重复上传。'}</p>}
            {r.backup?<div><p className="font-semibold">写入前原始备份</p><p className="select-text break-all text-muted-foreground">{r.backup}</p></div>:<p className="text-muted-foreground">设置已一致，未重复写入，未新建备份。</p>}
            <p className="text-xs text-muted-foreground">此记录核对配置数据，不代表按键或宏的物理播放、灯效显示已验收；底座灯效在灯光同步中单独管理。</p>
          </>}
        </div>
      </details>
      <Button variant="ghost" size="sm" onClick={onDismiss} aria-label="关闭配置应用结果">关闭</Button>
    </div>
  </aside>;
}
