use std::{
    error::Error,
    fs::File,
    io::BufReader,
    net::{SocketAddrV4, UdpSocket},
    thread,
    time::Duration,
    vec::Vec
};

use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};

use rusb::{
    Context, Device, Direction, DeviceDescriptor, DeviceHandle,
    TransferType, UsbContext,
};

use serde::{Serialize, Deserialize};
use serde_json;

mod autocrap;

use autocrap::{
    config::{CtrlKind, CtrlNum, Config, Mapping, MidiKind},
    interpreter::{Interpreter}
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1000);

#[derive(Clone, Copy, Debug)]
struct Endpoint {
    config: u8,
    iface: u8,
    setting: u8,
    address: u8,
    transfer_type: TransferType,
    direction: Direction,
}

fn main() {
    run().unwrap();
}

fn run() -> Result<()> {
    let file = File::open("nocturn-config.json")?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader)?;
    println!("config: {:?}", config);

    let mut context = Context::new().unwrap();

    match open_device(&mut context, config.vendor_id, config.product_id) {
        Some((mut device, device_desc, mut handle)) => {
            handle.reset().unwrap();

            let languages = handle.read_languages(DEFAULT_TIMEOUT).unwrap();

            println!("active configuration: {}", handle.active_configuration().unwrap());
            println!("languages: {:?}", languages);

            if !languages.is_empty() {
                let language = languages[0];

                println!(
                    "manufacturer: {:?}",
                    handle
                        .read_manufacturer_string(language, &device_desc, DEFAULT_TIMEOUT)
                        .ok()
                );
                println!(
                    "product: {:?}",
                    handle
                        .read_product_string(language, &device_desc, DEFAULT_TIMEOUT)
                        .ok()
                );
                println!(
                    "serial number: {:?}",
                    handle
                        .read_serial_number_string(language, &device_desc, DEFAULT_TIMEOUT)
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

            let interpreter = Interpreter::new(&config);

            write_init(&mut handle, ctrl_out_endpoint.address).unwrap();

            thread::scope(|s| {
                let writer_thread = s.spawn(|| {
                    run_writer(&config, &handle, &ctrl_out_endpoint).unwrap();
                });

                run_reader(&config, &interpreter, &handle, &ctrl_in_endpoint).unwrap();

                writer_thread.join().unwrap();

                // handle.write_interrupt(ctrl_out_endpoint.address, &[0x00, 0x00], DEFAULT_TIMEOUT)?;
            });
        }
        None => println!("could not find device {:04x}:{:04x}", config.vendor_id, config.product_id),
    }

    Ok(())
}

fn write_init<T: UsbContext>(handle: &mut DeviceHandle<T>, address: u8) -> Result<()> {
    let write = |bytes| handle.write_interrupt(address, bytes, DEFAULT_TIMEOUT);

    // b0 looks to be a "start" byte, 00 00 is reset (all leds off)
    write(&[0xb0, 0x00, 0x00])?;

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

fn configure_endpoint<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    endpoint: &Endpoint,
) -> Result<()> {
    // handle.set_active_configuration(endpoint.config)?;
    handle.claim_interface(endpoint.iface)?;
    // handle.set_alternate_setting(endpoint.iface, endpoint.setting)?;
    Ok(())
}

fn run_reader<T: UsbContext>(config: &Config, interpreter: &Interpreter, handle: &DeviceHandle<T>, endpoint: &Endpoint) -> Result<()> {
    let sock = UdpSocket::bind(config.host_addr)?;

    let mut all_bytes = [0u8; 8];

    // let mut xfader_hi = 0x00u8;
    // let mut xfader_lo = 0x00u8;

    loop {
        if let Ok(num_bytes) = handle.read_interrupt(endpoint.address, &mut all_bytes, DEFAULT_TIMEOUT) {
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

                if let Some(response) = interpreter.handle_ctrl(num, val) {
                    if let Some((addr, args)) = response.osc {
                        let msg = OscPacket::Message(OscMessage {
                            addr: addr, // TODO: dont allocate every time
                            args: args,
                        });
                        println!("osc: {:?}", msg);
                        let msg_buf = encoder::encode(&msg)?;

                        sock.send_to(&msg_buf, config.osc_out_addr)?;
                    }

                    if let Some(out_bytes) = response.ctrl {
                        // TODO: gather, then send to writer after while loop
                        println!("ctrl: {:02x?}", out_bytes);
                    }
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
            }
        }
    }

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
                        // let Some(osc_match_data) = config.match_osc(&msg) else {
                        //     println!("unhandled osc message: with size {} from {}: {} {:?}", size, addr, msg.addr, msg.args);
                        //     continue;
                        // };

                        // println!("write: {:02x?}", osc_match_data.ctrl_data);
                        // handle.write_interrupt(endpoint.address, &osc_match_data.ctrl_data, DEFAULT_TIMEOUT)?;
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
