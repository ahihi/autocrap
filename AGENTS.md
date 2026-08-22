# AGENTS.md

Context an AI agent needs before changing this repository. It records **why**
things are the way they are — the reasoning that the code cannot state for
itself. The code already says what it does; do not duplicate that here.

## What this is

A userspace driver for the Novation Nocturn, a USB control surface from 2009
with no working modern driver. It translates the device's proprietary USB
protocol to and from MIDI or OSC, so it can be used with anything.

This is a fork. `upstream` is `ahihi/autocrap` and `origin` is the fork.
Changes are written to be upstreamable, which shapes several decisions below.

## Code style

Not advisory. Code that ignores this should be rejected in review.

### Names carry the meaning, comments justify

A name answers "what is this?". A comment answers "why does this exist?". When
a name fully answers "what", a comment restating it is noise. If you feel a
comment coming on to explain what a variable holds, **rename the variable**.

Short names force every reader to rebuild context at every reference. A long,
precise name rebuilds it instantly, for free. This applies to locals as much as
fields — locals are read more often, being the active working state.

- **No shape names.** Not `list`, `dict`, `result`, `data`, `values`, `items`,
  `slot`. Every collection holds *something*: name that something.
  `bank_assignments`, not `slots`.
- **Name derived sets by the predicate that produced them.**
  `controls_that_restore_on_bank_change`, not `filtered`.
- **Function names read as a sentence.** `build_bank_indicator_writes`, not
  `leds`. `switch_to_bank`, not `set_bank`.
- **No diary names.** `old_bank` describes when, not what. Use
  `outgoing_bank_index`.
- **Abbreviate only to match established vocabulary.** This codebase spells it
  `midi` and `osc`. That does not license inventing new abbreviations.

### Comments

- **Every comment explains why, never what.** Delete any comment that restates
  the code.
- **No section-header comments** (`// helpers`, `// main logic`).
- **No diary comments.** A comment must be as true in ten years as today. Never
  describe what the code used to be. `// now uses a Vec` is banned.
- **Never guess.** If you cannot determine the reason with certainty, write no
  comment. A wrong comment is worse than none.
- **`///` rustdoc on every public item**, one tight sentence on purpose and why
  it exists. Do not restate the signature.
- Keep the genuine ones: hardware quirks, non-obvious algorithms, and the
  reasoning behind an edge case all deserve a "why".

### The one exception: serde field names are frozen

Fields on `Mapping`, `Config` and the interface structs are the public JSON
config format. Existing abbreviated names (`ctrl_in_num`, `ctrl_out_num`,
`midi`, `num`) **must stay exactly as they are**. Renaming them silently breaks
every config file every user has, which is far worse than an inconsistent name.

This covers only serde-facing names. Everything internal follows the rules
above. New serde names have no existing users, so they get full descriptive
names — the resulting mixed convention inside `Mapping` is deliberate.

## Architecture, and the reasoning behind it

### One type per file

`config.rs` and `interpreter.rs` are module roots holding only `mod`
declarations and `pub use` re-exports. Every type lives in its own file beside
them. This was a deliberate, behaviour-free commit at the base of the branch so
a maintainer can take it or leave it independently of any feature.

### Controls are independent; the interpreter owns what is shared

Each control is a `Box<dyn CtrlLogic>` that knows nothing about the others.
Anything crossing control boundaries — the bank counter — lives on
`Interpreter`, because a bank change must reach *every* control at once, which
the one-object-per-control model cannot express.

### An unbanked control is a vector of length one

`BankAssignment` holds a control's MIDI target, OSC address and remembered
value together, so the three cannot drift apart. Unbanked controls get a
single assignment rather than an `Option`, so every handler indexes the active
bank unconditionally. This removes a whole class of branch-forgotten bug;
prefer it to reintroducing a banked/unbanked split.

### Keep decision logic free of the clock

`ReversalFilterState` takes elapsed milliseconds as a parameter rather than
reading a clock, so its rules are pure and exhaustively testable.
`Instant::now()` appears in exactly one place in `RelativeLogic`. Preserve this
whenever adding time-dependent behaviour: a testable rule plus one impure
caller beats a clock threaded through everything.

### Config describes intent; calibration describes the hardware

`config/*.json` says what controls are mapped to. `calibration.json` says how
this particular worn device physically behaves (`step`, `reversal_filter`),
keyed by `ctrl_in_num` because that identifies the physical control whatever a
config calls it. Calibration wins where both are set.

The separation exists because switching config must not change how the hardware
feels. Do not move calibration properties back into configs.

### Connections outlive device sessions

The interpreter, the MIDI/OSC connections and the device write queue are
created once, outside the reconnect loop. Only the USB half is rebuilt when the
device goes away and returns.

**Do not move MIDI setup back inside the session.** Recreating a virtual MIDI
port makes receiving applications drop their binding to autocrap, which is a
worse failure than the disconnect being handled.

### Exit cleanly or the device wedges

`SIGTERM`/`SIGINT` set a flag that the loops check, so the process unwinds and
`rusb` releases the claimed USB interface. A process killed mid-syscall leaves
the interface claimed and **the device then needs a physical replug** before
anything can talk to it. This was diagnosed by experiment, not theory. Never
introduce a path that exits without unwinding.

Note `handle.reset()` is skipped on macOS: a real bus reset there makes the
device re-enumerate and invalidates the handle just opened.

### The connect sweep is written outside the queue, and paced

Connecting plays a one second sweep across every light. It exists because
plugging the device in previously produced no visible response at all until a
control was touched, so a working connection was indistinguishable from a
config that had quietly failed to claim the device.

Two details are load-bearing:

- **Frames go straight to the endpoint, not through the device write queue.**
  That queue is drained by a thread which has not started yet at that point, so
  queued frames would arrive all at once and animate nothing.
- **Each frame is paced against the animation's start time**, not by sleeping a
  fixed interval, so the sweep lasts a second regardless of how slow the writes
  prove to be on a given machine.

The light set comes from the config — every mapping's `ctrl_out_num` plus the
bank selector's own two lights, which are not mappings but are still lights.
Nothing is hardcoded to the Nocturn.

### A reconnect repaints the device from interpreter state

Unplugging clears the hardware but not the values held in memory, so connecting
writes every indicator back from `build_device_refresh_writes`. Device writes
queued while the device was away are discarded first, or they would paint stale
values over the refresh.

This is why bank index and per-bank values survive a replug: they were never
lost, only the hardware's display of them was.

### Status on stdout, logs on stderr

`--status` prints `@autocrap key=value` lines to stdout while `colog` logs to
stderr, so a front end can consume each stream for its own purpose. Readers
must ignore unknown keys and unsentinelled lines, so events can be added
without breaking existing readers. Treat it as a contract, not a habit.

## Behavioural rules that are easy to break by accident

These were decided deliberately. Changing them is a product decision, not a
refactor.

- **A bank switch sends no MIDI and no OSC.** Only LEDs change. The receiving
  application already holds the right value for every target; switching only
  changes which one the physical control is wired to.
- **The one exception is a held `Momentary`/`Raw` control**, which gets a real
  "off" against the *outgoing* bank and has its release consumed. Without it,
  brushing a touch sensor while changing bank leaves a parameter stuck on with
  no way to clear it.
- **Switching banks must never turn a parameter on.** The incoming bank's
  indicator is restored; no MIDI or OSC is emitted for it.
- **The bank-switch indicator write is unconditional.** Do not reuse the
  `changed` guards from the normal tick path: the hardware is displaying
  another bank's value, so it must be rewritten even when the values agree.
- **`handle_ctrl` returning `None` means unmatched** and logs `unhandled data`.
  `Some(vec![])` means handled with nothing to emit. Preserve the distinction —
  a bank button's release depends on it.
- **The crossfader cannot bank.** It reports a live slider position, so there is
  no stored value to restore. Validation rejects it.
- **Validation fails loudly.** A silently ignored banking config produces a
  device that looks broken for reasons the user cannot see.

## The macOS front end

`packaging/macos/` builds `Autocrap.app`, a menu bar supervisor.

- **It owns no device logic.** It spawns the ordinary binary, reads its two
  streams and draws a menu. autocrap must stay fully usable without it.
- **It never auto-restarts a child that died.** A crash loop would be invisible
  and would hammer the device. Restarting is a menu action the user takes
  knowingly.
- **A superseded process must not report its own exit**, or it wipes the state
  of the successor that replaced it — this bug orphaned a child on every config
  switch until it was fixed. Check identity in `terminationHandler`.
- **It stops children with `SIGTERM`, then waits**, because that is what lets
  the USB interface be released. Do not shortcut to `SIGKILL`.
- **The built bundle is gitignored.** Source and a build script are committed;
  a multi-megabyte binary changing on every rebuild is not.
- It resolves the binary, configs and calibration relative to its own bundle,
  which is why it must live in the repository root.

## Testing

- Tests live in `#[cfg(test)] mod tests` blocks beside the code.
- **No new dependencies.** `serde_json` builds `Config` fixtures from inline
  JSON, which is enough.
- **Test names state the scenario and the expected outcome**, as a sentence:
  `held_momentary_control_emits_off_for_outgoing_bank`.
- Anything touching USB cannot be unit tested. Behaviour that matters is pushed
  into pure functions that can be.

## Working with the hardware

- USB behaviour cannot be verified without the device attached; say so rather
  than implying something was tested when it was not.
- **Measure before designing.** `-l trace` prints every raw control event. Two
  conclusions about encoder wear were reached from a gentle capture and both
  were wrong; a controlled capture overturned them. Prefer a second measurement
  to a confident guess.
- Killing autocrap abruptly wedges the device. Stop it with `SIGTERM`.

## Repository conventions

- **Never commit or push without being asked.** Work is reviewed first.
- Commit messages explain why the change exists and what it trades off, not
  which files moved.
- `config/nocturn-midi.json` is the upstream original and should stay that way.
- `calibration.json` and `docs/` are personal to this fork.
- The branch carries separable concerns as separate commits so upstream can
  take them independently. Preserve that when adding to it.
