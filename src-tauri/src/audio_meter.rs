//! Windows render-endpoint peak meter. No microphone, audio files or network.
use std::ffi::c_void;
use std::ptr::null_mut;
type R<T> = Result<T, String>;
#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
const ENUM: Guid = Guid {
    a: 0xbcde0395,
    b: 0xe52f,
    c: 0x467c,
    d: [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
};
const IENUM: Guid = Guid {
    a: 0xa95664d2,
    b: 0x9614,
    c: 0x4f35,
    d: [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
};
const IMETER: Guid = Guid {
    a: 0xc02216f6,
    b: 0x8c67,
    c: 0x4b5b,
    d: [0x9d, 0x00, 0xd0, 0x08, 0xe7, 0x3e, 0x00, 0x64],
};
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(p: *mut c_void, flags: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        c: *const Guid,
        o: *mut c_void,
        ctx: u32,
        i: *const Guid,
        p: *mut *mut c_void,
    ) -> i32;
}
struct Com(*mut c_void);
impl Drop for Com {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let f: unsafe extern "system" fn(*mut c_void) -> u32 =
                    std::mem::transmute(slot(self.0, 2));
                f(self.0);
            }
        }
    }
}
unsafe fn slot(p: *mut c_void, i: usize) -> *const c_void {
    *(*(p as *const *const *const c_void)).add(i)
}
fn check(hr: i32) -> R<()> {
    if hr < 0 {
        Err(format!(
            "无法读取 Windows 播放设备音量：0x{:08X}",
            hr as u32
        ))
    } else {
        Ok(())
    }
}
pub struct Meter {
    meter: Com,
    initialized: bool,
}
impl Meter {
    pub fn open() -> R<Self> {
        unsafe {
            check(CoInitializeEx(null_mut(), 0))?;
            let mut this = Self {
                meter: Com(null_mut()),
                initialized: true,
            };
            let mut en = Com(null_mut());
            check(CoCreateInstance(&ENUM, null_mut(), 23, &IENUM, &mut en.0))?;
            let get: unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(en.0, 4));
            let mut device = Com(null_mut());
            check(get(en.0, 0, 1, &mut device.0))?;
            let activate: unsafe extern "system" fn(
                *mut c_void,
                *const Guid,
                u32,
                *mut c_void,
                *mut *mut c_void,
            ) -> i32 = std::mem::transmute(slot(device.0, 3));
            check(activate(
                device.0,
                &IMETER,
                23,
                null_mut(),
                &mut this.meter.0,
            ))?;
            Ok(this)
        }
    }
    pub fn peak(&self) -> R<f32> {
        unsafe {
            let get: unsafe extern "system" fn(*mut c_void, *mut f32) -> i32 =
                std::mem::transmute(slot(self.meter.0, 3));
            let mut peak = 0.0;
            check(get(self.meter.0, &mut peak))?;
            Ok(if peak.is_finite() {
                peak.clamp(0.0, 1.0)
            } else {
                0.0
            })
        }
    }
}
impl Drop for Meter {
    fn drop(&mut self) {
        let meter = std::mem::replace(&mut self.meter, Com(null_mut()));
        drop(meter);
        if self.initialized {
            unsafe { CoUninitialize() }
        }
    }
}
