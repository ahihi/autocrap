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

a prebuilt binary is available from [Releases](https://github.com/ahihi/autocrap/releases). note that it is the command line tool only — the [macOS menu bar app](#macos-menu-bar-app) is built from source.

to build everything yourself, see [getting started](#getting-started).

## getting started

you will need [Rust](https://www.rust-lang.org/tools/install). on macOS you will also need Xcode's command line tools, which you may already have:

```shell
xcode-select --install
```

then clone this repository and pick one of the two ways to run it.

### macOS, from the Dock

```shell
./packaging/macos/build.sh
```

that builds autocrap and wraps it in `Autocrap.app` in the repository root. drag that app to the Dock once, and click it whenever you want autocrap running.

there is no window. a glyph appears in the menu bar — that is how you know it is running — and its menu shows whether the Nocturn is connected, which bank you are on, and lets you switch config. plug the Nocturn in and it will play a light sweep to confirm it connected.

> [!IMPORTANT]
> leave the app in the repository folder. it finds the autocrap binary and your configs relative to its own location, so moving it elsewhere stops it working.

see [macOS menu bar app](#macos-menu-bar-app) for the rest of it.

### any platform, from a terminal

```shell
cargo build --release
./target/release/autocrap -c config/nocturn-midi.json
```

MIDI compatible applications should now see virtual input and output ports for autocrap. stop it with Ctrl-C rather than killing it, so it releases the device cleanly.

there are OS-specific steps on [Linux](#linux) and [Windows](#windows), and the [usage](#usage) section below covers the other configs and command line options.

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
  -c, --config <FILE>       Set a config file
  -l, --log <LOG>           Set logging level
      --status              Print machine-readable status events to stdout, for a front end to read
      --calibration <FILE>  Corrections for this particular device's faults, applied whichever config is loaded
  -h, --help                Print help
  -V, --version             Print version
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

### `name`

```
  "name": "4 banks",
```

optional. a human-readable label for the config as a whole, shown by front ends that let you pick between several, such as the [macOS menu bar app](#macos-menu-bar-app). when absent, the filename is used instead.

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
- this **replaces** the address autocrap would otherwise derive from [`name`](#name-1), so the control answers to `/reverb` on bank 0 rather than to `/knob0`.
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


## connecting and reconnecting

autocrap does not need the device to be plugged in when it starts, and does not
stop when it is unplugged.

- **starting without the device** waits for it rather than failing. plug the
  Nocturn in whenever you like and it connects on its own.
- **connecting plays a one second sweep** across every light on the device,
  lighting each in turn and then unlighting them, before painting your actual
  values. this is how you can tell at a glance that the connection really
  worked, rather than finding out only when a control fails to do anything. it
  doubles as a check that every lamp and LED ring still works.
- **unplugging mid-session** is not fatal. the MIDI and OSC connections stay
  open throughout, so applications connected to autocrap keep their connection
  and do not have to be re-pointed at it.
- **plugging back in** reconnects automatically, and the device is repainted
  from the values autocrap still holds — so your bank and every control's
  position survive a replug, even though the hardware itself lost them.
- **stopping it with Ctrl-C** clears the lights and releases the device. this
  matters: a process killed outright leaves the USB interface claimed, and the
  device then needs to be physically unplugged before anything can talk to it
  again.

the light sweep is driven from your config, so it covers exactly the lights your
config declares — every mapping's `ctrl_out_num`, plus the two bank selector
lights. nothing about it is specific to the Nocturn.


## device calibration

older hardware wears out. the Nocturn's rotary encoders are mechanical contacts, and as they degrade they start misreading their own position and reporting ticks in the *wrong* direction, so a knob jumps backwards while you are turning it forwards. how many detents an encoder reports per rotation also varies, which decides how far you have to turn to cross a control's full range.

both of those are properties of **the device in front of you**, not of what you have mapped it to. so they live in their own file rather than in a config, and are applied whichever config is loaded:

```shell
autocrap -c config/nocturn-midi.json --calibration calibration.json
```

without this separation the same corrections would have to be copied into every config, and switching config would silently change how your knobs feel.

### the file

```json
{
  "controls": {
    "64": {
      "step": 3,
      "reversal_filter": {
        "ticks_to_confirm_reversal": 6,
        "deliberate_reversal_pause_ms": 100
      }
    }
  }
}
```

`controls` is keyed by `ctrl_in_num`, the control number the device reports, because that identifies the physical control whatever a given config happens to call it. every property is optional, and a control with no entry is left exactly as its config describes it.

### `step`

multiplier applied to each raw tick of a `Relative` control before accumulating. defaults to 1, the raw device behaviour.

the Nocturn's encoders report one tick per detent, so a knob needs 127 detents to cross its range — far more turning than the detent spacing makes reasonable. a step of 3 crosses it in about 43.

### `reversal_filter`

rejects the false reversals a worn encoder reports.

an individual bad tick is indistinguishable from the start of a real direction change at the moment it arrives. rather than guess, opposing ticks are held back until the new direction proves itself:

- while the turn continues in its established direction, ticks pass through untouched.
- an opposing tick is deferred rather than applied. if the original direction resumes, the deferred ticks are discarded as noise and never reach the output.
- if `ticks_to_confirm_reversal` opposing ticks arrive in a row, the reversal is believed and every deferred tick is applied at once. **nothing is lost** — a genuine reversal costs a little latency, not travel.
- a tick arriving more than `deliberate_reversal_pause_ms` after the previous one is always applied immediately, since contact noise arrives in an unbroken burst and never follows a pause. stopping and turning back therefore costs nothing at all.

both halves are needed. a confirmation count alone cannot reject a long burst without a count so high it ruins the feel, and a time threshold alone throws away genuine reversals made mid-turn, which follow no pause.

`ticks_to_confirm_reversal` should be a little longer than the longest burst your encoder produces; 6 is a reasonable starting point. **higher is not better** — beyond what your hardware needs, the added latency starts registering as jitter in its own right.

### finding the right values

run with `-l trace` and turn a knob steadily in one direction without letting go. every tick should be in that direction, so any tick against it is a fault. the length of those wrong-direction runs is what `ticks_to_confirm_reversal` has to exceed.

this is worth doing per control: on a worn Nocturn one knob measured a 78% fault rate while another on the same device measured zero.

### a note on repair

a filter masks the fault; it does not fix it. contact cleaner worked into the encoder body often restores a knob completely and is worth trying first. calibration is for what remains after that.

## macOS menu bar app

a small front end that runs autocrap for you and shows what it is doing.

```shell
./packaging/macos/build.sh
```

that produces `Autocrap.app` in the repository root. drag it to the Dock once, and click it whenever you want autocrap running.

it has no window. a glyph appears in the menu bar and its menu shows whether the device is connected, which bank you are on, and lets you switch config, open the log or quit.

the glyph's **shape** shows the connection and its **colour** shows the bank, so you keep the bank reading even while the device is away:

| | |
| --- | --- |
| solid dial | device connected |
| hollow dial | waiting for the device |
| red warning | autocrap is not running, or failed to start |

| bank | colour |
| --- | --- |
| 0, or a config without banking | the menu bar's own colour |
| 1 | orange |
| 2 | green |
| 3 | blue |

bank 0 uses the menu bar's own colour rather than a fixed one, so it stays visible whether your menu bar is light or dark. red is reserved for the failure glyph, so no bank uses it.

- **the app owns no device logic.** it starts the ordinary `autocrap` binary, reads its output and draws a menu. autocrap remains completely usable on its own from a terminal.
- **it must stay in the repository folder**, because it finds the binary, the configs and the calibration relative to its own location. moving it elsewhere is reported rather than failing silently.
- **the calibration is applied whichever config you pick**, so switching never changes how the hardware feels.
- logs from both the app and autocrap are appended to `~/Library/Logs/autocrap.log`.

rebuild it after changing any Rust code, and relaunch it; replacing the bundle does not affect an instance already running.

### `--status`

the app reads the device's state over the same flag anyone can use:

```shell
autocrap -c config/nocturn-midi.json --status
```

which prints events to stdout, while logs continue to go to stderr:

```
@autocrap config=Default
@autocrap banks=4
@autocrap device=connected
@autocrap bank=2
```

each line is the sentinel `@autocrap` followed by one `key=value` pair. readers should ignore lines without the sentinel and keys they do not recognise, so that new events can be added without breaking them.


## building

you will need:

- rustc (tested with 1.79.0)
- Cargo

```shell
cd autocrap
cargo build --release
```

this creates a stand-alone executable under `target/release` called `autocrap`, which can be placed wherever you like.

to build the [macOS menu bar app](#macos-menu-bar-app) as well, run `./packaging/macos/build.sh` instead — it builds autocrap first, then wraps it. unlike the bare executable, the app has to stay in the repository folder, since it locates the binary and your configs relative to itself.

## disclaimer

all trademarks are property of their respective owners. all company and product names used in this repository are for identification purposes only. use of these names, trademarks and brands does not imply endorsement.
