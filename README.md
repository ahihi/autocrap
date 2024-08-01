# autocrap

bring your Novation Nocturn to life again! (and maybe other Automap-based devices in the future)

autocrap is a cross-platform userspace "driver" which provides input and output to the device via MIDI or OSC, allowing it to be used with any application that speaks these protocols.

autocrap is highly configurable to support different use cases, ranging from typical DAW controls to fully custom programming.

## compatibility

- macOS 10.12+ (tested with 10.14)
- Linux (tested with Debian 12)
- Windows (tested with Windows 10)
- possibly other platforms supported by Rust and libusb

## installation

binaries coming soon! in the meantime, see [building](#building).

## usage

> [!NOTE]
> there are some OS-specific considerations, please see the following sections:
>
> - [Linux](#linux)
> - [Windows](#windows)

autocrap requires a configuration JSON file to run. some [example configurations](config) are provided.

for example, to use the Nocturn as a typical MIDI controller, run:

```shell
autocrap -c config/nocturn-midi.json
```

MIDI compatible applications on your computer should now see virtual input/output ports for autocrap!

to view the full list of supported command-line options, run `autocrap -h`:

```
Usage: autocrap [OPTIONS] --config <FILE>

Options:
  -c, --config <FILE>  Set a config file
  -l, --log <LOG>      Set logging level
  -h, --help           Print help
  -V, --version        Print version
```

the logging level defaults to `info`. you can also set it to `debug` or `trace` to get more debugging information.

### Linux

#### device permissions

on Linux, your user must have permission to access the USB device. you may need to create a udev rule to set the permissions. the following instructions apply to Debian, but can hopefully be easily adapted to other distros.

create the file `/etc/udev/rules.d/51-nocturn.rules` with the contents:

```
SUBSYSTEM=="usb", ATTRS{idVendor}=="1235", ATTRS{idProduct}=="000a", MODE="0666"
```

then reload udev rules by running:

```shell
sudo udevadm control --reload-rules
```

and you should be good to go!

### Windows

#### generic driver

on Windows, you must also install a generic USB driver for the device in order to allow autocrap to communicate with it. for example, you can use [Zadig](https://zadig.akeo.ie/) to install the WinUSB driver.

#### no support for virtual MIDI ports

due to limitations in the midir library, virtual MIDI ports are currently unsupported on Windows. as an alternative, you can use [loopMIDI](https://www.tobias-erichsen.de/software/loopmidi.html). in loopMIDI, create two virtual ports named `autocrap in` and `autocrap out`, then reference them using `Name` ports in your configuration:

```
  "interface": {"Midi": {
    "client_name": "autocrap",
    "out_port": {"Name": "autocrap out"},
    "in_port": {"Name": "autocrap in"}
  }}
```

see also the [section on MIDI configuration](#midi).

## configuration

the configuration is a JSON object with the following properties:

### USB device properties

there is no need to edit these, unless you are creating a configuration to support a new device.

for the Nocturn, these values should be:

```
  "vendor_id": 4661,
  "product_id": 10,
  "in_endpoint": 1,
  "out_endpoint": 2,
```

#### `vendor_id`, `product_id`

vendor and product ID of the USB device, in base 10. these IDs are often displayed in hexadecimal, so a conversion is required.

#### `in_endpoint`, `out_endpoint`

numbers of the USB endpoints on which the device sends/receives data.

### `interface`

configures autocrap to communicate over either MIDI or OSC.

#### MIDI

example configuration:

```
  "interface": {"Midi": {
    "client_name": "autocrap",
    "out_port": {"Virtual": "autocrap"},
    "in_port": {"Virtual": "autocrap"}
  }},
```

##### `client_name`

not sure where this actually goes, possibly only used on some operating systems? TODO: find out

##### `out_port`, `in_port`

MIDI ports where autocrap will send output and read input. autocrap can create its own virtual ports or use existing ports.

###### virtual port

```
    "out_port": {"Virtual": "autocrap"},
```

will create a virtual output port named `autocrap`. you can change the name to whatever you like.

###### existing port, by name

```
    "out_port": {"Name": "Scarlett 6i6 USB"},
```

will send to the output port called `Scarlett 6i6 USB`. note that port naming conventions differ by operating system.

###### existing port, by index

```
    "out_port": {"Index": 0},
```

will send to the first output port on the computer. this is probably not a good idea if you have multiple ports, as the order may change.

#### OSC

example configuration:

```
  "interface": {"Osc": {
    "host_addr": "127.0.0.1:9900",
    "out_addr": "127.0.0.1:9901",
    "in_addr": "127.0.0.1:9902"
  }},
```

##### `host_addr`

IP address and port on which to bind the UDP socket used for sending to the OSC output. this is **not** the address where autocrap will send or receive OSC messages!

TODO: this is confusing and probably should be made optional, with automatic defaults.

##### `out_addr`, `in_addr`

IP address and port where to send and receive OSC messages.

### `mappings`

a list of single mappings and/or range mappings, specifying how autocrap should translate data between the MIDI/OSC interface and the device's native format.

#### single mapping

```
    {"Single": {
      "name": "speedDial",
      "ctrl_in_num": 74,
      "ctrl_out_num": 80,
      "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
      "midi": {
        "channel": 0,
        "kind": "Cc",
        "num": 74
      }
    }},
```

specifies how a single control on the device should be mapped.

##### `name`

name of the control. when using OSC, this is turned into the control's OSC address by prepending a slash; e.g. `speedDial` becomes `/speedDial`.

##### `ctrl_in_num`, `ctrl_out_num`

control number on which the device sends/receives data for this control. these are often the same, but not always, as is the case with the Nocturn's "speed dial".

`ctrl_out_num` is only used when the device has some indicator to display the state of the control, such as LEDs.

##### `ctrl_kind`

specifies what kind of control is in question. the following kinds are supported:

###### `Relative`

```
        "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
```

a relative control sends increment/decrement values. an example is the rotary encoders on the Nocturn.

`mode` specifies how autocrap should manage the control's state. the following modes are supported:

- `Accumulate`: makes the control act like a normal knob, by accumulating increments and decrements and sending out the current value over MIDI/OSC. if a `ctrl_out_num` is given, the current value is also sent to the device for display.
- `Raw`: sends out the raw increment and decrement data.

###### `OnOff`

```
        "ctrl_kind": {"OnOff": {"mode": "Toggle"}},
```

sends on/off values. examples include the Nocturn's buttons, as well as the touch sensors on the encoders and the crossfader.

`mode` specifies how autocrap should manage the control's state. the following modes are supported:

- `Toggle`: the on/off state is toggled whenever the control is pressed. if a `ctrl_out_num` is given, the state is also sent to the device for display.
- `Momentary`: the on/off state corresponds to whether the control is pressed or released. if a `ctrl_out_num` is given, the state is also sent to the device for display.
- `Raw`: sends out the raw pressed/released state. this only differs from `Momentary` in that the state is not automatically sent to the device for display.

###### `EightBit`

```
      "ctrl_in_sequence": [72, 73],
      "ctrl_kind": "EightBit",
```

the Nocturn sends its crossfader position as an 8-bit value in a two-value sequence. this control kind exists to handle that.

since the device sends the high and low bits with different control numbers, they must be specified using `ctrl_in_sequence`.

note that when using the MIDI interface, this value is currently reduced to 7 bits to fit in a CC message. with OSC, no such reduction happens.

##### `midi`

specifies the MIDI message corresponding to the control.

- `channel`: the MIDI channel. numbering is zero-based (0-15) as opposed to the one-based numbering (1-16) used in some applications.
- `kind`: the MIDI message kind. currently only `Cc` is supported.
- `num`: the control number (0-127).

#### range mapping

```
    {"Range": {
      "count": 16,
      "mapping": {
        "name": "button{i}",
        "ctrl_in_num": 112,
        "ctrl_out_num": 112,
        "ctrl_kind": {"OnOff": {"mode": "Toggle"}},
        "midi": {
          "channel": 0,
          "kind": "Cc",
          "num": 112
        }
      }
    }},
```

this is a shorthand for defining a sequence of similar mappings. `count` specifies the length of the sequence, and `mapping` specifies the first element of the sequence as a [single mapping](#single-mapping). note that for each element,

- in the `name` property, the string `{i}` is replaced with the index of the element.
- in `ctrl_in_num`, `ctrl_out_num` and `midi`→`num`, the index of the element is added to the number.

essentially, the range mapping example above expands to:

```
    {"Single": {
        "name": "button0",
        "ctrl_in_num": 112,
        "ctrl_out_num": 112,
        "ctrl_kind": {"OnOff": {"mode": "Toggle"}},
        "midi": {
          "channel": 0,
          "kind": "Cc",
          "num": 112
        }
    }},
    {"Single": {
        "name": "button1",
        "ctrl_in_num": 113,
        "ctrl_out_num": 113,
        "ctrl_kind": {"OnOff": {"mode": "Toggle"}},
        "midi": {
          "channel": 0,
          "kind": "Cc",
          "num": 113
        }
    }},
    ⋮
    {"Single": {
        "name": "button15",
        "ctrl_in_num": 127,
        "ctrl_out_num": 127,
        "ctrl_kind": {"OnOff": {"mode": "Toggle"}},
        "midi": {
          "channel": 0,
          "kind": "Cc",
          "num": 127
        }
    }},
```

## building

you will need:

- rustc (tested with 1.79.0)
- Cargo

```shell
cd autocrap
cargo build --release
```

this creates a stand-alone executable under `target/release` called `autocrap`, which can be placed wherever you like.

## disclaimer

all trademarks are property of their respective owners. all company and product names used in this repository are for identification purposes only. use of these names, trademarks and brands does not imply endorsement.
