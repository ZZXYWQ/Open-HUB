import {legacyModels,legacyNames} from '@/lib/legacy';
import {useEffect, useState} from 'react';
import {Download, Mouse} from 'lucide-react';
import {api} from '@/lib/api';
import type {SinoModel} from '@/lib/sino';
import {Button} from '@/components/ui/button';

const sources = [
  {id:'sc620',brand:'狼蛛 / AULA',software:'原厂数据库预览',label:'狼蛛 SC620 原厂配置预览',models:'SC620 · 原厂 1.2.2 文件结构',scope:'参数、按键与宏播放方式编辑／另存；未开放设备应用 · 开发中'},
  {id:'attackshark',brand:'Attack Shark',software:'Open HUB 配置包',label:'Attack Shark X11 配置导入',models:'X11 · USB / 2.4G',scope:'完整 DPI、八个控制、原生宏、性能及七种灯效；原厂私有文件尚未转换 · 模拟验证'},
  ...legacyModels.map(m=>({id:m.brand,brand:legacyNames[m.brand],software:'读取板载配置',label:legacyNames[m.brand]+' 板载配置导入',models:m.name+' · 有线 USB',scope:'预览 DPI、回报率、可读按键'+(m.lights.length?'和灯效':'')+'；原厂宏保持不变 · 模拟验证'})),
  {id:'hyperx',brand:'HyperX',software:'读取板载配置',label:'HyperX 板载配置导入',models:'Pulsefire Haste / Haste Wireless · HP USB ID',scope:'预览 DPI、回报率和可读按键；原厂宏与灯效无法读回 · 模拟验证'},
  {id:'pulsar',brand:'Pulsar',software:'读取板载配置',label:'Pulsar 板载配置导入',models:'X2 V2 Mini · USB / 1K 接收器',scope:'预览 DPI、回报率、基础按键、已校验的板载快捷键与指示灯 · 模拟验证'},
  {id:'vxe',brand:'蜻蜓 / VXE',software:'读取板载配置',label:'蜻蜓 / VXE 板载配置导入',models:'R1 / R1 Pro · MAD R / MAD R+ · 精确 USB ID',scope:'预览 DPI、回报率、按键和板载宏 · 模拟验证'},
  {id:'vgn',brand:'VGN',software:'读取板载配置',label:'VGN 板载配置导入',models:'蜻蜓 F1 Pro / F1 Pro Max · 精确 USB ID',scope:'预览 DPI、回报率、按键、板载宏和指示灯 · 模拟验证'},
  {id:'atk',brand:'ATK',software:'读取板载配置',label:'ATK 板载配置导入',models:'A9 Plus NK · USB / NearLink；X1 Pro Max · USB',scope:'预览 DPI、回报率、按键、板载宏和指示灯 · 模拟验证'},
  {id:'ghub',brand:'罗技 / Logitech',software:'G HUB',label:'G HUB 配置导入',models:'已适配的 HID++ 鼠标',scope:'DPI、回报率、应用关联'},
  {id:'mhub',brand:'迈从 / MCHOSE',software:'M HUB',label:'M HUB 配置导入',models:'A7 V3 Ultra+',scope:'完整 JSON 方案、显式宏库正文、参数编辑；也可导入板载引用 · 实验适配'},
  {id:'synapse',brand:'雷蛇 / Razer',software:'Synapse',label:'导入 Synapse 配置',models:'Naga V2 HyperSpeed · Synapse 4',scope:'预览 DPI、按键和可转换的宏'},
  {id:'icue',brand:'海盗船 / Corsair',software:'iCUE',label:'导入 iCUE 配置',models:'M65 PRO RGB',scope:'预览 DPI、按键和可转换的宏'},
  {id:'gg',brand:'赛睿 / SteelSeries',software:'GG',label:'导入 GG 配置',models:'Rival 300 / Rival 310 / Sensei 310 / Sensei Ten / Rival 600 / Rival 3（原版有线）',scope:'预览 DPI、按键、宏和对应灯区效果'},
] as const;

export function BrandProfileImports({onChoose}:{onChoose:(source:string,models:SinoModel[])=>void}) {
  const [models,setModels] = useState<SinoModel[]>([]);
  const [error,setError] = useState('');
  const [attempt,setAttempt] = useState(0);
  useEffect(()=>{
    let current=true;
    setError('');
    api.sinoModels().then(value=>{
      if(!current)return;
      if(!Array.isArray(value)){setError('板载配置支持列表暂不可用');return;}
      setModels(value);
    },e=>{if(current)setError(`板载配置支持列表读取失败：${String(e)}`);});
    return ()=>{current=false;};
  },[attempt]);
  const brands=[...new Set(models.map(m=>m.brand))].sort((a,b)=>a.localeCompare(b));
  return <section aria-label="按品牌导入配置" className="space-y-3">
    <p className="text-xs text-muted-foreground">选择鼠标品牌；每项标明已支持型号和可导入内容。</p>
    <div className="hub-source-grid">
      {sources.map(source=><button key={source.id} aria-label={source.label} className="hub-brand-card hub-brand-horizontal" style={{flexDirection:'column',alignItems:'stretch',gap:8}} onClick={()=>onChoose(source.id,[])}>
        <span className="flex items-start justify-between gap-2"><b>{source.brand}</b><Download size={16} className="mt-1 shrink-0"/></span>
        <span><small>{source.software} · {source.models}</small><small>{source.scope}</small></span>
      </button>)}
    </div>
    {brands.length>0&&<div className="rounded-xl border p-4">
      <p className="text-sm font-semibold">从鼠标读取板载配置</p>
      <p className="mt-2 text-xs text-muted-foreground">以下品牌读取指定型号的鼠标板载参数，不解析官方软件文件。原厂宏无法读回。</p>
      <div className="hub-source-grid mt-3">{brands.map(brand=>{
        const supported=models.filter(m=>m.brand===brand);
        const names=[...new Set(supported.map(m=>m.name))];
        return <button key={brand} aria-label={`${brand} 板载配置导入`} className="hub-brand-card hub-brand-horizontal" onClick={()=>onChoose(`device:${brand}`,supported)}>
          <span className="hub-brand-mark"><Mouse size={24}/></span><span><b>{brand}</b><small>{names.join(' / ')}</small><small>读取板载 DPI、回报率和按键 · 指定固件</small></span><Download size={16} className="shrink-0"/>
        </button>;
      })}</div>
    </div>}
    {error&&<div className="flex items-center gap-2 text-xs text-muted-foreground"><span>{error}</span><Button variant="ghost" size="sm" onClick={()=>setAttempt(v=>v+1)}>重试支持列表</Button></div>}
  </section>;
}
