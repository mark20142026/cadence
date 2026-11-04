#!/usr/bin/env python3
"""Write a harness spec with many random-input attempts for one level.

Usage: fuzz_spec.py <level> <attempts> <out_pattern> [seed] [steps] [speed] > spec
`out_pattern` must contain %d (the attempt number). Inputs are a mix of short taps and long holds
at random gaps, which makes runs reach different blocks, spikes and portals before dying.
"""
import random
import sys


def main():
    level = int(sys.argv[1])
    attempts = int(sys.argv[2])
    out = sys.argv[3]
    rng = random.Random(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
    steps = int(sys.argv[5]) if len(sys.argv) > 5 else 9000
    speed = int(sys.argv[6]) if len(sys.argv) > 6 else 8
    print(f"level {level}\nsteps {steps}\nspeed {speed}\nout {out}")
    for _ in range(attempts):
        print("attempt")
        step = rng.randint(20, 200)
        while step < steps:
            hold = rng.choice([1, 1, 2, 3, 8, 20, 40, 90])
            print(f"press {step} 1")
            print(f"release {step + hold} 1")
            step += hold + rng.choice([5, 15, 30, 45, 60, 90, 120, 200])


if __name__ == "__main__":
    main()
