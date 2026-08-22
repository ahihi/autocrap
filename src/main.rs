use std::{
    error::Error,
    fs::File,
    io::{BufReader, Write},
    net::UdpSocket,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc
    },
    thread,
    time::{Duration, Instant},
    vec::Vec
};

use clap::Parser;
use colog;
use log::{error, warn, info, debug, trace};
use midir::{
    MidiInput, MidiOutput,
};
#[cfg(unix)]
use midir::os::unix::{VirtualInput, VirtualOutput};

use rosc::encoder;
use rosc::{OscMessage, OscPacket};

use rusb::{
    Context, Device, Direction, DeviceDescriptor, DeviceHandle,
    TransferType, UsbContext,
};

use serde_json;

mod autocrap;

use autocrap::{
    config::{Config, DeviceCalibration, Interface, MidiInterface, MidiPort, OscInterface},
    interpreter::{Interpreter, CtrlResponse, MidiResponse, OscResponse}
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1000);

/// How long to wait before looking for the device again after it has gone away.
const DEVICE_RETRY_INTERVAL: Duration = Duration::from_millis(1000);

/// How often an inbound receiver wakes to check whether the process is stopping.
const RECEIVER_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How often the writer checks whether its device session has ended.
const WRITER_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long the sweep across every light lasts when a device connects.
const CONNECT_ANIMATION_DURATION: Duration = Duration::from_millis(1000);

/// The value that lights an indicator fully, whether it is a button lamp or an
/// encoder's LED ring.
const INDICATOR_FULLY_LIT: u8 = 0x7f;
const INDICATOR_UNLIT: u8 = 0x00;

#[derive(Clone, Copy, Debug)]
struct Endpoint {
    config: u8,
    iface: u8,
    setting: u8,
    address: u8,
    transfer_type: TransferType,
    direction: Direction,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Options {
    /// Set a config file
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,

    /// Set logging level
    #[arg(short, long)]
    log: Option<String>,

    /// Print machine-readable status events to stdout, for a front end to read
    #[arg(long)]
    status: bool,

    /// Corrections for this particular device's faults, applied whichever config is loaded
    #[arg(long, value_name = "FILE")]
    calibration: Option<PathBuf>,
}

/// Set when the user asks the process to stop, so the loops can unwind normally.
///
/// Exiting cleanly is what lets the USB interface be released. A process torn down
/// mid-syscall leaves the interface claimed, and the device then needs a physical
/// replug before anything can talk to it again.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn request_shutdown(_signal: libc::c_int) {
    // storing to an atomic is one of the few things safe to do in a signal handler
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
}

fn install_shutdown_handler() {
    unsafe {
        let handler = request_shutdown as extern "C" fn(libc::c_int);
        libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, handler as libc::sighandler_t);
    }
}

fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}

/// Writes machine-readable status events for a front end, when asked to.
///
/// Events go to stdout while logs go to stderr, so a reader can take each stream
/// for its own purpose without having to disentangle them.
struct StatusReporter {
    enabled: bool
}

impl StatusReporter {
    /// The sentinel that marks a line as a status event rather than incidental output.
    const SENTINEL: &'static str = "@autocrap";

    fn report(&self, key: &str, value: impl std::fmt::Display) {
        if !self.enabled {
            return;
        }

        println!("{} {}={}", Self::SENTINEL, key, value);
        // a front end reacts to these as they happen, so they cannot sit in a buffer
        let _ = std::io::stdout().flush();
    }
}

fn main() {
    run().unwrap();
}

fn run() -> Result<()> {
    let options = Options::parse();

    let mut colog_builder = colog::default_builder();
    if let Some(ref filters_str) = options.log {
        colog_builder.parse_filters(filters_str);
    }
    colog_builder.init();

    install_shutdown_handler();

    let file = File::open(&options.config)?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader)?;
    config.validate()?;
    info!("config: {:?}", config);

    // deliberately separate from the config: a worn encoder is a property of the
    // hardware, and its correction must survive switching between configs
    let calibration = match options.calibration {
        Some(ref path) => {
            let calibration: DeviceCalibration =
                serde_json::from_reader(BufReader::new(File::open(path)?))?;
            info!("calibration: {:?}", calibration);
            calibration
        },
        None => DeviceCalibration::default()
    };

    let status = StatusReporter { enabled: options.status };
    status.report("config", config.name.clone()
        .unwrap_or_else(|| options.config.file_stem()
            .map_or_else(|| "config".to_string(), |stem| stem.to_string_lossy().into_owned())));

    let mut context = Context::new().unwrap();

    // everything below outlives any single device session, so that unplugging the
    // device does not disturb the applications connected to autocrap, and does not
    // lose the values each control is holding
    let interpreter = Arc::new(RwLock::new(Interpreter::new(&config, &calibration)));
    status.report("banks", interpreter.read().unwrap().bank_count());

    let mut outbound = OutboundInterface::open(&config)?;
    let (ctrl_tx, ctrl_rx) = mpsc::channel();
    // shared rather than moved, so the queue outlives each device session
    let ctrl_rx = Mutex::new(ctrl_rx);
    let receiver_ctrl_tx = ctrl_tx.clone();

    thread::scope(|program_scope| {
        program_scope.spawn(|| {
            match config.interface {
                Interface::Midi(_) =>
                    run_midi_receiver(&config, &interpreter, receiver_ctrl_tx).unwrap(),
                Interface::Osc(_) =>
                    run_osc_receiver(&config, &interpreter, receiver_ctrl_tx).unwrap(),
            }
        });

        let mut absence_already_reported = false;

        loop {
            if shutdown_requested() {
                return Ok(());
            }

            match run_device_session(
                &mut context, &config, &interpreter, &mut outbound, &ctrl_tx, &ctrl_rx, &status
            ) {
                Ok(()) => return Ok(()),
                Err(reason) => {
                    // the device being absent is the normal state while it is
                    // unplugged, so it is reported once rather than every retry
                    if !absence_already_reported {
                        status.report("device", "absent");
                        warn!("device unavailable ({}), waiting for it to appear", reason);
                        absence_already_reported = true;
                    }
                }
            }

            thread::sleep(DEVICE_RETRY_INTERVAL);

            if is_device_present(&mut context, config.vendor_id, config.product_id) {
                absence_already_reported = false;
            }
        }
    })
}

/// Runs one connected-device session, returning only when the device stops responding.
fn run_device_session(
    context: &mut Context,
    config: &Config,
    interpreter: &Arc<RwLock<Interpreter>>,
    outbound: &mut OutboundInterface,
    ctrl_tx: &mpsc::Sender<Vec<u8>>,
    ctrl_rx: &Mutex<mpsc::Receiver<Vec<u8>>>,
    status: &StatusReporter
) -> Result<()> {
    let Some((mut device, device_desc, mut handle)) =
        open_device(context, config.vendor_id, config.product_id)
    else {
        return Err(format!("no device {:04x}:{:04x}", config.vendor_id, config.product_id).into());
    };

    // NOTE: on macOS, a real USB bus reset here makes the device
    // re-enumerate and invalidates this handle, causing intermittent
    // claim_interface() failures (NotFound/NoDevice) afterwards.
    // The device is already in a usable configured state after
    // open_device(), so skip the reset on macOS.
    #[cfg(not(target_os = "macos"))]
    handle.reset()?;

    let languages = handle.read_languages(DEFAULT_TIMEOUT)?;

    info!("active configuration: {}", handle.active_configuration()?);
    info!("languages: {:?}", languages);

    if !languages.is_empty() {
        let language = languages[0];

        info!("manufacturer: {:?}",
            handle.read_manufacturer_string(language, &device_desc, DEFAULT_TIMEOUT).ok());
        info!("product: {:?}",
            handle.read_product_string(language, &device_desc, DEFAULT_TIMEOUT).ok());
        info!("serial number: {:?}",
            handle.read_serial_number_string(language, &device_desc, DEFAULT_TIMEOUT).ok());
    }

    let ctrl_in_endpoint = find_endpoint(&mut device, &device_desc, |e| e.config == config.in_endpoint && e.transfer_type == TransferType::Interrupt && e.direction == Direction::In)
        .ok_or("control in endpoint not found")?;
    let ctrl_out_endpoint = find_endpoint(&mut device, &device_desc, |e| e.config == config.out_endpoint && e.transfer_type == TransferType::Interrupt && e.direction == Direction::Out)
        .ok_or("control out endpoint not found")?;

    info!("control in endpoint: {:?}", ctrl_in_endpoint);
    info!("control out endpoint: {:?}", ctrl_out_endpoint);

    match handle.set_auto_detach_kernel_driver(true) {
        Ok(()) => Ok(()),
        Err(rusb::Error::NotSupported) => Ok(()),
        err => err
    }?;

    // the device can end up reporting an "unconfigured" active
    // configuration (0), e.g. after a previous run didn't clean up.
    // explicitly (re)select a configuration so claim_interface()
    // below has something to claim. config 1 and config 2 expose
    // identical interface/endpoint layouts on this device, so
    // ctrl_in_endpoint's configuration works for both.
    handle.set_active_configuration(ctrl_in_endpoint.config)?;

    configure_endpoint(&mut handle, &ctrl_in_endpoint)?;
    configure_endpoint(&mut handle, &ctrl_out_endpoint)?;

    info!("device connected");
    status.report("device", "connected");
    status.report("bank", interpreter.read().unwrap().active_bank_index());

    // writes queued while the device was away would paint stale values over the
    // refresh below, so they are discarded rather than delivered late
    while ctrl_rx.lock().unwrap().try_recv().is_ok() {}

    write_init(&mut handle, ctrl_out_endpoint.address)?;

    play_connect_animation(
        &mut handle, ctrl_out_endpoint.address, &config.indicator_control_numbers()
    )?;

    // write_init cleared every LED, but the interpreter still holds the values the
    // controls had before the device went away, so the hardware is repainted to match
    for response in interpreter.read().unwrap().build_device_refresh_writes() {
        if let Some(CtrlResponse { data }) = response.ctrl {
            ctrl_tx.send(data)?;
        }
    }

    let session_is_live = AtomicBool::new(true);

    thread::scope(|session_scope| {
        let writer_thread = session_scope.spawn(|| {
            // a write failing means the device has gone; the reader reports that,
            // so this only needs to stop rather than raise its own alarm
            if let Err(reason) = run_writer(&handle, &ctrl_out_endpoint, ctrl_rx, &session_is_live) {
                debug!("writer stopped: {}", reason);
            }
        });

        let reader_outcome = run_reader(
            interpreter, outbound, &handle, &ctrl_in_endpoint, ctrl_tx.clone(), status
        );

        session_is_live.store(false, Ordering::Relaxed);
        let _ = writer_thread.join();

        reader_outcome
    })?;

    if shutdown_requested() {
        info!("shutting down");
        // leave the device dark rather than displaying values from a program that
        // is no longer running. best effort: it may already have been unplugged.
        let _ = write_init(&mut handle, ctrl_out_endpoint.address);
    }

    // dropping the handle here releases the claimed interface
    Ok(())
}

/// Whether the configured device is currently attached, without opening it.
fn is_device_present(context: &mut Context, vendor_id: u16, product_id: u16) -> bool {
    let Ok(devices) = context.devices() else {
        return false;
    };

    devices.iter().any(|device| match device.device_descriptor() {
        Ok(descriptor) =>
            descriptor.vendor_id() == vendor_id && descriptor.product_id() == product_id,
        Err(_) => false
    })
}

/// Sweeps every light on and then off again, to show the device is properly connected.
///
/// Written straight to the endpoint rather than through the write queue, because the
/// queue is drained by a thread that has not started yet, and because the timing has
/// to be paced rather than delivered as fast as the device will take it.
fn play_connect_animation<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    address: u8,
    indicator_control_numbers: &[u8]
) -> Result<()> {
    let lit_then_unlit = indicator_control_numbers.iter().map(|num| (*num, INDICATOR_FULLY_LIT))
        .chain(indicator_control_numbers.iter().map(|num| (*num, INDICATOR_UNLIT)));
    let frame_count = indicator_control_numbers.len() as u32 * 2;

    if frame_count == 0 {
        return Ok(());
    }

    let started_at = Instant::now();

    for (frame_index, (control_number, brightness)) in lit_then_unlit.enumerate() {
        handle.write_interrupt(address, &[control_number, brightness], DEFAULT_TIMEOUT)?;

        // paced against the start rather than by sleeping a fixed amount, so the
        // sweep still takes the same time however slow the writes turn out to be
        let frame_ends_at = started_at
            + CONNECT_ANIMATION_DURATION * (frame_index as u32 + 1) / frame_count;
        if let Some(remaining) = frame_ends_at.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
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
    info!("configure_endpoint {:?}", endpoint);
    handle.claim_interface(endpoint.iface)?;
    // handle.set_alternate_setting(endpoint.iface, endpoint.setting)?;
    Ok(())
}

/// The application-facing connections, which outlive any one device session.
///
/// These are opened once and kept open across reconnects. Tearing a virtual MIDI
/// port down and recreating it would make receiving applications drop their
/// binding to autocrap, which is a worse failure than the disconnect itself.
struct OutboundInterface {
    osc: Option<(UdpSocket, std::net::SocketAddrV4)>,
    midi: Option<(String, midir::MidiOutputConnection)>,
}

impl OutboundInterface {
    fn open(config: &Config) -> Result<OutboundInterface> {
        let osc = if let Interface::Osc(OscInterface { host_addr, out_addr, .. }) = config.interface {
            let sock = UdpSocket::bind(host_addr)?;
            Some((sock, out_addr))
        } else {
            None
        };

        let midi = if let Interface::Midi(ref interface) = config.interface {
            let client_name = &interface.client_name;
            let midi_out = MidiOutput::new(client_name)?;
            match interface.out_port {
                MidiPort::Index(index) =>
                    Some(midi_out.ports().remove(index))
                    .map(|p| (midi_out.port_name(&p).unwrap(), midi_out.connect(&p, client_name).unwrap())),
                MidiPort::Name(ref name) =>
                    midi_out.ports().into_iter().find(|p| &midi_out.port_name(&p).unwrap() == name)
                    .map(|p| (midi_out.port_name(&p).unwrap(), midi_out.connect(&p, client_name).unwrap())),
                #[cfg(unix)]
                MidiPort::Virtual(ref _name) =>
                    Some((client_name.to_string(), midi_out.create_virtual(client_name).unwrap())),
                #[cfg(not(unix))]
                MidiPort::Virtual(ref _name) => {
                    unimplemented!("virtual midi ports are currently unsupported on non-unix systems")
                }
            }
        } else {
            None
        };

        Ok(OutboundInterface { osc, midi })
    }
}

fn run_reader<T: UsbContext>(
    interpreter: &Arc<RwLock<Interpreter>>,
    outbound: &mut OutboundInterface,
    handle: &DeviceHandle<T>,
    endpoint: &Endpoint,
    ctrl_tx: mpsc::Sender<Vec<u8>>,
    status: &StatusReporter
) -> Result<()> {
    let osc = outbound.osc.as_ref();
    let mut reported_bank_index = interpreter.read().unwrap().active_bank_index();
    let midi = &mut outbound.midi;

    let mut all_bytes = [0u8; 8];

    // the read below wakes at least once per timeout, which bounds how long a
    // shutdown takes without needing a timer of its own
    while !shutdown_requested() {
        let num_bytes = match handle.read_interrupt(endpoint.address, &mut all_bytes, DEFAULT_TIMEOUT) {
            Ok(num_bytes) => num_bytes,
            // an idle device simply has nothing to report
            Err(rusb::Error::Timeout) | Err(rusb::Error::Interrupted) => continue,
            // anything else means this device session is over. returning lets the
            // caller wait for the device to come back rather than spinning on a
            // handle that will never read again.
            Err(err) => return Err(Box::new(err))
        };

        trace!("read({:?}): {:02x?}", num_bytes, &all_bytes[..num_bytes]);
        let mut i = 0;
        while i+1 < num_bytes {
            if all_bytes[i] == 0xb0 {
                i += 1;
                continue
            }

            let bytes = &all_bytes[i..i+2];
            i += 2;

            trace!("bytes: {:02x?}", bytes);

            let num = bytes[0];
            let val = bytes[1];

            // bound to a local so the write guard drops here rather than
            // being held across the sends below
            let Some(responses) = interpreter.write().unwrap().handle_ctrl(num, val) else {
                warn!("unhandled data: {:02x?}", bytes);
                continue;
            };

            for response in responses {
                if let Some((sock, out_addr)) = osc {
                    if let Some(OscResponse { addr, args }) = response.osc {
                        let msg = OscPacket::Message(OscMessage {
                            addr: addr,
                            args: args,
                        });
                        debug!("send osc: {:?}", msg);
                        let msg_buf = encoder::encode(&msg)?;

                        sock.send_to(&msg_buf, out_addr)?;
                    }
                }

                if let Some((_, out_conn)) = midi.as_mut() {
                    if let Some(MidiResponse { data }) = response.midi {
                        debug!("send midi: {:02x?}", data);
                        out_conn.send(&data)?;
                    }
                }

                if let Some(CtrlResponse { data }) = response.ctrl {
                    ctrl_tx.send(data)?;
                }
            }

            let active_bank_index = interpreter.read().unwrap().active_bank_index();
            if active_bank_index != reported_bank_index {
                reported_bank_index = active_bank_index;
                status.report("bank", active_bank_index);
            }
        }
    }

    Ok(())
}

fn run_writer<T: UsbContext>(
    handle: &DeviceHandle<T>,
    endpoint: &Endpoint,
    ctrl_rx: &Mutex<mpsc::Receiver<Vec<u8>>>,
    session_is_live: &AtomicBool
) -> Result<()> {
    // the queue is polled rather than blocked on, so that this thread notices the
    // reader ending the session instead of waiting forever for a write that the
    // disconnected device will never be sent
    while session_is_live.load(Ordering::Relaxed) && !shutdown_requested() {
        let queued = ctrl_rx.lock().unwrap().recv_timeout(WRITER_POLL_INTERVAL);
        let data = match queued {
            Ok(data) => data,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break
        };

        debug!("send ctrl: {:02x?}", data);
        handle.write_interrupt(endpoint.address, &data, DEFAULT_TIMEOUT)?;
    }

    Ok(())
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
    // bounded so this loop can notice a shutdown rather than blocking forever
    sock.set_read_timeout(Some(RECEIVER_POLL_INTERVAL))?;
    info!("listening to {}", in_addr);

    let mut buf = [0u8; rosc::decoder::MTU];
    loop {
        match sock.recv_from(&mut buf) {
            Ok((size, addr)) => {
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size])?;
                match packet {
                    OscPacket::Message(msg) => {
                        debug!("recv osc: {} {:?}", msg.addr, msg.args);
                        let Some(response) = interpreter.write().unwrap().handle_osc(&msg) else {
                            warn!("unhandled osc message: with size {} from {}: {} {:?}", size, addr, msg.addr, msg.args);
                            continue;
                        };

                        trace!("osc in response: {:?}", response);

                        let Some(CtrlResponse { data }) = response.ctrl else {
                            continue;
                        };

                        ctrl_tx.send(data)?
                    }
                    OscPacket::Bundle(bundle) => {
                        debug!("recv osc bundle: {:?}", bundle);
                        warn!("unhandled osc bundle: {:?}", bundle);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                   || e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                error!("error receiving from socket: {}", e);
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
    let midi = match in_port {
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
        #[cfg(unix)]
        MidiPort::Virtual(ref _name) =>
            Some((client_name.to_string(), midi_in.create_virtual(
                client_name,
                move |_time, msg, tx| {
                    tx.send(msg.to_vec()).unwrap();
                },
                tx
            ).unwrap())),
        #[cfg(not(unix))]
        MidiPort::Virtual(ref _name) => {
            unimplemented!("virtual midi ports are currently unsupported on non-unix systems")
        }
    };

    if let None = midi {
        warn!("no midi in port???");
    }

    while !shutdown_requested() {
        let msg = match rx.recv_timeout(RECEIVER_POLL_INTERVAL) {
            Ok(msg) => msg,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break
        };

        let Some(response) = interpreter.write().unwrap().handle_midi(&msg) else {
            warn!("unhandled midi message: {:02x?}", msg);
            continue;
        };

        let Some(CtrlResponse { data }) = response.ctrl else {
            continue;
        };

        ctrl_tx.send(data)?
    }

    Ok(())
}
