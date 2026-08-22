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

download a prebuilt binary from [Releases](https://github.com/ahihi/autocrap/releases), or [build](#building) it yourself!

## usage

> [!NOTE]
> there are some OS-specific considerations, please see the following sections:
>
> - [Linux](#linux)
> - [Windows](#windows)

autocrap requires a configuration JSON file to run. several ready-made [configurations](config) are provided, so you do not have to write one to get started:

| file | what you get |
| --- | --- |
| [`nocturn-midi.json`](config/nocturn-midi.json) | the Nocturn as a typical MIDI controller. every control sends one fixed CC. |
| [`nocturn-midi-banks.json`](config/nocturn-midi-banks.json) | the same, plus [bank switching](#bank-switching): two buttons page the knobs, buttons and speed dial through 4 banks, turning 23 controls into 92. |
| [`nocturn-osc.json`](config/nocturn-osc.json) | every control sends an OSC message, addressed by control name. |
| [`nocturn-osc-raw.json`](config/nocturn-osc-raw.json) | as above, but sending raw device values with no state tracking. |

to use the Nocturn as a typical MIDI controller, run:

```shell
autocrap -c config/nocturn-midi.json
```

MIDI compatible applications on your computer should now see virtual input/output ports for autocrap!

point `-c` at whichever file you want. if you launch autocrap from a script or a desktop shortcut that already names a config, it is usually easier to copy the one you want over the one that script names, for example:

```shell
cp config/nocturn-midi-banks.json config/nocturn-midi.json
```

any of these is a reasonable starting point to copy and edit. see [configuration](#configuration) for what every property means.

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

## bank switching

the Nocturn gives you 8 knobs, 16 buttons, a crossfader and a speed dial. that is 27 controls, and almost any real project wants more. bank switching lets two of the buttons page the rest of the device through up to 4 alternative layouts, so the same physical knob drives a different parameter depending on which bank you are on.

with the provided [`nocturn-midi-banks.json`](config/nocturn-midi-banks.json), 23 controls become 92 addressable parameters, plus 2 that stay pinned across every bank. to try it:

```shell
autocrap -c config/nocturn-midi-banks.json
```

press the two bank buttons (the third and fourth in the bottom row) and watch the knob rings and button lights change to show that bank's values.

### what makes it usable

paging controls is easy. paging them *without* wrecking your session is the part that takes care, so autocrap does the following:

- **every bank remembers its own value.** knob 1 on bank 0 and knob 1 on bank 1 are unrelated parameters, each holding whatever you last set it to.
- **the hardware shows you the bank you are on.** on every switch, each banked control's LED ring or button light is rewritten to that bank's value. you can see at a glance where all your parameters sit, without touching anything.
- **switching is completely silent.** no MIDI or OSC is sent when the bank changes. your receiving application already holds the correct value for every target, and switching banks does not change any of them, it only changes which one the knob under your hand is wired to. nothing jumps, nothing is retriggered, and you can page around mid-take.
- **you can pin controls across banks.** a control that opts out of banking is untouched by the bank counter. this is how you keep something like a master volume or a transport button in the same place on every bank.
- **held buttons cannot get stuck.** if you are holding a momentary control when the bank changes, autocrap sends an off value for the bank you are leaving and ignores the release that follows. this matters more than it sounds: resting a finger on the Nocturn's touch-sensitive knobs while reaching for a bank button is easy to do by accident, and without this the parameter would be left stuck on with no way to clear it.
- **the current bank is always visible**, displayed as a two-bit binary number on the two bank buttons' own LEDs.

the bank always starts at 0 when autocrap launches, and is not saved between runs.

### what is in the provided config

[`nocturn-midi-banks.json`](config/nocturn-midi-banks.json) banks everything that has a light to show it:

| control | banked? | bank 0 | bank 1 | bank 2 | bank 3 |
| --- | --- | --- | --- | --- | --- |
| knob 0-7 | yes | CC 64-71 | CC 16-23 | CC 24-31 | CC 32-39 |
| button 0-7 (toggle) | yes | CC 112-119 | CC 82-89 | CC 90-97 | CC 98-105 |
| button 8-9 | yes | CC 120-121 | CC 106-107 | CC 108-109 | CC 110-111 |
| button 12-15 | yes | CC 124-127 | CC 40-43 | CC 44-47 | CC 48-51 |
| speed dial | yes | CC 74 | CC 52 | CC 53 | CC 54 |
| crossfader | no | CC 72 on every bank | | | |
| speed dial click | no | CC 81 on every bank | | | |
| button 10-11 | — | the bank selectors. send nothing. | | | |

bank 0 keeps every control on the same CC number it uses in [`nocturn-midi.json`](config/nocturn-midi.json), so anything you have already mapped in your DAW keeps working, and the other three banks are new ground.

the crossfader is not banked because it cannot be: it reports the live physical position of a slider, so there is no stored value to restore when you switch to it. the speed dial click is not banked because it has no LED, and banking a control you cannot see the state of just makes it unpredictable. both of these are deliberate, and the first is enforced when the config loads.

### making your own

banking is opt-in per control, and nothing about it is hardcoded to particular buttons. see [bank selector mapping](#bank-selector-mapping) for choosing the two bank buttons and the bank count, and [`bank_midi_targets`](#bank_midi_targets) / [`bank_osc_addresses`](#bank_osc_addresses) for opting individual controls in. it works the same way over OSC as over MIDI.


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

a list of single mappings, range mappings and/or a bank selector mapping, specifying how autocrap should translate data between the MIDI/OSC interface and the device's native format.

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

if a `Momentary` or `Raw` control is [banked](#bank-selector-mapping) and is physically held when the bank changes, autocrap sends an off value for the bank being left and then ignores the release that follows. without this, the parameter would be left stuck on in the receiving application with no way to clear it. this matters most for the encoder and crossfader touch sensors, since resting a finger on a knob while pressing a bank button is easy to do by accident.

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

##### `bank_midi_targets`

```
      "bank_midi_targets": [
        {"channel": 0, "kind": "Cc", "num": 20},
        {"channel": 0, "kind": "Cc", "num": 21},
        {"channel": 0, "kind": "Cc", "num": 22},
        {"channel": 0, "kind": "Cc", "num": 23}
      ],
```

optional. one MIDI message per bank, in bank order, each in the same form as [`midi`](#midi-1). the control above sends CC 20 while bank 0 is active, CC 21 on bank 1, and so on, remembering a separate value for each.

**the presence of this property is what opts the control into [banking](#bank-switching).** there is no separate switch to turn on. a control with neither this nor `bank_osc_addresses` is completely unaffected by the bank counter, which is how you pin a control to the same target on every bank.

- the list must have exactly `bank_count` entries.
- this **replaces** `midi`. setting both on the same mapping is an error, so that there is no precedence rule to remember.
- `EightBit` controls cannot use it. a crossfader reports a live physical position rather than a stored value, so there would be nothing to restore when you switched to it.
- you only need the list matching the interface you actually run. a MIDI config never needs `bank_osc_addresses`.

##### `bank_osc_addresses`

```
      "bank_osc_addresses": ["/reverb", "/pianoVolume", "/padFilter", "/delaySend"],
```

optional. one OSC address per bank, in bank order. the OSC equivalent of `bank_midi_targets`, and its presence opts the control into banking in exactly the same way.

- the list must have exactly `bank_count` entries.
- this **replaces** the address autocrap would otherwise derive from [`name`](#name), so the control answers to `/reverb` on bank 0 rather than to `/knob0`.
- `EightBit` controls cannot use it, for the same reason as above.
- it can be used on its own, or alongside `bank_midi_targets` if you want one config to serve both interfaces.

##### banking and inbound messages

when your application sends a value *to* autocrap, the message is matched against every bank, not just the active one.

- a message addressed to the **active** bank updates the control and moves its light, exactly as an unbanked control would.
- a message addressed to **any other** bank is stored against that bank and nothing is written to the device, because a bank you are not looking at has nothing to show. the value is waiting on the hardware the next time you switch to it.

this means your application can set up all four banks at load time and have the device come up correct as you page through them.

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
- in each entry of `bank_midi_targets`, the index of the element is added to `num`. a range of 8 knobs with bank targets `[20, 40, 60, 80]` expands to `knob0` sending on `[20, 40, 60, 80]`, `knob1` on `[21, 41, 61, 81]`, and so on through `knob7` on `[27, 47, 67, 87]`.
- in each entry of `bank_osc_addresses`, the string `{i}` is replaced with the index of the element, exactly as in `name`. offsetting an address by a number would be meaningless, so addresses like `"/bank0/knob{i}"` are used instead.

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

#### bank selector mapping

```
    {"BankSelector": {
      "bank_count": 4,
      "previous_bank_button": {"input_control_number": 122, "led_control_number": 122},
      "next_bank_button": {"input_control_number": 123, "led_control_number": 123}
    }},
```

claims two of the device's buttons as bank controls, and switches on [bank switching](#bank-switching) for the whole config. one button steps back through the banks and the other steps forward, both wrapping: forward from the last bank returns to bank 0, and back from bank 0 goes to the last bank.

a config may contain at most one bank selector mapping. banks change when a bank button is pressed; the release does nothing.

> [!IMPORTANT]
> the two buttons you name here **stop working as ordinary controls**. they send no MIDI and no OSC, and they must not also appear as a single or range mapping. if they do, autocrap refuses to start and tells you which mapping is in the way, rather than leaving you with two buttons that half work.
>
> this includes collisions produced by a range mapping expanding over the number. the stock Nocturn config maps `button0` through `button15` to control numbers 112 to 127, so choosing any bank button in that span means removing or shortening the mapping that already covers it.

##### `bank_count`

how many banks to cycle through, from 2 to 4.

the ceiling is 4 because the current bank is displayed as a two-bit binary number on the two bank buttons' own LEDs, and two bits only count to four. if you ask for more, autocrap refuses to start rather than leaving you unable to tell which bank you are on.

##### `previous_bank_button`, `next_bank_button`

which physical buttons cycle the banks, and which lights to drive. each takes:

- `input_control_number`: the control number the device sends when that button is pressed. this is the same number you would put in `ctrl_in_num` if it were an ordinary control.
- `led_control_number`: the control number to write to in order to light that button. on the Nocturn's grid buttons this is the same as the input number, but it is configurable because it is not the same on every control.

the current bank is shown across the two lights as a binary number, with the low bit on the next button and the high bit on the previous button:

| bank | previous button LED | next button LED | reads as |
|---|---|---|---|
| 0 | off | off | `00` |
| 1 | off | on | `01` |
| 2 | on | off | `10` |
| 3 | on | on | `11` |

both lights being off on bank 0 means the display is correct from the moment autocrap starts, with no extra initialisation.


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
