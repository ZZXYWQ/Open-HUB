//! Live primary-display sampling. Pixels are averaged locally, never saved.
use std::ffi::c_void;
#[link(name = "user32")]
unsafe extern "system" {
    fn GetDC(h: *mut c_void) -> *mut c_void;
    fn ReleaseDC(h: *mut c_void, dc: *mut c_void) -> i32;
    fn GetSystemMetrics(n: i32) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetPixel(dc: *mut c_void, x: i32, y: i32) -> u32;
}
pub fn sample() -> Result<[[u8; 3]; 12], String> {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        if dc.is_null() {
            return Err("无法读取当前屏幕".into());
        }
        let w = GetSystemMetrics(0);
        let h = GetSystemMetrics(1);
        if w <= 0 || h <= 0 {
            ReleaseDC(std::ptr::null_mut(), dc);
            return Err("屏幕尺寸无效".into());
        }
        let mut out = [[0; 3]; 12];
        for (i, p) in out.iter_mut().enumerate() {
            let mut sum = [0u32; 3];
            let mut n = 0;
            for sy in 0..3 {
                for sx in 0..3 {
                    let x = ((i % 4) as i32 * w / 4 + (sx + 1) * w / 16).clamp(0, w - 1);
                    let y = ((i / 4) as i32 * h / 3 + (sy + 1) * h / 12).clamp(0, h - 1);
                    let rgb = GetPixel(dc, x, y);
                    if rgb != 0xffffffff {
                        for c in 0..3 {
                            sum[c] += (rgb >> (c * 8)) & 255;
                        }
                        n += 1;
                    }
                }
            }
            if n > 0 {
                for c in 0..3 {
                    p[c] = (sum[c] / n) as u8;
                }
            }
        }
        ReleaseDC(std::ptr::null_mut(), dc);
        Ok(out)
    }
}
