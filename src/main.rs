use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::BufReader,
    net::{SocketAddrV4, UdpSocket},
    str::FromStr,
    thread,
    time::Duration,
    vec::Vec
};

use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};

use rusb::{
    ConfigDescriptor, Context, Device, Direction, DeviceDescriptor, DeviceHandle, DeviceList, EndpointDescriptor,
    InterfaceDescriptor, Language, Speed, TransferType, UsbContext,
};

use serde::{Serialize, Deserialize};
use serde_json;

use usb_ids::{self, FromId};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const default_timeout: Duration = Duration::from_millis(1000);

#[derive(Clone, Copy, Debug)]
struct Endpoint {
    config: u8,
    iface: u8,
    setting: u8,
    address: u8,
    transfer_type: TransferType,
    direction: Direction,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum CtrlNum {
    Single(u8),
    Range(u8, u8),
    Pair(u8, u8)
}

impl CtrlNum {
    fn match_num(&self, num: u8) -> Option<u8> {
        match *self {
            CtrlNum::Single(n) if num == n =>
                Some(0),
            CtrlNum::Range(lo, hi) if lo <= num && num <= hi =>
                Some(num - lo),
            // TODO: Pair
            _ =>
                None
        }
    }

    fn range_size(&self) -> u8 {
        match *self {
            CtrlNum::Single(_) => 1,
            CtrlNum::Range(lo, hi) => hi - lo + 1,
            _ => unimplemented!()
        }
    }

    fn index_to_num(&self, i: u8) -> Option<u8> {
        match *self {
            CtrlNum::Single(num) if i == 0 =>
                Some(num),
            CtrlNum::Range(lo, hi) if 0 <= i && i <= hi-lo =>
                Some(lo + i),
            CtrlNum::Pair(_, _) =>
                unimplemented!(),
            _ => None
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum CtrlKind {
    Button,
    EightBit,
    Relative,
}

impl CtrlKind {
    fn ctrl_to_osc(&self, val: u8) -> Vec<OscType> {
        match self {
            CtrlKind::Button =>
                vec![OscType::Float(if val == 0x7f { 1.0 } else { 0.0 })],
            CtrlKind::Relative =>
                vec![OscType::Float(if val < 0x40 { val as f32 } else { val as f32 - 128.0 })],
            _ => unimplemented!()
        }
    }

    fn osc_to_ctrl(&self, args: &[OscType]) -> Option<u8> {
        if args.len() < 1 {
            return None;
        }

        let OscType::Float(val) = args[0] else {
            return None;
        };

        match self {
            CtrlKind::Button =>
                Some(float_to_7bit(val)),
            CtrlKind::Relative =>
                Some(float_to_7bit(val)),
            _ => unimplemented!()
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum MidiKind {
    Cc,
    CoarseFine,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Mapping {
    name: String,
    ctrl_in_num: Option<CtrlNum>,
    ctrl_out_num: Option<CtrlNum>,
    ctrl_kind: CtrlKind,
    midi_kind: MidiKind,
    midi_num: CtrlNum
}

impl Mapping {
    fn osc_addr(&self, i: u8) -> String {
        format!("/{}", self.name.replace("{i}", &i.to_string()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Config {
    vendor_id: u16,
    product_id: u16,
    in_endpoint: u8,
    out_endpoint: u8,
    host_addr: SocketAddrV4,
    osc_out_addr: SocketAddrV4,
    osc_in_addr: SocketAddrV4,
    mappings: Vec<Mapping>
}

impl Config {
    fn match_ctrl(&self, num: u8, val: u8) -> Option<CtrlMatchData> {
        for mapping in self.mappings.iter() {
            let Some(ctrl_in_num) = mapping.ctrl_in_num else {
                continue;
            };

            let Some(i) = ctrl_in_num.match_num(num) else {
                continue;
            };

            return Some(CtrlMatchData {
                osc_addr: mapping.osc_addr(i),
                osc_args: mapping.ctrl_kind.ctrl_to_osc(val)
            })
        }

        None
    }

    fn match_osc(&self, msg: &OscMessage) -> Option<OscMatchData> {
        for mapping in self.mappings.iter() {
            let Some(ctrl_out_num) = mapping.ctrl_out_num else {
                continue;
            };

            for i in 0..ctrl_out_num.range_size() {
                let addr = mapping.osc_addr(i);

                if addr != msg.addr {
                    continue;
                }

                let Some(num) = ctrl_out_num.index_to_num(i) else {
                    continue;
                };

                let Some(val) = mapping.ctrl_kind.osc_to_ctrl(&msg.args) else {
                    continue;
                };

                return Some(OscMatchData {
                    ctrl_data: vec![num, val]
                });
            }
        }

        None
    }
}

#[derive(Clone, Debug)]
struct CtrlMatchData {
    osc_addr: String,
    osc_args: Vec<OscType>,
}

#[derive(Clone, Debug)]
struct OscMatchData {
    ctrl_data: Vec<u8>,
}

fn main() {
    run().unwrap();
}

fn run() -> Result<()> {
    let file = File::open("nocturn-config.json")?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader)?;
    println!("config: {:?}", config);

    let sock = UdpSocket::bind(config.host_addr).unwrap();

    let mut context = Context::new().unwrap();

    match open_device(&mut context, config.vendor_id, config.product_id) {
        Some((mut device, device_desc, mut handle)) => {
            handle.reset().unwrap();

            let buttons: Vec<u8> = (0x70u8 .. 0x86u8).collect();
            let timeout = Duration::from_secs(1);
            let languages = handle.read_languages(timeout).unwrap();

            println!("active configuration: {}", handle.active_configuration().unwrap());
            println!("languages: {:?}", languages);

            if !languages.is_empty() {
                let language = languages[0];

                println!(
                    "manufacturer: {:?}",
                    handle
                        .read_manufacturer_string(language, &device_desc, timeout)
                        .ok()
                );
                println!(
                    "product: {:?}",
                    handle
                        .read_product_string(language, &device_desc, timeout)
                        .ok()
                );
                println!(
                    "serial number: {:?}",
                    handle
                        .read_serial_number_string(language, &device_desc, timeout)
                        .ok()
                );
            }

            let ctrl_in_endpoint = find_endpoint(&mut device, &device_desc, |e| e.config == config.in_endpoint && e.transfer_type == TransferType::Interrupt && e.direction == Direction::In)
                .ok_or("control out endpoint not found").unwrap();
            let ctrl_out_endpoint = find_endpoint(&mut device, &device_desc, |e| e.config == config.out_endpoint && e.transfer_type == TransferType::Interrupt && e.direction == Direction::Out)
                .ok_or("control out endpoint not found").unwrap();

            println!("control in endpoint: {:?}", ctrl_in_endpoint);
            println!("control out endpoint: {:?}", ctrl_out_endpoint);

            configure_endpoint(&mut handle, &ctrl_in_endpoint).unwrap();
            configure_endpoint(&mut handle, &ctrl_out_endpoint).unwrap();

            experiment_send_mini_init(&mut handle, ctrl_out_endpoint.address).unwrap();
            // experiment_send_init(&mut handle, ctrl_out_endpoint.address).unwrap();
            // return Ok(());

            thread::scope(|s| {
                let writer_thread = s.spawn(|| {
                    run_writer(&config, &handle, &ctrl_out_endpoint);
                });

                let mut all_bytes = [0u8; 8];

                let mut xfader_hi = 0x00u8;
                let mut xfader_lo = 0x00u8;

                loop {
                    if let Ok(num_bytes) = handle.read_interrupt(ctrl_in_endpoint.address, &mut all_bytes, default_timeout) {
                        // println!("read({:?}): {:02x?}", num_bytes, &all_bytes[..num_bytes]);
                        let mut i = 0;
                        while i+1 < num_bytes {
                            if all_bytes[i] == 0xb0 {
                                i += 1;
                                continue
                            }

                            let bytes = &all_bytes[i..i+2];
                            i += 2;

                            // println!("bytes: {:02x?}", bytes);

                            let num = bytes[0];
                            let val = bytes[1];

                            let addr: String;
                            let args: Vec<OscType>;

                            if let Some(data) = config.match_ctrl(num, val) {
                                addr = data.osc_addr;
                                args = data.osc_args;
                            } else {
                                println!("unhandled data: {:02x?}", bytes);
                                continue;
                            }

                            // } else if num == 0x48 {
                            //     // xfader hi
                            //     xfader_hi = val;
                            //     continue;
                            // } else if num == 0x49 {
                            //     // xfader lo
                            //     xfader_lo = val;
                            //     let val8 = (xfader_hi << 1) | (if xfader_lo != 0x00 { 1 } else { 0 });

                            //     addr = "/xfader".to_string();
                            //     args = vec![OscType::Float(val8 as f32 / 255.0)];

                            let msg = OscPacket::Message(OscMessage {
                                addr: addr.to_string(), // TODO: dont allocate every time
                                args: args,
                            });
                            // println!("osc: {:?}", msg);
                            let msg_buf = encoder::encode(&msg).unwrap();

                            sock.send_to(&msg_buf, config.osc_out_addr).unwrap();
                        }
                    }
                }

                writer_thread.join().unwrap();

                // handle.write_interrupt(ctrl_out_endpoint.address, &[0x00, 0x00], default_timeout)?;
            });
        }
        None => println!("could not find device {:04x}:{:04x}", config.vendor_id, config.product_id),
    }

    Ok(())
}

fn experiment_send_mini_init<T: UsbContext>(handle: &mut DeviceHandle<T>, address: u8) -> Result<()> {
    let write = |bytes| handle.write_interrupt(address, bytes, default_timeout);

    // b0 looks to be a "start" byte, 00 00 is reset (all leds off)
    write(&[0xb0, 0x00, 0x00])?;
    // ?
    // write(&[0x28, 0x00, 0x2b, 0x4a, 0x2c, 0x00, 0x2e, 0x35])?;
    // ?
    // write(&[0x2a, 0x02, 0x2c, 0x72, 0x2e, 0x30])?;
    // set knob 0, button 0, knob 1, button 1
    // write(&[0x40, 0x00, 0x70, 0x00, 0x41, 0x00, 0x71, 0x00])?;
    // set knob 2, button 2, knob 3, button 3
    // write(&[0x42, 0x00, 0x72, 0x00, 0x43, 0x00, 0x73, 0x00])?;
    // set knob 4, button 4, knob 5, button 5
    // write(&[0x44, 0x00, 0x74, 0x00, 0x45, 0x00, 0x75, 0x00])?;
    // set knob 6, button 6, knob 7, button 7
    // write(&[0x46, 0x00, 0x76, 0x00, 0x47, 0x00, 0x77, 0x00])?;
    // set button 9, button 10, button 11, speed dial
    // write(&[0x79, 0x00, 0x7a, 0x00, 0x7b, 0x00, 0x50, 0x00])?;
    // set ?, knob 0, ?, knob 1
    // write(&[0x48, 0x00, 0x40, 0x0c, 0x49, 0x00, 0x41, 0x0c])?;
    // set ?, knob 2, ?, knob 3
    // write(&[0x4a, 0x00, 0x42, 0x0c, 0x4b, 0x00, 0x43, 0x0c])?;
    // set ?, knob 4, ?, knob 5
    // write(&[0x4c, 0x00, 0x44, 0x0c, 0x4d, 0x00, 0x45, 0x0c])?;
    // set ?, knob 6, ?, knob 7
    // write(&[0x4e, 0x00, 0x46, 0x0c, 0x4f, 0x00, 0x47, 0x0c])?;
    // set buttons 8,12,13,14
    // write(&[0x78, 0x00, 0x7c, 0x00, 0x7d, 0x00, 0x7e, 0x00])?;
    // set button 15
    // write(&[0x7f, 0x00])?;
    // set knob 0, knob 1, knob 2, knob 3
    // write(&[0x40, 0x00, 0x41, 0x00, 0x42, 0x00, 0x43, 0x00])?;
    // set knob 4, knob 5, knob 6, knob 7
    // write(&[0x44, 0x00, 0x45, 0x00, 0x46, 0x00, 0x47, 0x00])?;
    // set knob 0, knob 1, knob 2, knob 3
    // write(&[0x40, 0x0c, 0x41, 0x0c, 0x42, 0x0c, 0x43, 0x0c])?;
    // set knob 4, knob 5, knob 6, knob 7
    // write(&[0x44, 0x0c, 0x45, 0x0c, 0x46, 0x0c, 0x47, 0x0c])?;

    Ok(())
}

fn experiment_send_init<T: UsbContext>(handle: &mut DeviceHandle<T>, address: u8) -> Result<()> {
    let write = |bytes| handle.write_interrupt(address, bytes, default_timeout);

    // b0 looks to be a "start" byte, 00 00 is reset (all leds off)
    write(&[0xb0, 0x00, 0x00])?;
    // ?
    write(&[0x28, 0x00, 0x2b, 0x4a, 0x2c, 0x00, 0x2e, 0x35])?;
    // ?
    write(&[0x2a, 0x02, 0x2c, 0x72, 0x2e, 0x30])?;
    // set knob 0, button 0, knob 1, button 1
    write(&[0x40, 0x00, 0x70, 0x00, 0x41, 0x00, 0x71, 0x00])?;
    // set knob 2, button 2, knob 3, button 3
    write(&[0x42, 0x00, 0x72, 0x00, 0x43, 0x00, 0x73, 0x00])?;
    // set knob 4, button 4, knob 5, button 5
    write(&[0x44, 0x00, 0x74, 0x00, 0x45, 0x00, 0x75, 0x00])?;
    // set knob 6, button 6, knob 7, button 7
    write(&[0x46, 0x00, 0x76, 0x00, 0x47, 0x00, 0x77, 0x00])?;
    // set button 9, button 10, button 11, speed dial
    write(&[0x79, 0x00, 0x7a, 0x00, 0x7b, 0x00, 0x50, 0x00])?;
    // set ?, knob 0, ?, knob 1
    write(&[0x48, 0x00, 0x40, 0x0c, 0x49, 0x00, 0x41, 0x0c])?;
    // set ?, knob 2, ?, knob 3
    write(&[0x4a, 0x00, 0x42, 0x0c, 0x4b, 0x00, 0x43, 0x0c])?;
    // set ?, knob 4, ?, knob 5
    write(&[0x4c, 0x00, 0x44, 0x0c, 0x4d, 0x00, 0x45, 0x0c])?;
    // set ?, knob 6, ?, knob 7
    write(&[0x4e, 0x00, 0x46, 0x0c, 0x4f, 0x00, 0x47, 0x0c])?;
    // set buttons 8,12,13,14
    write(&[0x78, 0x00, 0x7c, 0x00, 0x7d, 0x00, 0x7e, 0x00])?;
    // set button 15
    write(&[0x7f, 0x00])?;
    // set knob 0, knob 1, knob 2, knob 3
    write(&[0x40, 0x00, 0x41, 0x00, 0x42, 0x00, 0x43, 0x00])?;
    // set knob 4, knob 5, knob 6, knob 7
    write(&[0x44, 0x00, 0x45, 0x00, 0x46, 0x00, 0x47, 0x00])?;
    // set knob 0, knob 1, knob 2, knob 3
    write(&[0x40, 0x0c, 0x41, 0x0c, 0x42, 0x0c, 0x43, 0x0c])?;
    // set knob 4, knob 5, knob 6, knob 7
    write(&[0x44, 0x0c, 0x45, 0x0c, 0x46, 0x0c, 0x47, 0x0c])?;

    Ok(())
}

fn experiment_send_captured_init<T: UsbContext>(handle: &mut DeviceHandle<T>, address: u8) -> Result<()> {
    let write = |bytes| handle.write_interrupt(address, bytes, default_timeout);

    // b0 some general "start" byte?
    write(&[0xb0, 0x00, 0x00])?;
    // ?
    write(&[0x28, 0x00, 0x2b, 0x4a, 0x2c, 0x00, 0x2e, 0x35])?;
    // ?
    write(&[0x2a, 0x02, 0x2c, 0x72, 0x2e, 0x30])?;
    // set knob 0, button 0, ?, button 1
    write(&[0x40, 0x00, 0x70, 0x00, 0x31, 0x00, 0x71, 0x00])?;
    // set knob 2, button 2, knob 3, button 3
    write(&[0x42, 0x00, 0x72, 0x00, 0x43, 0x00, 0x73, 0x00])?;
    // set knob 4, button 4, knob 5, button 5
    write(&[0x44, 0x00, 0x74, 0x00, 0x45, 0x00, 0x75, 0x00])?;
    // set knob 6, button 6, knob 6, button 7
    write(&[0x46, 0x00, 0x76, 0x00, 0x47, 0x00, 0x77, 0x00])?;
    // set button 9, button 10, button 11, speed dial
    write(&[0x79, 0x00, 0x7a, 0x00, 0x7b, 0x00, 0x50, 0x7f])?;
    // set ?, knob 0, ?, knob 1
    write(&[0x48, 0x00, 0x40, 0x0c, 0x49, 0x00, 0x41, 0x0c])?;
    // set ?, knob 2, ?, knob 3
    write(&[0x4a, 0x00, 0x42, 0x0c, 0x4b, 0x00, 0x43, 0x0c])?;
    // set ?, knob 4, ?, knob 5
    write(&[0x4c, 0x00, 0x44, 0x0c, 0x4d, 0x00, 0x45, 0x0c])?;
    // set ?, knob 6, ?, knob 7
    write(&[0x4e, 0x00, 0x46, 0x0c, 0x4f, 0x00, 0x47, 0x0c])?;
    // set buttons 8,12,13,14 - 12(user) on
    write(&[0x78, 0x00, 0x7c, 0x7f, 0x7d, 0x00, 0x7e, 0x00])?;
    // set button 15
    write(&[0x7f, 0x00])?;
    // set knob 0, knob 1, knob 2, knob 3
    write(&[0x40, 0x00, 0x41, 0x00, 0x42, 0x00, 0x43, 0x00])?;
    // set knob 4, knob 5, knob 6, knob 7
    write(&[0x44, 0x00, 0x45, 0x00, 0x46, 0x00, 0x47, 0x00])?;
    // set knob 0, knob 1, knob 2, knob 3
    write(&[0x40, 0x0c, 0x41, 0x0c, 0x42, 0x0c, 0x43, 0x0c])?;
    // set knob 4, knob 5, knob 6, knob 7
    write(&[0x44, 0x0c, 0x45, 0x0c, 0x46, 0x0c, 0x47, 0x0c])?;

    Ok(())
}

fn open_device<T: UsbContext>(
    context: &mut T,
    vid: u16,
    pid: u16,
) -> Option<(Device<T>, DeviceDescriptor, DeviceHandle<T>)> {
    let devices = match context.devices() {
        Ok(d) => d,
        Err(_) => return None,
    };

    for device in devices.iter() {
        let device_desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        if device_desc.vendor_id() == vid && device_desc.product_id() == pid {
            match device.open() {
                Ok(handle) => return Some((device, device_desc, handle)),
                Err(e) => panic!("Device found but failed to open: {}", e),
            }
        }
    }

    None
}

fn find_endpoints<T: UsbContext>(
    device: &mut Device<T>,
    device_desc: &DeviceDescriptor,
) -> Vec<(u8, u8, u8, u8, TransferType, Direction)> {
    let mut endpoints = Vec::new();

    for n in 0..device_desc.num_configurations() {
        let config_desc = match device.config_descriptor(n) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                for endpoint_desc in interface_desc.endpoint_descriptors() {
                    endpoints.push((
                        config_desc.number(),
                        interface_desc.interface_number(),
                        interface_desc.setting_number(),
                        endpoint_desc.address(),
                        endpoint_desc.transfer_type(),
                        endpoint_desc.direction()
                    ));
                    // if endpoint_desc.direction() == Direction::In
                    //     && endpoint_desc.transfer_type() == transfer_type
                    // {

                    //     return Some(Endpoint {
                    //         config: config_desc.number(),
                    //         iface: interface_desc.interface_number(),
                    //         setting: interface_desc.setting_number(),
                    //         address: endpoint_desc.address(),
                    //     });
                    // }
                }
            }
        }
    }

    endpoints
}

fn find_endpoint<T: UsbContext>(
    device: &mut Device<T>,
    device_desc: &DeviceDescriptor,
    predicate: impl Fn(Endpoint) -> bool
) -> Option<Endpoint> {
    for n in 0..device_desc.num_configurations() {
        let config_desc = match device.config_descriptor(n) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                for endpoint_desc in interface_desc.endpoint_descriptors() {
                    let endpoint = Endpoint {
                        config: config_desc.number(),
                        iface: interface_desc.interface_number(),
                        setting: interface_desc.setting_number(),
                        address: endpoint_desc.address(),
                        transfer_type: endpoint_desc.transfer_type(),
                        direction: endpoint_desc.direction()
                    };

                    if predicate(endpoint) {
                        return Some(endpoint);
                    }
                }
            }
        }
    }

    None
}

fn read_endpoint<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    endpoint: Endpoint,
    transfer_type: TransferType,
) {
    println!("Reading from endpoint: {:?}", endpoint);

    let has_kernel_driver = match handle.kernel_driver_active(endpoint.iface) {
        Ok(true) => {
            handle.detach_kernel_driver(endpoint.iface).ok();
            true
        }
        _ => false,
    };

    println!(" - kernel driver? {}", has_kernel_driver);

    match configure_endpoint(handle, &endpoint) {
        Ok(_) => {
            let mut buf = [0; 256];
            let timeout = Duration::from_secs(1);

            match transfer_type {
                TransferType::Interrupt => {
                    match handle.read_interrupt(endpoint.address, &mut buf, timeout) {
                        Ok(len) => {
                            println!(" - read: {:?}", &buf[..len]);
                        }
                        Err(err) => println!("could not read from endpoint: {}", err),
                    }
                }
                TransferType::Bulk => match handle.read_bulk(endpoint.address, &mut buf, timeout) {
                    Ok(len) => {
                        println!(" - read: {:?}", &buf[..len]);
                    }
                    Err(err) => println!("could not read from endpoint: {}", err),
                },
                _ => (),
            }
        }
        Err(err) => println!("could not configure endpoint: {}", err),
    }

    if has_kernel_driver {
        handle.attach_kernel_driver(endpoint.iface).ok();
    }
}

fn configure_endpoint<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    endpoint: &Endpoint,
) -> Result<()> {
    // handle.set_active_configuration(endpoint.config)?;
    handle.claim_interface(endpoint.iface)?;
    // handle.set_alternate_setting(endpoint.iface, endpoint.setting)?;
    Ok(())
}

fn float_to_7bit(val: f32) -> u8 {
    (val.max(0.0).min(1.0) * 127.0).round() as u8
}

fn run_writer<T: UsbContext>(config: &Config, handle: &DeviceHandle<T>, endpoint: &Endpoint) -> Result<()> {
    let sock = UdpSocket::bind(config.osc_in_addr)?;
    println!("listening to {}", config.osc_in_addr);

    let mut buf = [0u8; rosc::decoder::MTU];
    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, addr)) => {
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size])?;
                match packet {
                    OscPacket::Message(msg) => {
                        let Some(osc_match_data) = config.match_osc(&msg) else {
                            println!("unhandled osc message: with size {} from {}: {} {:?}", size, addr, msg.addr, msg.args);
                            continue;
                        };

                        println!("write: {:02x?}", osc_match_data.ctrl_data);
                        handle.write_interrupt(endpoint.address, &osc_match_data.ctrl_data, default_timeout)?;
                    }
                    OscPacket::Bundle(bundle) => {
                        println!("OSC Bundle: {:?}", bundle);
                    }
                }
            }
            Err(e) => {
                println!("error receiving from socket: {}", e);
                break;
            }
        }
    }

    Ok(())
}
