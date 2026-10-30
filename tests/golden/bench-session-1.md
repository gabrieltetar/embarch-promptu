## Goal
Two reworks on the stepper driver board need validating: a pull-down on the enable line and a 100 nF cap on the fault pin

It unblocks: The reworks go into the next board revision

## Context
Bench right now (as of this prompt):
- Board: Stepper driver board rev C, both reworks done on unit 2
- A debug probe (SWD/JTAG) is attached. It can hold the MCU off or add current, so say when it should come off.
- It is powered from a bench supply.
- A USB cable connects it to this machine.
- Instruments: a scope or logic analyzer is on the signals.

## Done when
Pass means: The fault pin stays high through 50 enable toggles, and the motor holds still with enable low. Show the observation itself (log lines, measurements or a capture), not only a clean build.

## Constraints
- Build, flash and run through EmbArch's tools.
- If firmware must change for this, use the lightest form (a devicetree overlay or a config) before a new binding or board.

## Who does it
Wait for my green light before each step that touches the bench or the device.

— promptu bench-session@1 asked=pass:other,instruments:o1,gate:rec,flash:rec,heavy:rec,timebox:rec,unblocks:other,board:other,attached:o1+o2+o4
