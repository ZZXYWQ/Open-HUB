import sys, types, os, json, pathlib, argparse
root=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser();parser.add_argument("--source",type=pathlib.Path,required=True,help="Path to an audited flozz/rivalcfg checkout");args=parser.parse_args()
# No real HID module is imported. Any attempted device enumeration/open is a test failure.
hid=types.ModuleType('hid')
def forbidden(*a,**kw): raise AssertionError('Hardware access forbidden during reference simulation')
hid.enumerate=forbidden;hid.device=forbidden;sys.modules['hid']=hid
os.environ['RIVALCFG_DEBUG_NO_COMMAND_DELAY']='1'
sys.path.insert(0,str(args.source.resolve()))
from rivalcfg import mouse,usbhid,mouse_settings
from rivalcfg.devices import rival310
settings=mouse_settings.FakeMouseSettings(0x1038,0x1720,rival310.profile)
m=mouse.Mouse(usbhid.FakeDevice(),rival310.profile,settings)
cases=[]
def capture(name,method,value):
    stream=m._hid_device.bytes;stream.seek(0);stream.truncate(0)
    getattr(m,method)(value)
    p=stream.getvalue();assert p[0]==2 and p[1]==0
    cases.append(dict(name=name,method=method,value=value,report=list(p[1:])))
for n in [100,800,1600,12000]:capture('dpi1-'+str(n),'set_sensitivity1',n)
for n in [100,800,1600,12000]:capture('dpi2-'+str(n),'set_sensitivity2',n)
for n in [125,250,500,1000]:capture('rate-'+str(n),'set_polling_rate',n)
capture('buttons-default','set_buttons_mapping','default')
capture('buttons-key-a','set_buttons_mapping','buttons(button1=button1;button6=a)')
out=root/'src-tauri/tests/fixtures/rival310-rivalcfg.json'
out.parent.mkdir(parents=True,exist_ok=True)
out.write_text(json.dumps(dict(source='https://github.com/flozz/rivalcfg',simulated=True,hardware_access=False,cases=cases),indent=2),encoding='utf-8')
print(f'{len(cases)} upstream reference simulations passed; HID access forbidden')


