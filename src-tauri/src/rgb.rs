//! Bounded OpenRGB SDK v3 client. The server is strictly local.
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
type R<T> = Result<T, String>;
#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub key: String,
    pub id: u32,
    pub name: String,
    pub vendor: String,
    /// OpenRGB device_type (mouse=6), retained rather than inferred from names.
    pub kind: u32,
    pub leds: u16,
    pub direct: bool,
    pub active: u32,
    pub mode: Vec<u8>,
    pub colors: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: u32,
    pub key: String,
}
fn matches_target(d: &Device, t: &Target) -> bool {
    d.id == t.id && d.key == t.key
}
pub struct Client {
    stream: TcpStream,
    pub devices: Vec<Device>,
    pub topology_changed: bool,
}
fn io(e: std::io::Error) -> String {
    format!("OpenRGB：{e}")
}
impl Client {
    pub fn connect() -> R<Self> {
        let addr: SocketAddr = "127.0.0.1:6742".parse().unwrap();
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
            .map_err(|_| "电脑 RGB 服务尚未启动，先点击‘检测电脑灯光设备’".to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(io)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(io)?;
        stream.set_nodelay(true).map_err(io)?;
        let mut c = Self {
            stream,
            devices: Vec::new(),
            topology_changed: false,
        };
        c.send(0, 40, &3u32.to_le_bytes())?;
        let version = c.response(40, 0)?;
        if version.len() != 4 || u32::from_le_bytes(version[..4].try_into().unwrap()) < 3 {
            return Err("OpenRGB SDK 版本低于 3".into());
        }
        c.send(0, 50, b"Open HUB Lighting\0")?;
        c.send(0, 0, &[])?;
        let count = c.response(0, 0)?;
        if count.len() != 4 {
            return Err("OpenRGB 设备数量响应无效".into());
        }
        let n = u32::from_le_bytes(count[..4].try_into().unwrap());
        if n > 256 {
            return Err("OpenRGB 设备数量过多".into());
        }
        for id in 0..n {
            c.send(id, 1, &3u32.to_le_bytes())?;
            let data = c.response(1, id)?;
            c.devices.push(parse(id, &data)?);
        }
        Ok(c)
    }
    fn send(&mut self, device: u32, id: u32, data: &[u8]) -> R<()> {
        let mut header = Vec::from(*b"ORGB");
        header.extend(device.to_le_bytes());
        header.extend(id.to_le_bytes());
        header.extend((data.len() as u32).to_le_bytes());
        self.stream.write_all(&header).map_err(io)?;
        self.stream.write_all(data).map_err(io)
    }
    fn receive(&mut self) -> R<(u32, u32, Vec<u8>)> {
        let mut h = [0; 16];
        self.stream.read_exact(&mut h).map_err(io)?;
        if &h[..4] != b"ORGB" {
            return Err("OpenRGB 数据包头错误".into());
        }
        let id = u32::from_le_bytes(h[8..12].try_into().unwrap());
        let device = u32::from_le_bytes(h[4..8].try_into().unwrap());
        let size = u32::from_le_bytes(h[12..16].try_into().unwrap()) as usize;
        if size > 8 * 1024 * 1024 {
            return Err("OpenRGB 响应过大".into());
        }
        let mut b = vec![0; size];
        self.stream.read_exact(&mut b).map_err(io)?;
        if id == 100 {
            self.topology_changed = true;
            return Err("RGB 设备列表发生变化，已停止同步，请重新检测设备".into());
        }
        Ok((id, device, b))
    }
    fn response(&mut self, id: u32, device: u32) -> R<Vec<u8>> {
        for _ in 0..32 {
            let (i, d, b) = self.receive()?;
            if i == id && d == device {
                return Ok(b);
            }
        }
        Err("OpenRGB 响应不匹配".into())
    }
    pub fn select(&mut self, ids: &[Target]) -> R<Vec<Device>> {
        let mut selected = Vec::new();
        for target in ids {
            let id = &target.id;
            if selected.iter().any(|d: &Device| d.id == *id) {
                return Err("重复的 RGB 设备".into());
            }
            let d = self
                .devices
                .iter()
                .find(|d| d.id == *id)
                .ok_or("选中的 RGB 设备已断开，请重新检测")?
                .clone();
            if !matches_target(&d, target) {
                return Err("灯光目标的设备身份已改变，请重新检测并勾选，未写入任何灯光".into());
            }
            if !d.direct || d.leds == 0 {
                return Err(format!("{} 没有可用的 Direct 灯光模式", d.name));
            }
            selected.push(d);
        }
        Ok(selected)
    }
    pub fn begin(&mut self, devices: &[Device]) -> R<()> {
        for d in devices {
            self.send(d.id, 1100, &[])?;
        }
        Ok(())
    }
    pub fn poll(&mut self) -> R<()> {
        self.stream
            .set_read_timeout(Some(Duration::from_millis(1)))
            .map_err(io)?;
        let mut h = [0; 16];
        let peek = self.stream.peek(&mut h);
        self.stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(io)?;
        match peek {
            Ok(0) => Err("OpenRGB 服务器已退出".into()),
            Ok(n) if n >= 16 => {
                let _ = self.receive()?;
                Ok(())
            }
            Ok(_) => Ok(()),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(())
            }
            Err(e) => Err(io(e)),
        }
    }
    fn colors(&mut self, id: u32, colors: &[u8]) -> R<()> {
        let mut b = Vec::new();
        b.extend((6 + colors.len() as u32).to_le_bytes());
        b.extend(((colors.len() / 4) as u16).to_le_bytes());
        b.extend(colors);
        self.send(id, 1050, &b)
    }
    pub fn frame(&mut self, devices: &[Device], pixels: &[[u8; 3]; 12]) -> R<()> {
        self.poll()?;
        for d in devices {
            let mut colors = Vec::with_capacity(d.leds as usize * 4);
            for i in 0..d.leds as usize {
                let p = pixels[i * 12 / d.leds as usize];
                colors.extend([p[0], p[1], p[2], 0]);
            }
            self.colors(d.id, &colors)?;
        }
        Ok(())
    }
    pub fn restore(&mut self, devices: &[Device]) -> R<()> {
        if self.topology_changed {
            return Err("设备列表变动，为避免误写其他设备，已跳过旧编号的恢复".into());
        }
        self.poll()?;
        for d in devices {
            self.colors(d.id, &d.colors)?;
            let mut b = Vec::new();
            b.extend((8 + d.mode.len() as u32).to_le_bytes());
            b.extend(d.active.to_le_bytes());
            b.extend(&d.mode);
            self.send(d.id, 1101, &b)?;
        }
        Ok(())
    }
}
struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self.p.checked_add(n).ok_or("RGB 长度溢出")?;
        let v = self.b.get(self.p..end).ok_or("RGB 响应被截断")?;
        self.p = end;
        Ok(v)
    }
    fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn text(&mut self) -> R<String> {
        let n = self.u16()? as usize;
        let s = self.take(n)?;
        Ok(String::from_utf8_lossy(s)
            .trim_end_matches('\0')
            .to_string())
    }
}
fn parse(id: u32, b: &[u8]) -> R<Device> {
    let mut c = Cursor { b, p: 0 };
    let size = c.u32()? as usize;
    if size != b.len() {
        return Err("RGB 设备数据长度不符".into());
    }
    let kind = c.u32()?;
    let name = c.text()?;
    let vendor = c.text()?;
    let description = c.text()?;
    let version = c.text()?;
    let serial = c.text()?;
    let location = c.text()?;
    let key = format!("{name}|{vendor}|{description}|{version}|{serial}|{location}");
    let num = c.u16()?;
    let active = c.u32()?;
    if active >= num as u32 {
        return Err("RGB 当前模式越界".into());
    }
    let mut original = Vec::new();
    let mut direct = false;
    for i in 0..num {
        let start = c.p;
        let label = c.text()?.to_lowercase();
        direct |= label.contains("direct") || label.contains("custom");
        c.take(48)?;
        let n = c.u16()? as usize;
        c.take(n * 4)?;
        if i as u32 == active {
            original = b[start..c.p].to_vec();
        }
    }
    let zones = c.u16()?;
    for _ in 0..zones {
        c.text()?;
        c.take(16)?;
        let n = c.u16()? as usize;
        c.take(n)?;
    }
    let leds = c.u16()?;
    for _ in 0..leds {
        c.text()?;
        c.take(4)?;
    }
    let n = c.u16()? as usize;
    let colors = c.take(n * 4)?.to_vec();
    if n > 8192 || n != leds as usize {
        return Err("RGB LED 数量无效".into());
    }
    if c.p != b.len() {
        return Err("RGB 设备数据存在未解析字段".into());
    }
    Ok(Device {
        key,
        id,
        name,
        vendor,
        kind,
        leds,
        direct,
        active,
        mode: original,
        colors,
    })
}
#[derive(Default)]
pub struct Server(std::sync::Mutex<Option<std::process::Child>>);
impl Server {
    pub fn scan(&self) -> R<Vec<Device>> {
        if let Ok(c) = Client::connect() {
            return Ok(c.devices);
        }
        let mut child = self.0.lock().map_err(|_| "RGB 服务锁定失败")?;
        if child
            .as_mut()
            .and_then(|p| p.try_wait().ok())
            .flatten()
            .is_some()
        {
            *child = None
        }
        if child.is_none() {
            let base = std::env::current_exe().map_err(|e| e.to_string())?;
            let exe = base
                .parent()
                .ok_or("安装路径无效")?
                .join("rgb-engine/OpenRGB.exe");
            if !exe.exists() {
                return Err("安装包内的 RGB 引擎缺失，请重新安装".into());
            }
            let config = std::path::PathBuf::from(
                std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA 未设置")?,
            )
            .join("Open HUB/rgb-engine");
            std::fs::create_dir_all(&config).map_err(|e| e.to_string())?;
            let mut cmd = std::process::Command::new(exe);
            cmd.args([
                "--server",
                "--server-host",
                "127.0.0.1",
                "--server-port",
                "6742",
                "--noautoconnect",
                "--config",
            ])
            .arg(config);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            *child = Some(cmd.spawn().map_err(|e| e.to_string())?);
        }
        drop(child);
        for _ in 0..40 {
            if let Ok(c) = Client::connect() {
                return Ok(c.devices);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err("RGB 引擎未能启动，请检查设备兼容性或是否有其他灯光软件占用".into())
    }
    pub fn shutdown(&self) {
        if let Ok(mut c) = self.0.lock() {
            if let Some(mut p) = c.take() {
                let _ = p.kill();
                let _ = p.wait();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_controller_fixture() {
        let b = include_bytes!("../tests/fixtures/viewsonic-sdk3.bin");
        let d = parse(0, b).unwrap();
        assert_eq!(d.name, "ViewSonic Monitor XG270QG");
        assert_eq!(d.kind, 20);
        assert!(d.direct);
        assert!(d.leds > 0);
        for n in [0, 4, 10, b.len() - 1] {
            assert!(parse(0, &b[..n]).is_err())
        }
    }
}
#[cfg(test)]
mod kind_tests {
    use super::*;
    #[test]
    fn mouse_kind_is_read_from_protocol_not_product_name() {
        let mut b=include_bytes!("../tests/fixtures/viewsonic-sdk3.bin").to_vec();
        b[4..8].copy_from_slice(&6u32.to_le_bytes());
        let d=parse(0,&b).unwrap();
        assert_eq!(d.kind,6);
        assert_eq!(d.name,"ViewSonic Monitor XG270QG");
    }
}
#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn reused_device_number_cannot_take_over_another_brand() {
        let d = parse(0, include_bytes!("../tests/fixtures/viewsonic-sdk3.bin")).unwrap();
        assert!(matches_target(
            &d,
            &Target {
                id: 0,
                key: d.key.clone()
            }
        ));
        assert!(!matches_target(
            &d,
            &Target {
                id: 0,
                key: "Logitech|different-device".into()
            }
        ));
        assert!(!matches_target(
            &d,
            &Target {
                id: 1,
                key: d.key.clone()
            }
        ));
    }
}

impl Drop for Server{fn drop(&mut self){self.shutdown();}}
