use std::{
    error::Error,
    fs::File,
    io::BufReader,
    net::{UdpSocket},
    sync::{
        Arc, RwLock,
        mpsc
    },
    thread,
    time::Duration,
    vec::Vec
};

use midir::{
    MidiInput, MidiOutput,
    os::unix::{VirtualInput, VirtualOutput}
};
use rosc::encoder;
use rosc::{OscMessage, OscPacket};

use rusb::{
    Context, Device, Direction, DeviceDescriptor, DeviceHandle,
    TransferType, UsbContext,
};

use serde_json;

mod autocrap;

use autocrap::{
    config::{Config, Interface, MidiInterface, MidiPort, OscInterface},
    interpreter::{Interpreter, CtrlResponse, MidiResponse, OscResponse}
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
    let file = File::open("nocturn-midi.json")?;
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

            let interpreter = Arc::new(RwLock::new(Interpreter::new(&config)));
            let (receiver_ctrl_tx, ctrl_rx) = mpsc::channel();
            let reader_ctrl_tx = receiver_ctrl_tx.clone();

            write_init(&mut handle, ctrl_out_endpoint.address).unwrap();

            thread::scope(|s| {
                let writer_thread = s.spawn(|| {
                    run_writer(&handle, &ctrl_out_endpoint, ctrl_rx).unwrap();
                });

                let receiver_thread = s.spawn(|| {
                    match config.interface {
                        Interface::Midi(_) =>
                            run_midi_receiver(&config, &interpreter, receiver_ctrl_tx).unwrap(),
                        Interface::Osc(_) =>
                            run_osc_receiver(&config, &interpreter, receiver_ctrl_tx).unwrap(),
                    }
                });

                run_reader(&config, &interpreter, &handle, &ctrl_in_endpoint, reader_ctrl_tx).unwrap();

                receiver_thread.join().unwrap();
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

fn run_reader<T: UsbContext>(
    config: &Config,
    interpreter: &Arc<RwLock<Interpreter>>,
    handle: &DeviceHandle<T>,
    endpoint: &Endpoint,
    ctrl_tx: mpsc::Sender<Vec<u8>>
) -> Result<()> {
    let osc = if let Interface::Osc(OscInterface { host_addr, out_addr, .. }) = config.interface {
        let sock = UdpSocket::bind(host_addr)?;
        Some((sock, out_addr))
    } else {
        None
    };

    let mut midi = if let Interface::Midi(ref interface) = config.interface {
        let client_name = &interface.client_name;
        let midi_out = MidiOutput::new(client_name)?;
        match interface.out_port {
            MidiPort::Index(index) =>
                Some(midi_out.ports().remove(index))
                .map(|p| (midi_out.port_name(&p).unwrap(), midi_out.connect(&p, client_name).unwrap())),
            MidiPort::Name(ref name) =>
                midi_out.ports().into_iter().find(|p| &midi_out.port_name(&p).unwrap() == name)
                .map(|p| (midi_out.port_name(&p).unwrap(), midi_out.connect(&p, client_name).unwrap())),
            MidiPort::Virtual(ref name) =>
                Some((client_name.to_string(), midi_out.create_virtual(client_name).unwrap()))
        }
    } else {
        None
    };

    let mut all_bytes = [0u8; 8];

    loop {
        let Ok(num_bytes) =
            handle.read_interrupt(endpoint.address, &mut all_bytes, DEFAULT_TIMEOUT)
        else {
            continue;
        };

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

            let Some(response) = interpreter.write().unwrap().handle_ctrl(num, val) else {
                println!("unhandled data: {:02x?}", bytes);
                continue;
            };

            if let Some((sock, out_addr)) = osc.as_ref() {
                if let Some(OscResponse { addr, args }) = response.osc {
                    let msg = OscPacket::Message(OscMessage {
                        addr: addr,
                        args: args,
                    });
                    println!("osc: {:?}", msg);
                    let msg_buf = encoder::encode(&msg)?;

                    sock.send_to(&msg_buf, out_addr)?;
                }
            }

            if let Some((_, out_conn)) = midi.as_mut() {
                if let Some(MidiResponse { data }) = response.midi {
                    // println!("send midi: {:02x?}", data);
                    out_conn.send(&data)?;
                }
            }

            if let Some(CtrlResponse { data }) = response.ctrl {
                // println!("ctrl: {:02x?}", data);
                ctrl_tx.send(data)?;
            }
        }
    }
}

fn run_writer<T: UsbContext>(
    handle: &DeviceHandle<T>,
    endpoint: &Endpoint,
    ctrl_rx: mpsc::Receiver<Vec<u8>>
) -> Result<()> {
    loop {
        let ctrl_out = ctrl_rx.recv()?;
        handle.write_interrupt(endpoint.address, &ctrl_out, DEFAULT_TIMEOUT)?;
    }
}

fn run_osc_receiver(
    config: &Config,
    interpreter: &Arc<RwLock<Interpreter>>,
    ctrl_tx: mpsc::Sender<Vec<u8>>
) -> Result<()> {
    let Interface::Osc(OscInterface { in_addr, .. }) = config.interface else {
        return Ok(())
    };

    let sock = UdpSocket::bind(in_addr)?;
    println!("listening to {}", in_addr);

    let mut buf = [0u8; rosc::decoder::MTU];
    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, addr)) => {
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size])?;
                match packet {
                    OscPacket::Message(msg) => {
                        let Some(response) = interpreter.write().unwrap().handle_osc(&msg) else {
                            println!("unhandled osc message: with size {} from {}: {} {:?}", size, addr, msg.addr, msg.args);
                            continue;
                        };

                        let Some(CtrlResponse { data }) = response.ctrl else {
                            continue;
                        };

                        ctrl_tx.send(data)?
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

fn run_midi_receiver(
    config: &Config,
    interpreter: &Arc<RwLock<Interpreter>>,
    ctrl_tx: mpsc::Sender<Vec<u8>>
) -> Result<()> {
    let Interface::Midi(MidiInterface { ref client_name, ref in_port, .. }) = config.interface else {
        return Ok(())
    };

    let (tx, rx) = mpsc::channel();
    let midi_in = MidiInput::new(client_name).unwrap();
    match in_port {
        MidiPort::Index(index) =>
            Some(midi_in.ports().remove(*index))
            .map(|p| (midi_in.port_name(&p).unwrap(), midi_in.connect(
                &p,
                client_name,
                move |_time, msg, tx| {
                    tx.send(msg.to_vec()).unwrap();
                },
                tx
            ).unwrap())),
        MidiPort::Name(ref name) =>
            midi_in.ports().into_iter().find(|p| &midi_in.port_name(&p).unwrap() == name)
            .map(|p| (midi_in.port_name(&p).unwrap(), midi_in.connect(
                &p,
                client_name,
                move |_time, msg, tx| {
                    tx.send(msg.to_vec()).unwrap();
                },
                tx
            ).unwrap())),
        MidiPort::Virtual(ref name) =>
            Some((client_name.to_string(), midi_in.create_virtual(
                client_name,
                move |_time, msg, tx| {
                    tx.send(msg.to_vec()).unwrap();
                },
                tx
            ).unwrap()))
    };

    loop {
        let msg = rx.recv().unwrap();
        let Some(response) = interpreter.write().unwrap().handle_midi(&msg) else {
            println!("unhandled midi message: {:?}", msg);
            continue;
        };

        let Some(CtrlResponse { data }) = response.ctrl else {
            continue;
        };

        ctrl_tx.send(data)?
    }

    Ok(())
}
