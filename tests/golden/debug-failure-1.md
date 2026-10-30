## Goal
After an OTA update the sensor node sometimes boots the old image. investigate

## Context
Expected versus actual: Expected: the new image runs after the update. Actual: about 1 update in 5 falls back to the old image.
It happens intermittently, not on every run.
Already ruled out: Not a signature failure: the image verifies on the host.
Bench right now (as of this prompt):
- Board: Sensor node rev B, no reworks
- A debug probe (SWD/JTAG) is attached. It can hold the MCU off or add current, so say when it should come off.
- It is powered from a bench supply.

## Done when
The diagnosis comes with its evidence (logs, measurements, file:line).
For a fix: The trigger no longer reproduces it in ten tries.

## Constraints
- Run a command that changes state (flash, erase, reset, power) only when the evidence calls for it.
- Wait for my green light before touching the bench or the device under test.

## Who does it
Before any fix, give me ranked hypotheses, each with the evidence for and against it, then stop and wait for my pick.

— promptu debug-failure@1 asked=symptom:other,first:rec,repro:o2,ruled_out:other,fixed:rec,board:other,attached:o1+o2,rules:rec
